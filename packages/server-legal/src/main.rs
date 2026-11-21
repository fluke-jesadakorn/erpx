//! Offline conformance/operations entrypoint; it has no database or network access.
use erpx_server_legal::public::{
    OperatorTrustStore, enforce_pdf_child_resource_limits, validate_signed_pdf,
};
use std::{env, fs, io::Read, path::Path};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn main() {
    if enforce_pdf_child_resource_limits().is_err() {
        eprintln!("VERIFIER_RESOURCE_LIMIT_UNAVAILABLE");
        std::process::exit(2);
    }
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments.len() != 3 {
        eprintln!(
            "usage: erpx-validate-pdf <pdf> <operator-trust-directory> <validation-time-rfc3339>"
        );
        std::process::exit(2);
    }
    let result = (|| -> Result<_, String> {
        let file = fs::File::open(&arguments[0]).map_err(|_| "PDF_UNAVAILABLE")?;
        let mut bytes = Vec::new();
        file.take(10 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "PDF_UNAVAILABLE")?;
        if bytes.len() > 10 * 1024 * 1024 {
            return Err("PDF_SIZE_LIMIT".to_owned());
        }
        let trust = OperatorTrustStore::from_directory(Path::new(&arguments[1]))?;
        let moment = OffsetDateTime::parse(&arguments[2], &Rfc3339)
            .map_err(|_| "INVALID_VALIDATION_TIME")?;
        Ok(validate_signed_pdf(&bytes, &trust, moment))
    })();
    match result {
        Ok(report) => match serde_json::to_string(&report) {
            Ok(json) if json.len() <= 2 * 1024 * 1024 => println!("{json}"),
            Ok(_) => println!(
                "{}",
                serde_json::json!({"status":"indeterminate", "signatures":[], "reasons":["VALIDATION_FAILED", "VALIDATOR_OUTPUT_LIMIT"]})
            ),
            Err(_) => std::process::exit(2),
        },
        Err(reason) => {
            println!(
                "{}",
                serde_json::json!({"status":"indeterminate", "signatures":[], "reasons":["VALIDATION_FAILED", reason]})
            );
        }
    }
}
