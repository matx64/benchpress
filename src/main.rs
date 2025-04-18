use clap::Parser;
use reqwest::{Client, Error};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Target URL to benchmark
    #[arg(short, long)]
    url: String,

    /// Total number of requests
    #[arg(short, long)]
    requests: u32,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    println!("Starting benchmark...");
    println!("URL: {}", args.url);
    println!("Total Requests: {}", args.requests);

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to build Client");

    execute(&client, args).await;
}

async fn execute(client: &Client, args: Args) {
    let mut ok_count = 0;
    let mut err_count = 0;

    for _ in 0..args.requests {
        match req(&client, &args.url).await {
            Ok(is_ok) if is_ok => ok_count += 1,
            _ => err_count += 1,
        }
    }

    println!("\n------ Results ------");
    println!("Successful requests: {}", ok_count);
    println!("Failed requests: {}", err_count);
}

async fn req(client: &Client, url: &String) -> Result<bool, Error> {
    let status = client.get(url).send().await?.status();
    Ok(!status.is_server_error() && !status.is_client_error())
}
