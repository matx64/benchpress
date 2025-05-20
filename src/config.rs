use clap::{Parser, ValueEnum};
use reqwest::{Client, Method};
use std::{fmt::Display, time::Duration};

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

    /// Request timeout in seconds
    #[arg(short, long, default_value_t = 30)]
    pub timeout: u64,

    /// HTTP method
    #[arg(short, long, default_value_t = HttpMethod::Get)]
    pub method: HttpMethod,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Options,
    Patch,
}

pub fn init() -> (Args, Client, Method) {
    let args = Args::parse();

    if let Err(err) = url::Url::parse(&args.url) {
        eprintln!("Invalid URL error: {}", err);
        std::process::exit(1);
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout))
        .pool_max_idle_per_host(args.concurrency)
        .build()
        .expect("Failed to build Client");

    let method: Method = args.method.clone().into();

    (args, client, method)
}

impl Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Patch => "PATCH",
        };
        write!(f, "{}", s)
    }
}

impl From<HttpMethod> for Method {
    fn from(value: HttpMethod) -> Self {
        match value {
            HttpMethod::Get => Method::GET,
            HttpMethod::Post => Method::POST,
            HttpMethod::Put => Method::PUT,
            HttpMethod::Delete => Method::DELETE,
            HttpMethod::Head => Method::HEAD,
            HttpMethod::Options => Method::OPTIONS,
            HttpMethod::Patch => Method::PATCH,
        }
    }
}
