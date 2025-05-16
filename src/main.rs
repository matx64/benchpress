mod config;
mod log;
mod result;

use config::Args;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressDrawTarget};
use log::{error_log, result_log, start_log};
use reqwest::{Client, StatusCode};
use result::{ExecutionResult, RequestResult};
use std::{sync::Arc, time::Instant};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let (args, client) = config::init();
    start_log(&args);
    execute(client, args).await;
}

async fn execute(client: Client, args: Args) {
    let mut results: Vec<RequestResult> = Vec::with_capacity(args.requests);
    let pb =
        ProgressBar::with_draw_target(Some(args.requests as u64), ProgressDrawTarget::stdout());
    let url = Arc::new(args.url);
    let client = Arc::new(client);

    for batch_threshold in (0..args.requests).step_by(args.concurrency) {
        let batch_size = std::cmp::min(args.concurrency, args.requests - batch_threshold);

        let mut futures = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            let client = client.clone();
            let url = url.clone();
            futures.push(tokio::spawn(
                async move { send_request(client, &url).await },
            ));
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

async fn send_request(client: Arc<Client>, url: &str) -> RequestResult {
    let start = Instant::now();
    let response = client.get(url).send().await;
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
