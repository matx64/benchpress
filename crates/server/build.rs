fn main() {
    minijinja_embed::embed_templates!("templates");
    println!("cargo::rerun-if-changed=static");
    println!("cargo::rerun-if-changed=assets");
    println!("cargo::rerun-if-changed=../../install.sh");
}
