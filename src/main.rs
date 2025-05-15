mod config;
mod log;

use config::Args;
use futures::future::join_all;
use log::{result_log, start_log, ulimit_log};
use reqwest::{Client, Error};
use std::{error::Error as StdError, sync::Arc, time::Instant};

struct RequestResult {
    code: u16,
    duration_ms: u128,
}

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
    let duration = start.elapsed().as_millis();

    match response {
        Ok(response) => RequestResult {
            code: response.status().as_u16(),
            duration_ms: duration,
        },
        Err(err) => {
            check_ulimit_error(err);
            std::process::exit(1);
        }
    }
}

fn show_results(results: Vec<RequestResult>) {
    let mut count_1xx: u32 = 0;
    let mut count_2xx: u32 = 0;
    let mut count_3xx: u32 = 0;
    let mut count_4xx: u32 = 0;
    let mut count_5xx: u32 = 0;

    let mut total_duration: u128 = 0;
    let mut fastest: u128 = u128::MAX;
    let mut slowest: u128 = u128::MIN;

    for result in &results {
        total_duration += result.duration_ms;
        fastest = std::cmp::min(fastest, result.duration_ms);
        slowest = std::cmp::max(slowest, result.duration_ms);

        match result.code {
            100..=199 => count_1xx += 1,
            200..=299 => count_2xx += 1,
            300..=399 => count_3xx += 1,
            400..=499 => count_4xx += 1,
            500..=599 => count_5xx += 1,
            _ => {}
        }
    }

    result_log(
        results,
        count_1xx,
        count_2xx,
        count_3xx,
        count_4xx,
        count_5xx,
        total_duration,
        fastest,
        slowest,
    );
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
