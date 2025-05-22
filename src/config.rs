use crate::args::{Args, Header, HttpMethod};

use reqwest::{
    Client, Method,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use std::{sync::Arc, time::Duration};

pub struct Config {
    pub url: String,
    pub requests: usize,
    pub concurrency: usize,
    pub body: String,
    pub method: Method,
    pub client: Client,
}

pub fn init(args: Args) -> Arc<Config> {
    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout))
        .pool_max_idle_per_host(args.concurrency)
        .default_headers(build_header_map(args.headers))
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
        body: args.body.unwrap_or_default(),
        method,
        client,
    })
}

fn build_header_map(headers: Vec<Header>) -> HeaderMap {
    let mut map = HeaderMap::new();

    for header in headers {
        let name = HeaderName::from_bytes(header.key.as_bytes()).unwrap();
        let value = HeaderValue::from_str(&header.value).unwrap();
        map.insert(name, value);
    }
    map
}
