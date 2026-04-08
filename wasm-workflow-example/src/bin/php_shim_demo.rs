use superwire_core::{tool, Tool};
use tools::php_weather::{PhpWeatherInput, PhpWeatherOutput};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let city_name = std::env::var("CITY").unwrap_or_else(|_| "Madrid".to_string());
    let normalized_city_name = city_name.strip_prefix("city=").unwrap_or(&city_name).to_string();

    let php_weather_tool: Tool<PhpWeatherInput, PhpWeatherOutput> = tool!("../../tools/php_weather.wasm")?;

    println!("tool definition: {:#?}", php_weather_tool.definition());

    let tool_output = php_weather_tool
        .run(PhpWeatherInput {
            city: Some(normalized_city_name),
        })
        .await?;

    println!("tool output: {:#?}", tool_output);

    Ok(())
}
