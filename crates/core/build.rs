use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"))
        .join("../../suites");
    let dir = dir.canonicalize().expect("suites directory");
    println!("cargo::rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read suites")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let mut out = String::from("pub(crate) static BUILTIN: &[(&str, &str)] = &[\n");
    for f in &files {
        println!("cargo::rerun-if-changed={}", f.display());
        let name = f.file_name().expect("name").to_string_lossy();
        writeln!(
            out,
            "    ({name:?}, include_str!({:?})),",
            f.display().to_string()
        )
        .expect("write");
    }
    out.push_str("];\n");
    let lock = dir.join("suites.lock");
    println!("cargo::rerun-if-changed={}", lock.display());
    let dest = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("builtin_suites.rs");
    std::fs::write(dest, out).expect("write builtin_suites.rs");
}
