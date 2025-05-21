use crate::args::{Args, HttpMethod};

use reqwest::{Client, Method};
use std::{sync::Arc, time::Duration};

pub struct Config {
    pub url: String,
    pub requests: usize,
    pub concurrency: usize,
    pub method: Method,
    pub client: Client,
}

pub fn init(args: Args) -> Arc<Config> {
    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout))
        .pool_max_idle_per_host(args.concurrency)
        .build()
        .expect("Failed to build Client");

    let method = match args.method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
        HttpMethod::Put => Method::PUT,
        HttpMethod::Delete => Method::DELETE,
        HttpMethod::Head => Method::HEAD,
        HttpMethod::Options => Method::OPTIONS,
        HttpMethod::Patch => Method::PATCH,
    };

    Arc::new(Config {
        url: args.url,
        requests: args.requests,
        concurrency: args.concurrency,
        method,
        client,
    })
}
