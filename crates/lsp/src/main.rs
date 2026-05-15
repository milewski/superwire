use superwire_lsp::server::LanguageServer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    colog::init();

    LanguageServer::run_stdio()?;

    Ok(())
}
