//! Disposable Rust child for supervisor tests. This is never a runtime binary.
use std::{env, fs, io::Write, path::Path, process, thread, time::Duration};

fn main() {
    let args: Vec<_> = env::args().collect();
    assert_eq!(args.len(), 4);
    let root = env::current_dir().unwrap().parent().unwrap().to_path_buf();
    fs::write(root.join("child.pid"), process::id().to_string()).unwrap();
    let mode = fs::read(&args[1]).unwrap();
    match mode.as_slice() {
        b"sleep" => thread::sleep(Duration::from_secs(60)),
        b"memory" => {
            let mut allocations = Vec::new();
            for _ in 0..16 {
                allocations.push(vec![1_u8; 4 * 1024 * 1024]);
                std::hint::black_box(&allocations);
                thread::sleep(Duration::from_millis(25));
            }
            thread::sleep(Duration::from_secs(60));
        }
        b"stdout" => loop {
            if std::io::stdout().write_all(&[b'x'; 8192]).is_err() {
                process::exit(0);
            }
        },
        b"stderr" => loop {
            if std::io::stderr().write_all(&[b'x'; 8192]).is_err() {
                process::exit(0);
            }
        },
        b"exit" => process::exit(7),
        b"inspect" => {
            assert!(env::var_os("HOME").is_none());
            assert!(env::var_os("PATH").is_none());
            assert!(env::var_os("DATABASE_URL").is_none());
            assert_eq!(env::var("TZ").unwrap(), "UTC");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(env::current_dir().unwrap())
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o700
                );
                assert_eq!(
                    fs::metadata(Path::new(&args[1]))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
                assert_eq!(
                    fs::metadata(Path::new(&args[2]))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o700
                );
            }
            fs::write(root.join("inspected"), b"ok").unwrap();
            println!("{{\"status\":\"valid\"}}");
        }
        _ => println!("not a verifier report"),
    }
}
