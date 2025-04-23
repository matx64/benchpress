use crate::{RequestResult, config::Args};
use colored::Colorize;

pub fn start_log(args: &Args) {
    println!(
    "\n{}",
    "┌──────────────────────────────┐\n│ 🚀 Benchmark Initialized     │\n└──────────────────────────────┘"
        .bold()
        .green()
);
    println!("{} {}", "🌐 Target URL:".bold(), args.url.cyan());
    println!(
        "{} {}",
        "📦 Total Requests:".bold(),
        args.requests.to_string().yellow()
    );
    println!(
        "{} {}",
        "🧵 Concurrency Level:".bold(),
        args.concurrency.to_string().magenta()
    );
}

pub fn result_log(
    results: Vec<RequestResult>,
    count_1xx: u32,
    count_2xx: u32,
    count_3xx: u32,
    count_4xx: u32,
    count_5xx: u32,
    total_duration: u128,
) {
    println!(
        "\n{}",
        "┌──────────────────────┐\n│     📊 Results       │\n└──────────────────────┘"
            .bold()
            .cyan()
    );
    println!(
        "{} {}\n",
        "🔢 Total requests:".bold(),
        results.len().to_string().yellow()
    );
    println!(
        "{} {}",
        "🟦 1xx responses:".bold(),
        count_1xx.to_string().blue()
    );
    println!(
        "{} {}",
        "🟩 2xx responses:".bold(),
        count_2xx.to_string().green()
    );
    println!(
        "{} {}",
        "🟨 3xx responses:".bold(),
        count_3xx.to_string().cyan()
    );
    println!(
        "{} {}",
        "🟥 4xx responses:".bold(),
        count_4xx.to_string().yellow()
    );
    println!(
        "{} {}\n",
        "🟥 5xx responses:".bold(),
        count_5xx.to_string().red()
    );
    println!(
        "{} {:.2}ms\n",
        "⏱️  Average duration:".bold(),
        total_duration as f64 / results.len() as f64
    );
}

pub fn ulimit_log() {
    eprintln!(
                    "\n{}\n{}\n{}\n{}\n{}\n",
                    "❌ Error: Too many open files (Host file descriptor limit reached).".bold().red(),
                    "The configured `concurrency` parameter is too high, reaching your Host file descriptor limit.".italic(),
                    "👉 Reduce the `concurrency` value or check the Host limit with:".bold(),
                    "   ulimit -n".cyan(),
                    "🔧 Increase the limit with: ulimit -n 4096".green()
                );
}
