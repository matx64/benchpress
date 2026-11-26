use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use benchpress_core::Registry;

fn main() -> Result<()> {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    match cmd.as_str() {
        "suites-lock" => {
            let path = root.join("suites/suites.lock");
            let lock = Registry::builtin().lockfile();
            std::fs::write(&path, &lock).with_context(|| format!("writing {}", path.display()))?;
            print!("{lock}");
            Ok(())
        }
        _ => bail!("usage: cargo xtask suites-lock"),
    }
}
