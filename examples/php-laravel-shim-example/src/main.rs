use superwire_core::{tool, Tool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let city_name = std::env::var("CITY").unwrap_or_else(|_| "Madrid".to_string());
    let internal_token = std::env::var("SUPERWIRE_INTERNAL_TOKEN").unwrap_or_else(|_| "dev-superwire-token".to_string());

    let php_weather_tool: Tool<serde_json::Value, serde_json::Value, serde_json::Value> = tool!("../tools/php_weather.wasm")?;

    println!("tool definition: {:#?}", php_weather_tool.definition());

    let tool_output = php_weather_tool
        .run_with_bound_input(
            serde_json::json!({
                "city": city_name,
            }),
            serde_json::json!({
                "city": null,
                "internal_token": internal_token,
            }),
        )
        .await?;

    println!("tool output: {:#?}", tool_output);

    Ok(())
}
