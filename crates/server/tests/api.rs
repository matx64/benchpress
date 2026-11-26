use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use benchpress_server::{AppState, Config, app, db, seed};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

const CLIENT_IP: &str = "203.0.113.77";

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Logs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut logs) = self.0.lock() {
            logs.extend_from_slice(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Step {
    name: &'static str,
    method: Method,
    path: &'static str,
    token: Option<&'static str>,
    body: Option<&'static str>,
    status: StatusCode,
    code: Option<&'static str>,
}

#[tokio::test]
async fn submit_show_unpublish() {
    let logs = Logs::default();
    let sink = logs.clone();
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || sink.clone())
            .finish(),
    );
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("t.db").to_string_lossy().into_owned();
    let config = Config {
        db: db_path.clone(),
        bind: "127.0.0.1:0".parse().unwrap(),
        public_url: "http://test".into(),
        admin_token_sha: None,
        json_logs: false,
    };
    let state = AppState::new(config, db::open(&db_path).await.unwrap())
        .await
        .unwrap();
    let router = app(Arc::clone(&state), false);

    let (result, _) = seed::generate(1).remove(0);
    let valid = serde_json::to_string(&result).unwrap();
    let mut bad = result.clone();
    bad.scenarios[0].runs[0].wall_ms = 1;
    let bad = serde_json::to_string(&bad).unwrap();
    let mut huge = String::from("{\"pad\":\"");
    huge.push_str(&"x".repeat(benchpress_core::validate::MAX_PAYLOAD_BYTES));
    huge.push_str("\"}");

    let send = |method: Method, path: String, token: Option<String>, body: String| {
        let router = router.clone();
        async move {
            let mut req = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("x-forwarded-for", CLIENT_IP)
                .header("x-real-ip", CLIENT_IP);
            if let Some(t) = token {
                req = req.header("authorization", format!("Bearer {t}"));
            }
            let res = router
                .oneshot(req.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            let status = res.status();
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };

    let (status, created) = send(Method::POST, "/api/v1/results".into(), None, valid.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap().to_owned();
    let token = created["delete_token"].as_str().unwrap().to_owned();
    assert_eq!(created["url"], format!("http://test/r/{id}"));

    let steps = [
        Step {
            name: "duplicate",
            method: Method::POST,
            path: "/api/v1/results",
            token: None,
            body: Some("valid"),
            status: StatusCode::CONFLICT,
            code: Some("duplicate"),
        },
        Step {
            name: "implausible",
            method: Method::POST,
            path: "/api/v1/results",
            token: None,
            body: Some("bad"),
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: Some("implausible_wall_time"),
        },
        Step {
            name: "not json",
            method: Method::POST,
            path: "/api/v1/results",
            token: None,
            body: Some("{"),
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: Some("invalid_json"),
        },
        Step {
            name: "too large",
            method: Method::POST,
            path: "/api/v1/results",
            token: None,
            body: Some("huge"),
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: Some("payload_too_large"),
        },
        Step {
            name: "show",
            method: Method::GET,
            path: "/api/v1/results/{id}",
            token: None,
            body: None,
            status: StatusCode::OK,
            code: None,
        },
        Step {
            name: "page",
            method: Method::GET,
            path: "/r/{id}",
            token: None,
            body: None,
            status: StatusCode::OK,
            code: None,
        },
        Step {
            name: "delete without token",
            method: Method::DELETE,
            path: "/api/v1/results/{id}",
            token: None,
            body: None,
            status: StatusCode::UNAUTHORIZED,
            code: Some("unauthorized"),
        },
        Step {
            name: "delete with wrong token",
            method: Method::DELETE,
            path: "/api/v1/results/{id}",
            token: Some("wrong"),
            body: None,
            status: StatusCode::FORBIDDEN,
            code: Some("forbidden"),
        },
        Step {
            name: "delete with token",
            method: Method::DELETE,
            path: "/api/v1/results/{id}",
            token: Some("own"),
            body: None,
            status: StatusCode::NO_CONTENT,
            code: None,
        },
        Step {
            name: "show after delete",
            method: Method::GET,
            path: "/api/v1/results/{id}",
            token: None,
            body: None,
            status: StatusCode::NOT_FOUND,
            code: Some("not_found"),
        },
        Step {
            name: "page after delete",
            method: Method::GET,
            path: "/r/{id}",
            token: None,
            body: None,
            status: StatusCode::NOT_FOUND,
            code: None,
        },
        Step {
            name: "resubmit after delete",
            method: Method::POST,
            path: "/api/v1/results",
            token: None,
            body: Some("valid"),
            status: StatusCode::CONFLICT,
            code: Some("duplicate"),
        },
    ];

    for s in steps {
        let body = match s.body {
            Some("valid") => valid.clone(),
            Some("bad") => bad.clone(),
            Some("huge") => huge.clone(),
            Some(raw) => raw.to_owned(),
            None => String::new(),
        };
        let token = s.token.map(|t| {
            if t == "own" {
                token.clone()
            } else {
                t.to_owned()
            }
        });
        let (status, json) = send(s.method, s.path.replace("{id}", &id), token, body).await;
        assert_eq!(status, s.status, "{}: {json}", s.name);
        if let Some(code) = s.code {
            assert_eq!(json["code"], code, "{}: {json}", s.name);
        }
    }
    assert!(
        state.snap().rows.iter().all(|r| r.id != id),
        "unpublished result still in snapshot"
    );
    let logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("/api/v1/results"),
        "request logging is not captured"
    );
    assert!(!logs.contains(CLIENT_IP), "client IP leaked into logs");
}
