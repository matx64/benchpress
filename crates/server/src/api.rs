//! JSON API under `/api/v1`.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use benchpress_core::BenchResult;
use benchpress_core::hash::result_fingerprint;
use benchpress_core::validate::{Rejection, validate_payload_size, validate_result};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::db::{self, Inserted};

fn reject(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(Rejection {
            code: code.into(),
            message: message.into(),
        }),
    )
        .into_response()
}

fn internal(e: &anyhow::Error) -> Response {
    tracing::error!(error = %e, "request failed");
    reject(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "something went wrong on the server",
    )
}

/// Ten base58 characters.
pub fn public_id() -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let bytes: [u8; 16] = rand::random();
    bytes
        .iter()
        .take(10)
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect()
}

fn sha_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Current UTC time, RFC 3339.
pub fn now() -> String {
    time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// `POST /api/v1/results`.
pub async fn create(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    if let Err(r) = validate_payload_size(body.len()) {
        return reject(StatusCode::UNPROCESSABLE_ENTITY, &r.code, r.message);
    }
    let result: BenchResult = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return reject(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_json",
                format!("the body is not a valid result document: {e}"),
            );
        }
    };
    let flags = match validate_result(&result, &state.registry) {
        Ok(f) => f,
        Err(r) => return reject(StatusCode::UNPROCESSABLE_ENTITY, &r.code, r.message),
    };
    let fingerprint = match result_fingerprint(&result) {
        Ok(f) => f,
        Err(e) => return internal(&e.into()),
    };
    let token = hex::encode(rand::random::<[u8; 32]>());
    let mut id = public_id();
    loop {
        match db::insert(
            &state.pool,
            &id,
            &fingerprint,
            &sha_hex(&token),
            &now(),
            &result,
            &flags,
        )
        .await
        {
            Ok(Inserted::Ok) => break,
            Ok(Inserted::IdTaken) => id = public_id(),
            Ok(Inserted::Duplicate) => {
                return reject(
                    StatusCode::CONFLICT,
                    "duplicate",
                    "this exact result was already submitted",
                );
            }
            Err(e) => return internal(&e),
        }
    }
    if let Err(e) = state.refresh().await {
        tracing::error!(error = %e, "snapshot refresh failed after insert");
    }
    let url = format!("{}/r/{id}", state.config.public_url);
    tracing::info!(id, suite = %result.suite.id, "result stored");
    (
        StatusCode::CREATED,
        Json(json!({ "id": id, "url": url, "delete_token": token })),
    )
        .into_response()
}

/// Replace axum's plain-text 413 with the API's JSON rejection.
pub async fn json_too_large(res: Response) -> Response {
    if res.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return reject(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "the result document is too large",
        );
    }
    res
}

/// `GET /api/v1/results/{id}`.
pub async fn show(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match db::get(&state.pool, &id).await {
        Ok(Some(s)) => Json(s.result).into_response(),
        Ok(None) => reject(StatusCode::NOT_FOUND, "not_found", "no result with that id"),
        Err(e) => internal(&e),
    }
}

/// `DELETE /api/v1/results/{id}` with the delete token or the admin token.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return reject(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "send `Authorization: Bearer <delete token>`",
        );
    };
    let stored = match db::delete_token_sha(&state.pool, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return reject(StatusCode::NOT_FOUND, "not_found", "no result with that id"),
        Err(e) => return internal(&e),
    };
    let presented = sha_hex(token);
    let is_admin = state
        .config
        .admin_token_sha
        .as_deref()
        .is_some_and(|a| a == presented);
    if presented != stored && !is_admin {
        return reject(
            StatusCode::FORBIDDEN,
            "forbidden",
            "that token cannot delete this result",
        );
    }
    if let Err(e) = db::delete(&state.pool, &id).await {
        return internal(&e);
    }
    state.og_cache.forget(&id);
    if let Err(e) = state.refresh().await {
        tracing::error!(error = %e, "snapshot refresh failed after delete");
    }
    tracing::info!(id, admin = is_admin, "result unpublished");
    StatusCode::NO_CONTENT.into_response()
}

/// `GET /api/v1/suites`.
pub async fn suites(State(state): State<Arc<AppState>>) -> Response {
    let list: Vec<_> = state
        .registry
        .all()
        .map(|s| {
            json!({
                "id": s.manifest.id,
                "version": s.manifest.version,
                "hash": s.hash,
                "tier": s.manifest.tier,
                "display_name": s.manifest.display_name,
            })
        })
        .collect();
    Json(list).into_response()
}
