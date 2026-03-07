/// Simple test to verify Ollama connectivity

use ollama_rs::Ollama;
use ollama_rs::generation::completion::request::GenerationRequest;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Testing Ollama connectivity...\n");

    let url = url::Url::parse("http://100.76.5.36:11434")?;
    let client = Ollama::from_url(url);

    println!("Sending test request to qwen3:4b-instruct...");

    let request = GenerationRequest::new(
        "qwen3:4b-instruct".to_string(),
        "Say hello in one word.".to_string(),
    );

    match client.generate(request).await {
        Ok(response) => {
            println!("✓ Success!");
            println!("Response: {}", response.response);
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }

    Ok(())
}
