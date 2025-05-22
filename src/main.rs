mod args;
mod config;
mod log;
mod result;

use config::Config;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressDrawTarget};
use log::{error_log, result_log, start_log};
use reqwest::StatusCode;
use result::{ExecutionResult, RequestResult};
use std::{sync::Arc, time::Instant};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = args::init();
    let cfg = config::init(args);
    start_log(&cfg);
    execute(cfg).await;
}

async fn execute(cfg: Arc<Config>) {
    let pb = ProgressBar::with_draw_target(Some(cfg.requests as u64), ProgressDrawTarget::stdout());

    let mut results: Vec<RequestResult> = Vec::with_capacity(cfg.requests);

    for batch_threshold in (0..cfg.requests).step_by(cfg.concurrency) {
        let batch_size = std::cmp::min(cfg.concurrency, cfg.requests - batch_threshold);

        let mut futures = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            let cfg = cfg.clone();
            futures.push(tokio::spawn(async move { send_request(cfg).await }));
        }

        let batch_results = join_all(futures).await;

        for result in batch_results {
            match result {
                Ok(result) => results.push(result),
                Err(err) => eprintln!("Join future error: {}", err),
            }
            pb.inc(1);
        }
    }

    pb.finish();
    let execution_result = ExecutionResult::new().init(results);
    result_log(execution_result);
}

async fn send_request(cfg: Arc<Config>) -> RequestResult {
    let start = Instant::now();
    let response = cfg
        .client
        .request(cfg.method.clone(), &cfg.url)
        .body(cfg.body.clone())
        .send()
        .await;
    let duration_ms = start.elapsed().as_millis();

    match response {
        Ok(response) => RequestResult::new(response.status(), duration_ms),
        Err(err) if err.is_timeout() => {
            RequestResult::new(StatusCode::REQUEST_TIMEOUT, duration_ms)
        }
        Err(err) => {
            error_log(err);
            std::process::exit(1);
        }
    }
}
