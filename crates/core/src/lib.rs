//! Shared data contracts for Benchpress: suite manifests, result documents,
//! hashing, normalization and validation. No I/O beyond parsing.

pub mod format;
pub mod hardware;
pub mod hash;
pub mod registry;
pub mod result;
pub mod suite;
pub mod validate;

pub use registry::{RegisteredSuite, Registry};
pub use result::BenchResult;
pub use suite::{SuiteManifest, Tier};

/// Errors from parsing or structural validation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Input could not be parsed.
    #[error("parse error: {0}")]
    Parse(String),
    /// Input parsed but breaks a rule.
    #[error("{0}")]
    Invalid(String),
}

/// Version of this crate, used as the client version by the CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
