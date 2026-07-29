mod auth;
pub mod career;
mod character;
pub mod day;
mod error;
pub mod finance;
pub mod life;
pub mod market;
pub mod offline;
mod routes;
pub mod runs;
mod state;
mod store;
pub mod trading;
mod worker;

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
const DEFAULT_PUBLIC_ORIGIN: &str = "http://localhost:8080";
const MAX_DB_CONNECTIONS: u32 = 8;
pub(crate) const ENGINE_VERSION: &str = "m5a-dev-v1";

pub async fn run_api() -> anyhow::Result<()> {
    init_tracing();

    let pool = connect_database().await?;

    // The API is the deployment migration authority (§4.4, M5-D §6.4).
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("failed to apply migrations")?;
    tracing::info!("migrations applied");

    let providers = auth::Providers::from_env(public_origin())?;
    let market_generators = market::create_market_generator_registry()
        .context("failed to create the market generator registry")?;
    let markets = Arc::new(store::create_mysql_market_store(
        pool.clone(),
        market_generators,
    ));
    let finance_rules = finance::create_finance_rules();
    let saves = Arc::new(store::create_mysql_save_store(
        pool.clone(),
        finance_rules.clone(),
    ));
    let finances = Arc::new(store::create_mysql_finance_store(
        pool.clone(),
        finance_rules.clone(),
    ));
    let careers = Arc::new(store::create_mysql_career_store(
        pool.clone(),
        finance_rules.clone(),
    ));
    let lives = Arc::new(store::create_mysql_life_store(pool.clone(), finance_rules));
    let runs = Arc::new(store::create_mysql_run_store(pool.clone()));
    let offline_progress = Arc::new(store::create_mysql_offline_progress_store(
        pool.clone(),
        offline::create_offline_rules(),
    ));
    let users = store::create_mysql_user_store(pool);
    let games = day::create_daily_pipeline(
        saves.clone(),
        markets.clone(),
        careers.clone(),
        lives.clone(),
    );
    let app_stores = state::create_app_stores(state::AppStoreDependencies {
        games,
        trades: saves,
        finances: finances.clone(),
        cash_products: finances.clone(),
        assets: finances.clone(),
        tax_accounts: finances,
        careers,
        lives,
        markets,
        runs,
        offline_progress,
        users: Arc::new(users),
    });
    let state = state::AppState::new(app_stores, providers);

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

pub async fn run_offline_worker() -> anyhow::Result<()> {
    init_tracing();
    worker::run(connect_database().await?).await
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("lifeledger_server=debug,tower_http=debug,axum=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn connect_database() -> anyhow::Result<sqlx::MySqlPool> {
    let url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL is not set - see server/deploy/app.env.example")?;
    let pool = MySqlPoolOptions::new()
        .max_connections(MAX_DB_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                // Canonical catalog projections exceed MySQL's 1 KiB aggregation default.
                sqlx::query("SET SESSION group_concat_max_len = 1048576")
                    .execute(&mut *connection)
                    .await?;
                // Offline accrual and lease authority is always represented in UTC (§6.4).
                sqlx::query("SET SESSION time_zone = '+00:00'")
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .context("failed to connect to MySQL")?;

    tracing::info!("connected to MySQL");

    Ok(pool)
}

fn public_origin() -> String {
    let raw = std::env::var("PUBLIC_ORIGIN").unwrap_or_else(|_| DEFAULT_PUBLIC_ORIGIN.to_string());

    raw.trim_end_matches('/').to_owned()
}

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
