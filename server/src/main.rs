mod character;
mod error;
mod routes;
mod state;
mod store;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Router;
use sqlx::mysql::MySqlPoolOptions;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
/// 홈서버 한 대에 여러 서비스가 같은 MySQL 을 쓴다. 커넥션을 넉넉히 잡지 않는다.
const MAX_DB_CONNECTIONS: u32 = 8;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let pool = connect_database().await?;

    // 스키마는 서버가 기동하면서 맞춘다 (§4.4)
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("마이그레이션 적용에 실패했습니다")?;
    tracing::info!("마이그레이션 적용 완료");

    let store = store::create_mysql_save_store(pool).await?;
    let state = state::AppState::new(Arc::new(store));

    let app = Router::new()
        .merge(routes::router(state))
        .layer(TraceLayer::new_for_http());

    let addr = bind_addr()?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    tracing::info!("listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("lifeledger_server=debug,tower_http=debug,axum=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// DB 는 필수다. 주소가 없으면 게임 상태를 둘 곳이 없으므로 기동하지 않는다.
async fn connect_database() -> anyhow::Result<sqlx::MySqlPool> {
    let url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL 이 없습니다 — server/deploy/app.env.example 을 보세요")?;

    let pool = MySqlPoolOptions::new()
        .max_connections(MAX_DB_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await
        .context("MySQL 에 연결하지 못했습니다")?;

    tracing::info!("MySQL 연결됨");

    Ok(pool)
}

/// 바인드 주소는 `BIND_ADDR` 환경변수로 덮어쓴다 (기본값 `127.0.0.1:8080`).
fn bind_addr() -> anyhow::Result<SocketAddr> {
    let raw = std::env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());
    raw.parse()
        .with_context(|| format!("invalid BIND_ADDR: {raw}"))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }

    tracing::info!("shutdown signal received");
}
