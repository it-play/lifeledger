#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lifeledger_server::run_market_data_sync().await
}
