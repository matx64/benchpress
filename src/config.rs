use clap::Parser;
use reqwest::Client;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Target URL to benchmark
    #[arg(short, long)]
    pub url: String,

    /// Total number of requests
    #[arg(short, long, default_value_t = 1)]
    pub requests: usize,

    /// Number of concurrent requests
    #[arg(short, long, default_value_t = 50)]
    pub concurrency: usize,
}

pub fn init() -> (Args, Client) {
    let args = Args::parse();

    if let Err(err) = url::Url::parse(&args.url) {
        eprintln!("Invalid URL error: {}", err);
        std::process::exit(1);
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to build Client");

    (args, client)
}
