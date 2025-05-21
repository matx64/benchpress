use crate::{config::Config, result::ExecutionResult};

use colored::Colorize;
use reqwest::Error;
use std::{error::Error as StdError, sync::Arc};

pub fn start_log(cfg: &Arc<Config>) {
    println!(
    "\n{}",
    "┌──────────────────────────────┐\n│ 🚀 Benchmark Initialized     │\n└──────────────────────────────┘"
        .bold()
        .green()
);
    println!("{} {}", "🌐 Target URL:".bold(), cfg.url.cyan());
    println!(
        "{} {}",
        "📦 Total Requests:".bold(),
        cfg.requests.to_string().yellow()
    );
    println!(
        "{} {}",
        "🧵 Concurrency Level:".bold(),
        cfg.concurrency.to_string().magenta()
    );
}

pub fn result_log(result: ExecutionResult) {
    println!(
        "\n{}",
        "┌──────────────────────┐\n│     📊 Results       │\n└──────────────────────┘"
            .bold()
            .cyan()
    );
    println!(
        "{} {}\n",
        "🔢 Total requests:".bold(),
        result.total_requests.to_string().yellow()
    );
    println!(
        "{} {}",
        "🟦 1xx responses:".bold(),
        result.count_1xx.to_string().blue()
    );
    println!(
        "{} {}",
        "🟩 2xx responses:".bold(),
        result.count_2xx.to_string().green()
    );
    println!(
        "{} {}",
        "🟨 3xx responses:".bold(),
        result.count_3xx.to_string().cyan()
    );
    println!(
        "{} {}",
        "🟥 4xx responses (excluding timeouts):".bold(),
        result.count_4xx.to_string().yellow()
    );
    println!(
        "{} {}",
        "🟥 5xx responses:".bold(),
        result.count_5xx.to_string().red()
    );
    println!(
        "{} {}\n",
        "🟥 Timeouts:".bold(),
        result.count_timeout.to_string().red()
    );
    // println!(
    //     "{} {:.2}ms",
    //     "⏱️  Average duration:".bold(),
    //     total_duration as f64 / results.len() as f64
    // );
    println!(
        "{} {:.2}ms",
        "🏎️  Fastest duration:".bold(),
        result.fastest as f64
    );
    println!(
        "{} {:.2}ms\n",
        "🐢 Slowest duration:".bold(),
        result.slowest as f64
    );
}

pub fn error_log(err: Error) {
    if let Some(err) = err.source() {
        if let Some(err) = err.source() {
            if err.to_string().contains("Too many open files") {
                ulimit_log();
            } else {
                println!("Error: {}", err);
            }
        } else {
            println!("Error: {}", err);
        }
    } else {
        println!("Error: {}", err);
    }
}

fn ulimit_log() {
    eprintln!(
                    "\n{}\n{}\n{}\n{}\n{}\n",
                    "❌ Error: Too many open files (Host file descriptor limit reached).".bold().red(),
                    "The configured `concurrency` parameter is too high, reaching your Host file descriptor limit.".italic(),
                    "👉 Reduce the `concurrency` value or check the Host limit with:".bold(),
                    "   ulimit -n".cyan(),
                    "🔧 Increase the limit with: ulimit -n 4096".green()
                );
}
