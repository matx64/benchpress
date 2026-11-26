//! `benchpress-server`: the website and API. One binary, one SQLite file.

pub mod api;
pub mod assets;
pub mod charts;
pub mod db;
pub mod og;
pub mod pages;
pub mod seed;
pub mod stats;
pub mod views;

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use benchpress_core::Registry;
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

/// Server configuration from the environment.
#[derive(Debug, Clone)]
pub struct Config {
    pub db: String,
    pub bind: SocketAddr,
    pub public_url: String,
    pub admin_token_sha: Option<String>,
    pub json_logs: bool,
}

impl Config {
    /// Read `BENCHPRESS_*` variables.
    pub fn from_env() -> Result<Config> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        if var("BENCHPRESS_ADMIN_TOKEN").is_some_and(|t| t.len() < 32) {
            anyhow::bail!("BENCHPRESS_ADMIN_TOKEN must be at least 32 characters");
        }
        Ok(Config {
            db: var("BENCHPRESS_DB").unwrap_or_else(|| "benchpress.db".into()),
            bind: var("BENCHPRESS_BIND")
                .unwrap_or_else(|| "0.0.0.0:8080".into())
                .parse()
                .context("BENCHPRESS_BIND")?,
            // TODO(decision): final domain.
            public_url: var("BENCHPRESS_PUBLIC_URL")
                .unwrap_or_else(|| "http://localhost:8080".into())
                .trim_end_matches('/')
                .to_owned(),
            admin_token_sha: var("BENCHPRESS_ADMIN_TOKEN")
                .map(|t| hex::encode(Sha256::digest(t.as_bytes()))),
            json_logs: var("BENCHPRESS_LOG_FORMAT").is_some_and(|f| f == "json"),
        })
    }
}

/// Shared state for every handler.
pub struct AppState {
    pub pool: SqlitePool,
    pub registry: Registry,
    pub config: Config,
    pub snapshot: RwLock<Arc<stats::Snapshot>>,
    refresh_lock: tokio::sync::Mutex<()>,
    pub views: views::Views,
    pub og_cache: og::Cache,
}

impl AppState {
    /// Build state and load the first snapshot.
    pub async fn new(config: Config, pool: SqlitePool) -> Result<Arc<AppState>> {
        let og_dir = std::path::Path::new(&config.db).with_extension("og");
        let state = Arc::new(AppState {
            pool,
            registry: Registry::builtin(),
            views: views::Views::new(&config.public_url),
            og_cache: og::Cache::new(og_dir),
            snapshot: RwLock::new(Arc::default()),
            refresh_lock: tokio::sync::Mutex::new(()),
            config,
        });
        state.refresh().await?;
        Ok(state)
    }

    /// Reload the snapshot after a write.
    pub async fn refresh(&self) -> Result<()> {
        let _guard = self.refresh_lock.lock().await;
        let mut rows = db::all_rows(&self.pool).await?;
        stats::mark_outliers(&mut rows);
        *self.snapshot.write().expect("snapshot lock") = Arc::new(stats::Snapshot { rows });
        Ok(())
    }

    /// Current snapshot.
    pub fn snap(&self) -> Arc<stats::Snapshot> {
        self.snapshot.read().expect("snapshot lock").clone()
    }
}

/// Rate-limit key: the client IP from Fly's `Fly-Client-IP`, else the peer address, widened to its /64 for IPv6 since one host usually owns the whole prefix.
#[derive(Clone, Copy)]
struct NetworkKey;

impl tower_governor::key_extractor::KeyExtractor for NetworkKey {
    type Key = std::net::IpAddr;

    fn extract<T>(
        &self,
        req: &axum::http::Request<T>,
    ) -> Result<Self::Key, tower_governor::GovernorError> {
        let ip = req
            .headers()
            .get("fly-client-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<std::net::IpAddr>().ok())
            .or_else(|| {
                req.extensions()
                    .get::<axum::extract::ConnectInfo<SocketAddr>>()
                    .map(|c| c.0.ip())
            })
            .ok_or(tower_governor::GovernorError::UnableToExtractKey)?;
        Ok(match ip.to_canonical() {
            std::net::IpAddr::V6(ip) => std::net::IpAddr::V6(std::net::Ipv6Addr::from(
                ip.to_bits() & !((1u128 << 64) - 1),
            )),
            ip @ std::net::IpAddr::V4(_) => ip,
        })
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = term => {}
    }
}

/// Build the router.
pub fn app(state: Arc<AppState>, rate_limit: bool) -> Router {
    let mut submit = post(api::create)
        .layer::<_, std::convert::Infallible>(axum::middleware::map_response(api::json_too_large))
        .layer(DefaultBodyLimit::max(
            benchpress_core::validate::MAX_PAYLOAD_BYTES + 1024,
        ));
    if rate_limit {
        let governor = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(360)
                .burst_size(10)
                .key_extractor(NetworkKey)
                .finish()
                .expect("valid governor config"),
        );
        let limiter = governor.limiter().clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
            loop {
                tick.tick().await;
                limiter.retain_recent();
            }
        });
        submit = submit.layer(GovernorLayer::new(governor));
    }
    Router::new()
        .route("/api/v1/results", submit)
        .route("/api/v1/results/{id}", get(api::show).delete(api::delete))
        .route("/api/v1/suites", get(api::suites))
        .route("/healthz", get(|| async { "ok" }))
        .merge(pages::routes())
        .merge(assets::routes())
        .fallback(pages::not_found)
        .layer(CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<_>| {
                    tracing::info_span!("http", method = %req.method(), path = %req.uri().path())
                })
                .on_response(|res: &axum::http::Response<_>, latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::info!(status = res.status().as_u16(), ms = latency.as_millis() as u64, "response");
                }),
        )
        .with_state(state)
}

#[derive(Parser)]
#[command(
    name = "benchpress-server",
    version,
    about = "Benchpress website and API"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve the website and API (default)
    Serve,
    /// Insert fake results for local design work. Never run against production.
    Seed {
        /// Number of results
        #[arg(long, default_value_t = 36)]
        count: usize,
    },
}

fn init_logs(json: bool) {
    let filter = tracing_subscriber::EnvFilter::try_from_env("BENCHPRESS_LOG")
        .unwrap_or_else(|_| "info,sqlx=warn,tower_governor=warn".into());
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if json {
        builder.json().init();
    } else {
        builder.init();
    }
}

/// Entry point used by the binary.
pub async fn main_entry() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::from_env()?;
    init_logs(config.json_logs);
    let pool = db::open(&config.db)
        .await
        .with_context(|| format!("opening {}", config.db))?;
    match cli.command.unwrap_or(Cmd::Serve) {
        Cmd::Seed { count } => seed::run(&pool, count).await,
        Cmd::Serve => {
            let bind = config.bind;
            let state = AppState::new(config, pool).await?;
            let refresher = Arc::clone(&state);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
                tick.tick().await;
                loop {
                    tick.tick().await;
                    if let Err(e) = refresher.refresh().await {
                        tracing::error!(error = %e, "periodic snapshot refresh failed");
                    }
                }
            });
            let listener = tokio::net::TcpListener::bind(bind).await?;
            tracing::info!(%bind, "listening");
            axum::serve(
                listener,
                app(state, true).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown_signal())
            .await?;
            Ok(())
        }
    }
}
