#[tokio::main]
async fn main() -> anyhow::Result<()> {
    benchpress_server::main_entry().await
}
