mod config;

use config::Args;

use reqwest::{Client, Error};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() {
    let (args, client) = config::init();

    println!("Starting benchmark...");
    println!("URL: {}", args.url);
    println!("Total Requests: {}", args.requests);

    execute(client, args).await;
}

async fn execute(client: Client, args: Args) {
    let url = args.url.as_str();
    let semaphore = Arc::new(Semaphore::new(args.concurrency));
    let mut handles = Vec::with_capacity(args.requests);

    for _ in 0..args.requests {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let url = url.to_string();

        let handle = tokio::spawn(async move {
            let result = req(&client, &url).await;
            drop(permit);
            result
        });

        handles.push(handle);
    }

    let mut ok_count = 0;
    let mut err_count = 0;

    for handle in handles {
        match handle.await.unwrap() {
            Ok(true) => ok_count += 1,
            _ => err_count += 1,
        }
    }

    println!("\n------ Results ------");
    println!("Total requests: {}", args.requests);
    println!("Successful requests: {}", ok_count);
    println!("Failed requests: {}", err_count);
}

async fn req(client: &Client, url: &str) -> Result<bool, Error> {
    let status = client.get(url).send().await?.status();
    Ok(!status.is_server_error() && !status.is_client_error())
}
