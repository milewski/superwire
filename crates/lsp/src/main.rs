use superwire_lsp::server::LanguageServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    colog::init();

    LanguageServer::run_stdio().await?;

    Ok(())
}
