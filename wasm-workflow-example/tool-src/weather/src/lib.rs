use serde::Deserialize;
use serde_json::{json, Value};
use std::mem;
use std::ptr;

#[derive(Debug, Deserialize)]
struct WeatherInput {
    city: String,
}

#[link(wasm_import_module = "superwire")]
extern "C" {
    fn host_http_get(url_pointer: i32, url_length: i32) -> i64;
}

#[no_mangle]
pub extern "C" fn tool_alloc(allocation_length: i32) -> i32 {
    if allocation_length <= 0 {
        return 0;
    }

    let allocation_length = match usize::try_from(allocation_length) {
        Ok(allocation_length) => allocation_length,
        Err(_) => return 0,
    };

    let mut allocation_buffer = Vec::<u8>::with_capacity(allocation_length);
    let allocation_pointer = allocation_buffer.as_mut_ptr();

    mem::forget(allocation_buffer);

    i32::try_from(allocation_pointer as usize).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn tool_definition() -> i64 {
    write_json_value(json!({
        "name": "weather",
        "description": "Fetches current weather from wttr.in",
        "parameters_schema": {
            "type": "object",
            "properties": {
                "city": {
                    "type": "string"
                }
            },
            "required": ["city"]
        }
    }))
}

#[no_mangle]
pub extern "C" fn tool_execute(input_pointer: i32, input_length: i32) -> i64 {
    let input_json = match read_json_input(input_pointer, input_length) {
        Ok(input_json) => input_json,
        Err(error_message) => return write_json_value(json!({ "error": error_message })),
    };

    let weather_input = match serde_json::from_str::<WeatherInput>(&input_json) {
        Ok(weather_input) => weather_input,
        Err(error) => return write_json_value(json!({ "error": format!("invalid input payload: {error}") })),
    };

    let encoded_city_name = encode_url_path_segment(weather_input.city.trim());
    let weather_service_url = format!("https://wttr.in/{encoded_city_name}?format=%C+%t");

    let weather_service_response = match invoke_host_http_get(&weather_service_url) {
        Ok(weather_service_response) => weather_service_response,
        Err(error_message) => return write_json_value(json!({ "error": error_message })),
    };

    let weather_summary = weather_service_response.trim();

    if weather_summary.is_empty() {
        return write_json_value(json!({
            "error": "weather service returned an empty response"
        }));
    }

    write_json_value(json!({
        "city": weather_input.city,
        "summary": weather_summary,
        "source": "wttr.in"
    }))
}

fn invoke_host_http_get(request_url: &str) -> Result<String, String> {
    let response_slice = unsafe {
        host_http_get(
            i32::try_from(request_url.as_ptr() as usize).map_err(|_| "request url pointer does not fit i32".to_string())?,
            i32::try_from(request_url.len()).map_err(|_| "request url length does not fit i32".to_string())?,
        )
    };

    if response_slice <= 0 {
        return Err("host http request failed".to_string());
    }

    let (response_pointer, response_length) = unpack_pointer_and_length(response_slice)?;
    let response_slice = unsafe { std::slice::from_raw_parts(response_pointer as *const u8, response_length) };

    String::from_utf8(response_slice.to_vec()).map_err(|error| format!("host http response is not valid utf-8: {error}"))
}

fn read_json_input(input_pointer: i32, input_length: i32) -> Result<String, String> {
    if input_pointer <= 0 {
        return Err("input pointer must be positive".to_string());
    }

    if input_length < 0 {
        return Err("input length cannot be negative".to_string());
    }

    let input_pointer = usize::try_from(input_pointer).map_err(|_| "input pointer is invalid".to_string())?;
    let input_length = usize::try_from(input_length).map_err(|_| "input length is invalid".to_string())?;
    let input_slice = unsafe { std::slice::from_raw_parts(input_pointer as *const u8, input_length) };

    String::from_utf8(input_slice.to_vec()).map_err(|error| format!("input payload is not valid utf-8: {error}"))
}

fn write_json_value(json_value: Value) -> i64 {
    let serialized_json = match serde_json::to_string(&json_value) {
        Ok(serialized_json) => serialized_json,
        Err(_) => return 0,
    };

    let serialized_bytes = serialized_json.as_bytes();

    let output_length = match i32::try_from(serialized_bytes.len()) {
        Ok(output_length) => output_length,
        Err(_) => return 0,
    };

    let output_pointer = tool_alloc(output_length);

    if output_pointer <= 0 {
        return 0;
    }

    unsafe {
        ptr::copy_nonoverlapping(serialized_bytes.as_ptr(), output_pointer as *mut u8, serialized_bytes.len());
    }

    pack_pointer_and_length(output_pointer, output_length)
}

fn encode_url_path_segment(path_segment: &str) -> String {
    let mut encoded = String::new();

    for byte in path_segment.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            encoded.push(char::from(byte));

            continue;
        }

        encoded.push('%');
        encoded.push_str(&format!("{byte:02X}"));
    }

    encoded
}

fn unpack_pointer_and_length(encoded_slice: i64) -> Result<(usize, usize), String> {
    let encoded_slice = u64::try_from(encoded_slice).map_err(|_| "encoded response slice is negative".to_string())?;

    let response_pointer = usize::try_from(encoded_slice >> 32).map_err(|_| "encoded response pointer does not fit usize".to_string())?;

    let response_length =
        usize::try_from(encoded_slice & 0xFFFF_FFFF).map_err(|_| "encoded response length does not fit usize".to_string())?;

    Ok((response_pointer, response_length))
}

fn pack_pointer_and_length(output_pointer: i32, output_length: i32) -> i64 {
    let output_pointer = match u32::try_from(output_pointer) {
        Ok(output_pointer) => output_pointer,
        Err(_) => return 0,
    };

    let output_length = match u32::try_from(output_length) {
        Ok(output_length) => output_length,
        Err(_) => return 0,
    };

    i64::try_from((u64::from(output_pointer) << 32) | u64::from(output_length)).unwrap_or(0)
}
