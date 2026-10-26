#[tokio::main]
async fn main() -> std::process::ExitCode {
    match erpx_server_runtime::public::run_cli(std::env::args().skip(1)).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
