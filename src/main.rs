mod config;
mod log;
mod result;

use config::Args;
use futures::future::join_all;
use log::{result_log, start_log, ulimit_log};
use reqwest::{Client, Error};
use result::{ExecutionResult, RequestResult};
use std::{error::Error as StdError, sync::Arc, time::Instant};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let (args, client) = config::init();

    start_log(&args);
    execute(client, args).await;
}

async fn execute(client: Client, args: Args) {
    let url = Arc::new(args.url);
    let mut results: Vec<RequestResult> = Vec::with_capacity(args.requests);

    for batch_threshold in (0..args.requests).step_by(args.concurrency) {
        let batch_size = std::cmp::min(args.concurrency, args.requests - batch_threshold);

        let futures = (0..batch_size).map(|_| {
            let client = client.clone();
            let url = url.clone();

            tokio::spawn(async move { send_request(client, &url).await })
        });

        let batch_results = join_all(futures).await;

        for result in batch_results {
            match result {
                Ok(result) => results.push(result),
                Err(err) => eprintln!("Join future error: {}", err),
            }
        }
    }

    show_results(results);
}

async fn send_request(client: Client, url: &str) -> RequestResult {
    let start = Instant::now();
    let response = client.get(url).send().await;
    let duration_ms = start.elapsed().as_millis();

    match response {
        Ok(response) => RequestResult::new(response.status(), duration_ms),
        Err(err) => {
            check_ulimit_error(err);
            std::process::exit(1);
        }
    }
}

fn show_results(results: Vec<RequestResult>) {
    let execution_result = ExecutionResult::new().init(results);
    result_log(execution_result);
}

fn check_ulimit_error(err: Error) {
    if let Some(err) = err.source() {
        if let Some(err) = err.source() {
            if err.to_string().contains("Too many open files") {
                ulimit_log();
            }
        } else {
            println!("Error: {}", err);
        }
    } else {
        println!("Error: {}", err);
    }
}
