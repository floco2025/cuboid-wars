#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    server::run_server().await
}
