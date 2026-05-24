use clap::Parser;
use std::net::SocketAddr;
use superwire_executor::{
    serve_executor_with_agent_cache, AgentCacheConfig, AgentCacheDriver, AgentCacheTimeToLive, RedisAgentCacheConfig,
};

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

    #[arg(long, default_value = "127.0.0.1:6379")]
    redis_host: String,

    #[arg(long)]
    redis_password: Option<String>,

    #[arg(long, default_value_t = 0)]
    redis_database: u8,
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
    let redis_config = RedisAgentCacheConfig::new(cli.redis_host, cli.redis_password, cli.redis_database);
    let cache_config = AgentCacheConfig::new(cli.cache_driver).with_redis(redis_config);

    log::info!("starting executor server on {}", cli.address);

    serve_executor_with_agent_cache(cli.address, cli.disable_playground, cache_config, cli.cache_time_to_live.0).await?;

    Ok(())
}
