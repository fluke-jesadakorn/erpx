use std::net::SocketAddr;

use crate::adapters::{
    http::foundation_router,
    migration_inventory::{
        EmbeddedMigrationInventory, inspect_registered_migration as inspect_migration,
    },
};

pub async fn run_cli(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let arguments: Vec<_> = arguments.into_iter().collect();
    match arguments.as_slice() {
        [command] if command == "owners" => {
            let (inventory, readiness) = inspect_migration(&EmbeddedMigrationInventory)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"inventory":inventory,"readiness":readiness})
                )
                .map_err(|_| "RUST_INVENTORY_ENCODING_FAILED")?
            );
            Ok(())
        }
        [command, option, address] if command == "api" && option == "--listen" => {
            let address = validate_foundation_address(address)?;
            let listener = tokio::net::TcpListener::bind(address)
                .await
                .map_err(|_| "RUST_FOUNDATION_LISTEN_FAILED")?;
            eprintln!("Rust foundation listening on {address}; readiness remains unavailable.");
            axum::serve(listener, foundation_router())
                .with_graceful_shutdown(shutdown())
                .await
                .map_err(|_| "RUST_FOUNDATION_HTTP_FAILED".to_owned())
        }
        _ => Err(
            "Usage: erpx-server owners | erpx-server api --listen 127.0.0.1:<disposable-port>"
                .to_owned(),
        ),
    }
}

pub fn validate_foundation_address(address: &str) -> Result<SocketAddr, String> {
    let address: SocketAddr = address
        .parse()
        .map_err(|_| "EXPLICIT_LOOPBACK_ADDRESS_REQUIRED")?;
    if !address.ip().is_loopback()
        || address.port() == 0
        || matches!(
            address.port(),
            3000 | 3001 | 3100 | 3101 | 3103 | 5432 | 5433 | 5435 | 8080 | 3200..=3299
        )
    {
        return Err("DISPOSABLE_FOUNDATION_PORT_REQUIRED".to_owned());
    }
    Ok(address)
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            let _ = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
