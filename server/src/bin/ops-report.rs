use anyhow::bail;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut require_clean_migrations = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--check-migrations" => require_clean_migrations = true,
            _ => bail!("unsupported ops-report argument"),
        }
    }
    lifeledger_server::run_ops_report(require_clean_migrations).await
}
