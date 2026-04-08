use php_tools::php_weather::{PhpWeatherBoundInput, PhpWeatherInput, PhpWeatherOutput};
use superwire_core::{tool, Tool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let city_name = std::env::var("CITY").unwrap_or_else(|_| "Madrid".to_string());
    let internal_token = std::env::var("SUPERWIRE_INTERNAL_TOKEN").unwrap_or_else(|_| "dev-superwire-token".to_string());

    let php_weather_tool: Tool<PhpWeatherInput, PhpWeatherOutput, PhpWeatherBoundInput> = tool!("../tools/php_weather.wasm")?;

    println!("tool definition: {:#?}", php_weather_tool.definition());

    let tool_output = php_weather_tool
        .run_with_bound_input(
            PhpWeatherInput {
                city: Some(city_name),
            },
            PhpWeatherBoundInput {
                city: None,
                internal_token: Some(internal_token),
            },
        )
        .await?;

    println!("tool output: {:#?}", tool_output);

    Ok(())
}
