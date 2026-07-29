#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lifeledger_server::run_offline_worker().await
}
