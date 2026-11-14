//! Supervised, offline PDF verifier entrypoint for the remaining TypeScript
//! Legal route. It owns the bounded child, applies the production limits and
//! emits the same report shape as the owner verifier. It has no database,
//! network or caller-selected trust access.
use erpx_server_legal::public::{
    ApprovedCompanyTrustMaterial, AuthorizedPdfSnapshot, BoundedPdfVerifier, PDF_BUSINESS_LIMIT,
    PdfProcessLimits, PdfValidationReport, PdfVerifierProcess, TrustMaterialKind,
    TrustMaterialState, VERIFICATION_OUTPUT_LIMIT, read_trust_directory,
};
use erpx_server_shared::public::{CompanyId, HoldingId, RecordId, TimestampMillis, UserId};
use sha2::{Digest, Sha256};
use std::{env, fs, io::Read, path::Path};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn main() {
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments.len() != 3 {
        eprintln!(
            "usage: erpx-verify-pdf <pdf> <operator-trust-directory> <validation-time-rfc3339>"
        );
        std::process::exit(2);
    }
    match run(
        Path::new(&arguments[0]),
        Path::new(&arguments[1]),
        &arguments[2],
    ) {
        Ok(report) => match serde_json::to_string(&report) {
            Ok(json) if json.len() <= VERIFICATION_OUTPUT_LIMIT => println!("{json}"),
            _ => failure("VERIFIER_OUTPUT_LIMIT"),
        },
        Err(code) => failure(&code),
    }
}

fn failure(code: &str) {
    println!(
        "{}",
        serde_json::json!({
            "status": "indeterminate",
            "signatures": [],
            "reasons": ["VALIDATION_FAILED", code],
        })
    );
}

fn run(
    pdf_path: &Path,
    trust_path: &Path,
    validation_time: &str,
) -> Result<PdfValidationReport, String> {
    let pdf = read_bounded(pdf_path, PDF_BUSINESS_LIMIT)?;
    let document_checksum_sha256 = hex::encode(Sha256::digest(&pdf));
    let materials = read_trust_directory(trust_path)?;
    let moment = OffsetDateTime::parse(validation_time, &Rfc3339)
        .map_err(|_| "INVALID_VALIDATION_TIME".to_owned())?;
    let millis = i64::try_from(moment.unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| "INVALID_VALIDATION_TIME".to_owned())?;
    let company_id = CompanyId::new("supervised-cli").map_err(|_| "INVALID_SNAPSHOT".to_owned())?;
    let creator =
        UserId::new("supervised-cli-creator").map_err(|_| "INVALID_SNAPSHOT".to_owned())?;
    let approver =
        UserId::new("supervised-cli-approver").map_err(|_| "INVALID_SNAPSHOT".to_owned())?;
    let mut trust = Vec::with_capacity(materials.len());
    for (index, material) in materials.into_iter().enumerate() {
        let kind = if material.path.starts_with("roots/") {
            TrustMaterialKind::Root
        } else if material.path.starts_with("intermediates/") {
            TrustMaterialKind::Intermediate
        } else if material.path.ends_with(".crl") {
            TrustMaterialKind::Crl
        } else {
            TrustMaterialKind::Ocsp
        };
        trust.push(ApprovedCompanyTrustMaterial {
            id: RecordId::new(format!("supervised-{index:04}"))
                .map_err(|_| "INVALID_SNAPSHOT".to_owned())?,
            company_id: company_id.clone(),
            version: 1,
            kind,
            state: TrustMaterialState::Approved,
            created_by: creator.clone(),
            approved_by: Some(approver.clone()),
            independent_person_review: true,
            checksum_sha256: hex::encode(Sha256::digest(&material.content)),
            content: material.content,
        });
    }
    let snapshot = AuthorizedPdfSnapshot {
        holding_id: HoldingId::new("supervised-cli").map_err(|_| "INVALID_SNAPSHOT".to_owned())?,
        company_id,
        document_version_id: RecordId::new("supervised-cli")
            .map_err(|_| "INVALID_SNAPSHOT".to_owned())?,
        document_checksum_sha256,
        authorization_revision: "supervised-cli/operator-directory".to_owned(),
        trust_revision: "supervised-cli/approved-material".to_owned(),
        pdf,
        trust,
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix("erpx-verify-pdf-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let root = builder
        .tempdir_in(env::temp_dir())
        .map_err(|_| "VERIFIER_UNAVAILABLE".to_owned())?;
    let child = env::current_exe()
        .map_err(|_| "VERIFIER_UNAVAILABLE".to_owned())?
        .parent()
        .ok_or_else(|| "VERIFIER_UNAVAILABLE".to_owned())?
        .join("erpx-validate-pdf");
    let verifier = BoundedPdfVerifier::new(&child, root.path(), PdfProcessLimits::default())
        .map_err(|failure| failure.code().to_owned())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "VERIFIER_UNAVAILABLE".to_owned())?;
    let evidence = runtime
        .block_on(verifier.verify(snapshot, TimestampMillis::new(millis)))
        .map_err(|failure| failure.code().to_owned())?;
    runtime.block_on(verifier.shutdown_and_drain());
    Ok(evidence.report)
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| "PDF_UNAVAILABLE".to_owned())?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "PDF_UNAVAILABLE".to_owned())?;
    if bytes.len() > limit {
        return Err("PDF_SIZE_LIMIT".to_owned());
    }
    Ok(bytes)
}
