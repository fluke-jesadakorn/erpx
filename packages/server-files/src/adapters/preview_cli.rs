use erpx_server_files::public::preview::run_preview_child;
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3
        || run_preview_child(
            std::path::Path::new(&args[1]),
            std::path::Path::new(&args[2]),
        )
        .is_err()
    {
        // Protocol/resource failure only: never print private file/tool errors.
        std::process::exit(1);
    }
}
