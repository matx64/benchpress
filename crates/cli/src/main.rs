//! `benchpress`: benchmark compile times on this machine.

mod commands;
mod fetch;
mod interrupt;
mod output;
mod paths;
mod probe;
mod progress;
mod report;
mod runner;
mod sampler;
mod submit;
mod toolchain;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Know what your machine can lift. Compile-time benchmarks from real machines.
#[derive(Parser)]
#[command(name = "benchpress", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run a suite: quick, standard, heavy, or a suite id
    Run(commands::run::Args),
    /// List the builtin suites
    List,
    /// List local results, or show one
    Results {
        #[command(subcommand)]
        action: Option<ResultsAction>,
    },
    /// Validate a local result, confirm and upload it
    Submit {
        /// Local result id (or prefix) or a path to a result file
        id: String,
        /// Upload without asking
        #[arg(long)]
        yes: bool,
    },
    /// Remove a result you published from this machine
    Unpublish {
        /// Local result id (or prefix)
        id: String,
    },
    /// Check this machine's setup without running a benchmark
    Doctor,
    /// Remove cached toolchains, sources and build directories
    Clean {
        /// Remove local results too, including the tokens that unpublish them
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
enum ResultsAction {
    /// Show one result
    Show {
        /// Local result id (or prefix)
        id: String,
        /// Print the raw JSON
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("BENCHPRESS_LOG")
                .unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .init();
    interrupt::install();
    let cli = Cli::parse();
    let offer_cleanup = paths::ephemeral()
        && output::is_interactive()
        && !matches!(cli.command, None | Some(Command::Clean { .. }));
    let res = match cli.command {
        None => commands::menu::run(),
        Some(Command::Run(args)) => commands::run::execute(&args),
        Some(Command::List) => commands::list(),
        Some(Command::Results { action: None }) => commands::results_list(),
        Some(Command::Results {
            action: Some(ResultsAction::Show { id, json }),
        }) => commands::results_show(&id, json),
        Some(Command::Submit { id, yes }) => submit::resolve(&id)
            .and_then(|p| submit::submit(&p, yes))
            .map(|_| ()),
        Some(Command::Unpublish { id }) => submit::resolve(&id).and_then(|p| submit::unpublish(&p)),
        Some(Command::Doctor) => commands::doctor(),
        Some(Command::Clean { all }) => commands::clean(all),
    };
    let code = match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => commands::report_error(&e),
    };
    if interrupt::terminating() {
        return ExitCode::from(143);
    }
    if offer_cleanup {
        interrupt::reset();
        if let Err(e) = commands::data::offer_cleanup() {
            output::error(format!("{e:#}"));
        }
    }
    code
}

/// Parse a `--workdir` value.
fn parse_path(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    if p.is_relative() {
        std::env::current_dir()
            .map(|d| d.join(p))
            .map_err(|e| e.to_string())
    } else {
        Ok(p)
    }
}
