use clap::Parser;
use std::net::SocketAddr;
use superwire_executor::{AgentCacheConfig, AgentCacheDriver, AgentCacheTimeToLive, RedisAgentCacheConfig};
use superwire_executor_server::serve_executor_with_agent_cache;

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
    let command = Cli::parse();
    let redis_config = RedisAgentCacheConfig::new(command.redis_host, command.redis_password, command.redis_database);
    let cache_config = AgentCacheConfig::new(command.cache_driver).with_redis(redis_config);

    log::info!("starting executor server on {}", command.address);

    serve_executor_with_agent_cache(
        command.address,
        command.disable_playground,
        cache_config,
        command.cache_time_to_live.0,
    )
    .await?;

    Ok(())
}
