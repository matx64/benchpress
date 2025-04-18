mod config;

use config::Args;
use reqwest::{Client, Error};

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

    let mut ok_count = 0;
    let mut err_count = 0;

    for _ in 0..args.requests {
        match req(&client, url).await {
            Ok(true) => ok_count += 1,
            _ => err_count += 1,
        }
    }

    println!("\n------ Results ------");
    println!("Successful requests: {}", ok_count);
    println!("Failed requests: {}", err_count);
}

async fn req(client: &Client, url: &str) -> Result<bool, Error> {
    let status = client.get(url).send().await?.status();
    Ok(!status.is_server_error() && !status.is_client_error())
}
