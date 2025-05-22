use clap::{Parser, ValueEnum};

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

    /// Request body as string [default: empty]
    #[arg(short, long)]
    pub body: Option<String>,

    /// Custom HTTP header [allows multiple]
    #[arg(
        short = 'H',
        long = "header",
        value_name = "KEY=VALUE",
        action = clap::ArgAction::Append
    )]
    pub headers: Vec<Header>,
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

impl std::fmt::Display for HttpMethod {
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

#[derive(Debug, Clone)]
pub struct Header {
    pub key: String,
    pub value: String,
}

impl std::str::FromStr for Header {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (key, value) = s.split_once('=').ok_or("Invalid header format")?;

        let key = key.trim();
        let value = value.trim();

        if key.is_empty() {
            return Err("Header key cannot be empty".to_string());
        }

        if key.contains(|c: char| c.is_whitespace() || c == ':') {
            return Err(format!("Header key '{}' contains invalid characters", key));
        }

        Ok(Self {
            key: key.to_string(),
            value: value.to_string(),
        })
    }
}

pub fn init() -> Args {
    let args = Args::parse();

    if let Err(err) = url::Url::parse(&args.url) {
        eprintln!("Invalid URL error: {}", err);
        std::process::exit(1);
    }
    args
}
