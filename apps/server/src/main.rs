#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    erpx_server::run().await
}
