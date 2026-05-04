use clap::Parser;
use std::net::SocketAddr;
use superwire_executor::serve_executor;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:3000")]
    address: SocketAddr,
}

#[tokio::main]
async fn main() {
    colog::init();

    if let Err(error) = run().await {
        log::error!("executor failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    log::info!("starting executor server on {}", cli.address);

    serve_executor(cli.address).await?;

    Ok(())
}
