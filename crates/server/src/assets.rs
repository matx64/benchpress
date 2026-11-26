//! Static assets compiled into the binary, served with content-hashed names.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use sha2::{Digest, Sha256};

use crate::AppState;

struct Asset {
    bytes: std::borrow::Cow<'static, [u8]>,
    content_type: &'static str,
    hashed: String,
}

macro_rules! embed {
    ($path:literal, $ct:literal) => {
        (
            $path,
            std::borrow::Cow::Borrowed(&include_bytes!(concat!("../static/", $path))[..]),
            $ct,
        )
    };
}

static ASSETS: LazyLock<HashMap<&'static str, Asset>> = LazyLock::new(|| {
    let css = format!(
        "@layer reset, tokens, base, layout, components, pages, utilities;\n@layer tokens {{\n{}\n}}\n{}",
        include_str!("../static/tokens.css"),
        include_str!("../static/app.css")
    );
    let list: Vec<(&'static str, std::borrow::Cow<'static, [u8]>, &'static str)> = vec![
        (
            "app.css",
            std::borrow::Cow::Owned(css.into_bytes()),
            "text/css; charset=utf-8",
        ),
        embed!("fonts/archivo-latin-variable.woff2", "font/woff2"),
        embed!("icons/favicon.svg", "image/svg+xml"),
        embed!("icons/apple-touch-icon.png", "image/png"),
        embed!("icons/icon-192.png", "image/png"),
        embed!("icons/icon-512.png", "image/png"),
        embed!("icons/icon-maskable-512.png", "image/png"),
        embed!("favicon.ico", "image/x-icon"),
        embed!("site.webmanifest", "application/manifest+json"),
        embed!("social/og-default.png", "image/png"),
    ];
    list.into_iter()
        .map(|(name, bytes, content_type)| {
            let hash = hex::encode(&Sha256::digest(&bytes)[..6]);
            let hashed = match name.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem}.{hash}.{ext}"),
                None => format!("{name}.{hash}"),
            };
            (
                name,
                Asset {
                    bytes,
                    content_type,
                    hashed,
                },
            )
        })
        .collect()
});

static BY_HASHED: LazyLock<HashMap<String, &'static str>> = LazyLock::new(|| {
    ASSETS
        .iter()
        .map(|(name, a)| (a.hashed.clone(), *name))
        .collect()
});

/// Hashed URL for an asset, e.g. `/static/app.3f2a1b9c0d1e.css`.
pub fn url(name: &str) -> String {
    ASSETS.get(name).map_or_else(
        || format!("/static/{name}"),
        |a| format!("/static/{}", a.hashed),
    )
}

fn serve(name: &str, immutable: bool) -> Response {
    let Some(a) = ASSETS.get(name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let cache = if immutable {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=86400"
    };
    (
        [
            (header::CONTENT_TYPE, a.content_type),
            (header::CACHE_CONTROL, cache),
        ],
        a.bytes.clone(),
    )
        .into_response()
}

/// Where the launcher is fetched from. Switch to
/// `https://github.com/matx64/benchpress/releases/latest/download/run.sh` after the first release.
pub const RUN_URL: &str = "https://raw.githubusercontent.com/matx64/benchpress/main/run.sh";

/// The launcher's source, for reading before running it.
pub const SCRIPT_SOURCE_URL: &str = "https://github.com/matx64/benchpress/blob/main/run.sh";

/// Asset routes.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/static/{*path}",
            get(|Path(path): Path<String>| async move {
                match BY_HASHED.get(&path) {
                    Some(name) => serve(name, true),
                    None => StatusCode::NOT_FOUND.into_response(),
                }
            }),
        )
        .route(
            "/favicon.ico",
            get(|| async { serve("favicon.ico", false) }),
        )
        .route(
            "/site.webmanifest",
            get(|| async { serve("site.webmanifest", false) }),
        )
        .route(
            "/icons/{name}",
            get(|Path(name): Path<String>| async move { serve(&format!("icons/{name}"), false) }),
        )
        .route(
            "/social/og-default.png",
            get(|| async { serve("social/og-default.png", false) }),
        )
        .route("/robots.txt", get(|| async { "User-agent: *\nAllow: /\n" }))
}
