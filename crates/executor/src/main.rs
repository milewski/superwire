use clap::Parser;
use std::net::SocketAddr;
use superwire_executor::{serve_executor_with_cache, AgentCacheDriver, AgentCacheTimeToLive};

#[derive(Debug, Parser)]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:13703")]
    address: SocketAddr,

    #[arg(long, default_value_t = false)]
    disable_playground: bool,

    #[arg(long, default_value_t = AgentCacheDriver::InMemory)]
    cache_driver: AgentCacheDriver,

    #[arg(long = "cache-ttl", default_value_t = AgentCacheTimeToLive::default())]
    cache_time_to_live: AgentCacheTimeToLive,
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

    serve_executor_with_cache(cli.address, cli.disable_playground, cli.cache_driver, cli.cache_time_to_live.0).await?;

    Ok(())
}
