use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use sqlx::mysql::MySqlConnectOptions;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use url::Url;

struct DatabaseTarget {
    host: String,
    port: u16,
}

/// Builds MySQL connection options and enables the bounded transport workaround when requested.
pub async fn mysql_connect_options(
    raw_url: &str,
    tcp_nodelay_proxy_enabled: bool,
) -> Result<MySqlConnectOptions> {
    let options = MySqlConnectOptions::from_str(raw_url)
        .context("DATABASE_URL is not a valid MySQL connection URL")?;
    if !tcp_nodelay_proxy_enabled {
        return Ok(options);
    }

    let target = parse_target(raw_url)?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("failed to bind the database TCP_NODELAY proxy")?;
    let local_addr = listener
        .local_addr()
        .context("failed to read the database TCP_NODELAY proxy address")?;
    tokio::spawn(serve_proxy(listener, Arc::new(target)));

    tracing::info!(
        port = local_addr.port(),
        "database TCP_NODELAY proxy enabled"
    );

    Ok(options.host("127.0.0.1").port(local_addr.port()))
}

fn parse_target(raw_url: &str) -> Result<DatabaseTarget> {
    let url = Url::parse(raw_url).context("DATABASE_URL is not a valid URL")?;
    ensure!(
        url.scheme() == "mysql",
        "DATABASE_URL must use the mysql scheme"
    );
    ensure!(
        !url.query_pairs().any(|(key, _)| key == "socket"),
        "DATABASE_TCP_NODELAY_PROXY does not support Unix socket URLs"
    );
    ensure!(
        !url.query_pairs().any(|(key, value)| {
            key == "ssl-mode"
                && value
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric())
                    .collect::<String>()
                    .eq_ignore_ascii_case("verifyidentity")
        }),
        "DATABASE_TCP_NODELAY_PROXY does not support TLS hostname verification"
    );

    let host = url
        .host_str()
        .context("DATABASE_URL has no MySQL host")?
        .to_owned();
    let port = url
        .port_or_known_default()
        .context("DATABASE_URL has no MySQL port")?;

    Ok(DatabaseTarget { host, port })
}

async fn serve_proxy(listener: TcpListener, target: Arc<DatabaseTarget>) {
    loop {
        match listener.accept().await {
            Ok((client, _peer)) => {
                let target = target.clone();
                tokio::spawn(async move {
                    if let Err(error) = proxy_connection(client, &target).await {
                        tracing::warn!(?error, "database TCP proxy connection failed");
                    }
                });
            }
            Err(error) => {
                tracing::warn!(?error, "database TCP proxy accept failed");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn proxy_connection(mut client: TcpStream, target: &DatabaseTarget) -> Result<()> {
    client
        .set_nodelay(true)
        .context("failed to configure the database proxy client socket")?;
    let mut upstream = TcpStream::connect((target.host.as_str(), target.port))
        .await
        .context("failed to connect the database proxy upstream")?;
    upstream
        .set_nodelay(true)
        .context("failed to configure the database proxy upstream socket")?;
    copy_bidirectional(&mut client, &mut upstream)
        .await
        .context("database proxy stream failed")?;

    Ok(())
}
