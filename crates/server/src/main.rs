use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use horon_messenger_sdk::Client;

#[derive(Debug, Parser)]
#[command(
    name = "horon-messenger-server",
    version,
    about = "HTTP API for horon-messenger"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 8787)]
    port: u16,

    #[arg(long, env = "HORON_MESSENGER_CONFIG_DIR")]
    config_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let mut builder = Client::builder();
    if let Some(dir) = args.config_dir {
        builder = builder.config_dir(dir);
    }
    let client = Arc::new(builder.build()?);
    let app = horon_messenger_server::build_router(client);

    let listener = tokio::net::TcpListener::bind((args.host.as_str(), args.port)).await?;
    tracing::info!(
        "horon-messenger-server listening on http://{}:{}",
        args.host,
        args.port
    );
    axum::serve(listener, app).await?;
    Ok(())
}
