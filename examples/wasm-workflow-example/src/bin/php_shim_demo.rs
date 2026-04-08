use superwire_core::{tool, Tool};
use tools::php_weather::{PhpWeatherBoundInput, PhpWeatherInput, PhpWeatherOutput};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let city_name = std::env::var("CITY").unwrap_or_else(|_| "Madrid".to_string());
    let normalized_city_name = city_name.strip_prefix("city=").unwrap_or(&city_name).to_string();

    let php_weather_tool: Tool<PhpWeatherInput, PhpWeatherOutput, PhpWeatherBoundInput> = tool!("../../tools/php_weather.wasm")?;

    println!("tool definition: {:#?}", php_weather_tool.definition());

    let tool_output = php_weather_tool
        .run_with_bound_input(
            PhpWeatherInput {
                city: Some(normalized_city_name),
            },
            PhpWeatherBoundInput {
                city: None,
                internal_token: Some("dev-superwire-token".to_string()),
            },
        )
        .await?;

    println!("tool output: {:#?}", tool_output);

    Ok(())
}
