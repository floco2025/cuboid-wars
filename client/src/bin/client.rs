#[tokio::main]
async fn main() -> anyhow::Result<()> {
    client::run_client().await
}
