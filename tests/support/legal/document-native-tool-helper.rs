//! Fixed disposable tools for aggregate-deadline tests, never runtime binaries.
use std::{env, fs, io::Write, path::Path, process, thread, time::Duration};
fn main() {
    let args: Vec<_> = env::args().collect();
    let name = Path::new(&args[0]).file_name().unwrap().to_str().unwrap();
    let root = env::current_dir().unwrap().parent().unwrap().to_path_buf();
    fs::write(root.join(format!("{}.pid", process::id())), b"native tool").unwrap();
    match name {
        "pdftotext" => {}
        "pdfinfo" => println!("Pages: 25"),
        "pdftoppm" => {
            let prefix = Path::new(args.last().unwrap());
            for page in 1..=25 {
                fs::write(
                    format!("{}-{page:02}.png", prefix.display()),
                    b"fixture-only",
                )
                .unwrap();
            }
        }
        "tesseract" => {
            thread::sleep(Duration::from_millis(100));
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join("completed-pages.log"))
                .unwrap();
            writeln!(file, "page").unwrap();
            println!("partial draft");
        }
        _ => process::exit(2),
    }
}
