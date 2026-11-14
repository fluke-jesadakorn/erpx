//! Operational child/protocol evidence only. No Company trust, permission,
//! release decision or signature-validity assertion is produced by this probe.
use crate::domain::pdf_verification::{AuthorizedPdfSnapshot, PdfVerificationFailure as Failure};
use erpx_server_shared::public::{CompanyId, HoldingId, RecordId};
use lopdf::{Document, Object, Stream, dictionary};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::PathBuf};

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfProviderProfile {
    protocol: u32,
    component_version: String,
    binary_sha256: String,
    binary_bytes: u64,
    probe_sha256: String,
    limits: PdfProviderLimits,
    profile_sha256: String,
}
#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PdfProviderLimits {
    pub deadline_millis: u64,
    pub output_bytes: usize,
    pub memory_bytes: u64,
}
impl PdfProviderProfile {
    pub fn sha256(&self) -> &str {
        &self.profile_sha256
    }
    pub fn binary_sha256(&self) -> &str {
        &self.binary_sha256
    }
    pub fn probe_sha256(&self) -> &str {
        &self.probe_sha256
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfProviderReadiness {
    profile: PdfProviderProfile,
    checked_at: i64,
    probe_outcome: &'static str,
}
impl PdfProviderReadiness {
    pub fn profile(&self) -> &PdfProviderProfile {
        &self.profile
    }
    pub fn checked_at(&self) -> i64 {
        self.checked_at
    }
    pub fn probe_outcome(&self) -> &str {
        self.probe_outcome
    }
}

/// Vetted writer, fixed small empty-page data. It has no signatures, embedded
/// trust, links, external references, current time or caller-selected content.
fn probe_bytes() -> Result<Vec<u8>, Failure> {
    let mut pdf = Document::with_version("1.7");
    let pages = pdf.new_object_id();
    let content = pdf.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page=pdf.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![Object::Integer(0),Object::Integer(0),Object::Integer(595),Object::Integer(842)],"Contents"=>content});
    pdf.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![Object::Reference(page)],"Count"=>1}.into(),
    );
    let catalog = pdf.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    pdf.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes)
        .map_err(|_| Failure::WorkerUnavailable)?;
    if bytes.len() > 8192 {
        return Err(Failure::WorkerUnavailable);
    }
    Ok(bytes)
}
pub(super) fn snapshot() -> Result<AuthorizedPdfSnapshot, Failure> {
    let pdf = probe_bytes()?;
    Ok(AuthorizedPdfSnapshot {
        holding_id: HoldingId::new("operational-probe").map_err(|_| Failure::InvalidSnapshot)?,
        company_id: CompanyId::new("operational-probe").map_err(|_| Failure::InvalidSnapshot)?,
        document_version_id: RecordId::new("operational-probe")
            .map_err(|_| Failure::InvalidSnapshot)?,
        document_checksum_sha256: hex::encode(Sha256::digest(&pdf)),
        authorization_revision: "operational-probe/data-only".into(),
        trust_revision: "operational-probe/no-company-trust".into(),
        pdf,
        trust: Vec::new(),
    })
}
pub(super) async fn inspect_profile(
    binary: PathBuf,
    private_root: PathBuf,
    limits: PdfProviderLimits,
) -> Result<PdfProviderProfile, Failure> {
    tokio::task::spawn_blocking(move||{
        let root=fs::symlink_metadata(private_root).map_err(|_|Failure::WorkerUnavailable)?;
        if !root.is_dir()||root.file_type().is_symlink(){return Err(Failure::WorkerUnavailable);}
        #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;if root.permissions().mode()&0o077!=0{return Err(Failure::WorkerUnavailable);}}
        let mut file=fs::File::open(binary).map_err(|_|Failure::WorkerUnavailable)?;let metadata=file.metadata().map_err(|_|Failure::WorkerUnavailable)?;
        if !metadata.is_file()||metadata.len()>256*1024*1024{return Err(Failure::WorkerUnavailable);}
        #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;if metadata.permissions().mode()&0o111==0{return Err(Failure::WorkerUnavailable);}}
        let mut hash=Sha256::new();let mut bytes=0;let mut buffer=[0;64*1024];
        loop{let count=file.read(&mut buffer).map_err(|_|Failure::WorkerUnavailable)?;if count==0{break;}bytes+=count as u64;if bytes>256*1024*1024{return Err(Failure::WorkerUnavailable);}hash.update(&buffer[..count]);}
        if bytes!=metadata.len(){return Err(Failure::WorkerUnavailable);}
        let binary_sha256=hex::encode(hash.finalize());let probe_sha256=hex::encode(Sha256::digest(probe_bytes()?));
        let data=serde_json::json!({"format":"erpx-pdf-provider-profile/v1","protocol":1,"componentVersion":env!("CARGO_PKG_VERSION"),"mode":"offline-strict-revocation","engine":"ERPX Rust / OpenSSL / lopdf","requiredPathProof":true,"binarySha256":binary_sha256,"binaryBytes":bytes,"probeSha256":probe_sha256,"limits":limits});
        let profile_sha256=hex::encode(Sha256::digest(data.to_string().as_bytes()));
        Ok(PdfProviderProfile{protocol:1,component_version:env!("CARGO_PKG_VERSION").into(),binary_sha256,binary_bytes:bytes,probe_sha256,limits,profile_sha256})
    }).await.map_err(|_|Failure::WorkerUnavailable)?
}
pub(super) fn ready(profile: PdfProviderProfile, checked_at: i64) -> PdfProviderReadiness {
    PdfProviderReadiness {
        profile,
        checked_at,
        probe_outcome: "unsigned-indeterminate",
    }
}
