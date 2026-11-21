use erpx_server_files::public::malware_scanner::{ClamAvConfig, ClamAvScanner, MalwareVerdict};
use erpx_server_shared::public::{Clock, TimestampMillis};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Now;
impl Clock for Now {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        )
    }
}

#[tokio::test]
async fn real_local_clamav_proves_clean_and_standard_test_signature_verdicts() {
    let socket = PathBuf::from(
        std::env::var("ERPX_CLAMD_SOCKET").expect("Explicit local ClamAV provider socket required"),
    );
    let definitions = PathBuf::from(
        std::env::var("ERPX_CLAMAV_DATABASE_DIR")
            .expect("Explicit ClamAV provider definitions required"),
    );
    assert!(socket.is_absolute() && definitions.is_absolute());
    let scanner = ClamAvScanner::new(ClamAvConfig {
        socket,
        database_directory: definitions,
        maximum_definition_age: Duration::from_secs(86400),
        command_timeout: Duration::from_secs(5),
        scan_timeout: Duration::from_secs(30),
        concurrent_scans: 1,
    })
    .unwrap();
    let clean = scanner
        .scan(b"ERPX synthetic native scanner provider fixture", &Now)
        .await
        .unwrap();
    assert_eq!(clean.verdict, MalwareVerdict::Clean);
    assert!(clean.readiness.engine.starts_with("ClamAV "));
    // This public standard test signature is sent in memory only; never create
    // a quarantined file in the checkout, operator storage or installed data.
    let standard = concat!(
        "X5O!P%@AP[4\\PZX54(P^)7CC)7}$",
        "EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*"
    );
    let infected = scanner.scan(standard.as_bytes(), &Now).await.unwrap();
    match infected.verdict {
        MalwareVerdict::Infected { signature } => {
            assert!(signature.contains("Eicar") || signature.contains("EICAR"))
        }
        MalwareVerdict::Clean => panic!("The real daemon failed its standard detection test"),
    }
}
