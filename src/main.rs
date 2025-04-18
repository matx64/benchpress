use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Target URL to benchmark
    #[arg(short, long)]
    url: String,
}

fn main() {
    let args = Args::parse();

    println!("URL: {}", args.url)
}
