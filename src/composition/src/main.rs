//! Thin executable composition. Owner implementations never depend on this crate.
use erpx_server_runtime::public::{
    EmbeddedMigrationInventory, inspect_migration, validate_foundation_address,
};
#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run_cli(std::env::args().skip(1)).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
pub async fn run_cli(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let arguments: Vec<_> = arguments.into_iter().collect();
    erpx_service::module_assembly::validate_build().map_err(str::to_owned)?;
    match arguments.as_slice() {
        #[cfg(feature = "legal-document-worker")]
        [command, once, option, path]
            if command == "legal-worker"
                && once == "--once"
                && option == "--development-config" =>
        {
            let outcome =
                erpx_service::legal_worker_bootstrap::run_once_file(std::path::Path::new(path))
                    .await
                    .map_err(str::to_owned)?;
            println!("{}", outcome);
            Ok(())
        }
        #[cfg(feature = "private-field-api")]
        [command, option, path] if command == "private-api" && option == "--development-config" => {
            erpx_service::private_development_api::run_development_api_file(
                std::path::Path::new(path),
                shutdown(),
            )
            .await
            .map_err(str::to_owned)
        }
        #[cfg(feature = "production-api")]
        [command, option, path]
            if command == "production-api" && option == "--production-config" =>
        {
            erpx_service::private_development_api::run_production_api_file(
                std::path::Path::new(path),
                shutdown(),
            )
            .await
            .map_err(str::to_owned)
        }
        #[cfg(feature = "production-api")]
        [command, option, path]
            if command == "production-worker" && option == "--production-config" =>
        {
            erpx_service::private_development_api::run_production_workers_file(
                std::path::Path::new(path),
                shutdown(),
            )
            .await
            .map_err(str::to_owned)
        }
        #[cfg(all(feature = "files-scan-worker", feature = "private-field-api"))]
        [command, option, path]
            if command == "field-files-worker" && option == "--development-config" =>
        {
            erpx_service::private_development_api::run_field_files_worker_file(
                std::path::Path::new(path),
                shutdown(),
            )
            .await
            .map_err(str::to_owned)
        }
        #[cfg(feature = "private-field-api")]
        [command, option, path]
            if command == "field-dependency-worker" && option == "--development-config" =>
        {
            erpx_service::private_development_api::run_field_dependency_worker_file(
                std::path::Path::new(path),
                shutdown(),
            )
            .await
            .map_err(str::to_owned)
        }
        #[cfg(feature = "private-field-api")]
        [command, option, path, holding_flag, holding, event_flag, event, at_flag, at, reason_flag, reason]
            if command == "field-dependency-retry" && option == "--development-config"
                && holding_flag == "--holding" && event_flag == "--event"
                && at_flag == "--dead-letter-at" && reason_flag == "--reason" =>
        {
            let at=at.parse::<i64>().map_err(|_|"FIELD_RETRY_TIME_INVALID".to_owned())?;
            erpx_service::private_development_api::retry_field_dependency_worker_file(
                std::path::Path::new(path),holding,event,at,reason,
            ).await.map_err(str::to_owned)
        }
        #[cfg(feature = "private-field-api")]
        [command, option, path]
            if command == "service-count-worker" && option == "--development-config" =>
        {
            erpx_service::private_development_api::run_service_count_worker_file(
                std::path::Path::new(path),
                shutdown(),
            )
            .await
            .map_err(str::to_owned)
        }
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
            let router = erpx_service::module_assembly::assemble().map_err(str::to_owned)?;
            let listener = tokio::net::TcpListener::bind(address)
                .await
                .map_err(|_| "RUST_FOUNDATION_LISTEN_FAILED")?;
            eprintln!("Rust foundation listening on {address}; readiness remains unavailable.");
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown())
                .await
                .map_err(|_| "RUST_FOUNDATION_HTTP_FAILED".to_owned())
        }
        _ => {
            let mut usage = String::from(
                "Usage: erpx-server owners | erpx-server api --listen 127.0.0.1:<disposable-port>",
            );
            if cfg!(feature = "legal-document-worker") {
                usage.push_str(" | erpx-server legal-worker --once --development-config /absolute/private.json");
            }
            if cfg!(feature = "private-field-api") {
                usage.push_str(
                    " | erpx-server private-api --development-config /absolute/private.json",
                );
            }
            if cfg!(feature = "production-api") {
                usage.push_str(
                    " | erpx-server production-api --production-config /absolute/private.json | erpx-server production-worker --production-config /absolute/private.json",
                );
            }
            Err(usage)
        }
    }
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
