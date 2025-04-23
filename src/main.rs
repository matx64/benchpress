mod config;
mod log;

use config::Args;
use log::{result_log, start_log, ulimit_log};
use reqwest::{Client, Error};
use std::{
    error::Error as StdError,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::Semaphore, time::sleep};

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
    let url = args.url.as_str();
    let semaphore = Arc::new(Semaphore::new(args.concurrency));

    let mut results: Vec<RequestResult> = Vec::with_capacity(args.requests);

    for batch in (0..args.requests).step_by(args.concurrency) {
        let requests_in_batch = std::cmp::min(args.concurrency, args.requests - batch);
        let mut handles = Vec::with_capacity(requests_in_batch);

        for _ in 0..requests_in_batch {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let client = client.clone();
            let url = url.to_string();

            let handle = tokio::spawn(async move {
                let result = request(&client, &url).await;
                drop(permit);
                result
            });

            handles.push(handle);
        }

        for handle in handles {
            results.push(handle.await.unwrap());
        }

        sleep(Duration::from_millis(10)).await;
    }

    show_results(results);
}

async fn request(client: &Client, url: &str) -> RequestResult {
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

    for result in &results {
        total_duration += result.duration_ms;
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
