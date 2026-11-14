//! Operational evidence only. Resource observations and benign probes neither
//! grant Company/record authority nor establish OCR text accuracy or release.
use super::document_engine::DocumentToolConfiguration;
use crate::domain::document_extraction::{
    DocumentExtractionFailure as Failure, DocumentExtractionInput, DocumentExtractionReport,
    DocumentExtractionSource, EXTRACTION_DEADLINE_SECONDS, EXTRACTION_OUTPUT_LIMIT,
    EXTRACTION_TEXT_LIMIT,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::PathBuf};
mod probes;
const RESOURCE_TOTAL_LIMIT: u64 = 512 * 1024 * 1024;
const RESOURCE_FILE_LIMIT: u64 = 256 * 1024 * 1024;
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentProviderResource {
    role: String,
    sha256: String,
    bytes: u64,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentProviderProfile {
    protocol: u32,
    component_version: String,
    languages: String,
    resources: Vec<DocumentProviderResource>,
    profile_sha256: String,
}
impl DocumentProviderProfile {
    pub fn sha256(&self) -> &str {
        &self.profile_sha256
    }
    pub fn resources(&self) -> &[DocumentProviderResource] {
        &self.resources
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentProviderReadiness {
    profile: DocumentProviderProfile,
    checked_at: i64,
    probes: Vec<String>,
}
impl DocumentProviderReadiness {
    pub fn profile(&self) -> &DocumentProviderProfile {
        &self.profile
    }
    pub fn checked_at(&self) -> i64 {
        self.checked_at
    }
    pub fn probes(&self) -> &[String] {
        &self.probes
    }
}
pub(super) async fn inspect_profile(
    binary: PathBuf,
    tools: DocumentToolConfiguration,
) -> Result<DocumentProviderProfile, Failure> {
    tokio::task::spawn_blocking(move||{
        tools.validate()?;
        let mut files=vec![("rust-child".to_owned(),binary,true),("pdftotext".into(),tools.pdftotext.clone(),true),("pdfinfo".into(),tools.pdfinfo.clone(),true),("pdftoppm".into(),tools.pdftoppm.clone(),true),("tesseract".into(),tools.tesseract.clone(),true)];
        if tools.languages.split('+').count()>8{return Err(Failure::InvalidToolConfiguration);}
        files.extend(tools.languages.split('+').map(|language|(format!("language:{language}"),tools.tessdata.join(format!("{language}.traineddata")),false)));
        let mut total=0;let mut resources=Vec::new();
        for (role,path,executable) in files{
            let path=path.canonicalize().map_err(|_|Failure::InvalidToolConfiguration)?;
            let mut file=fs::File::open(path).map_err(|_|Failure::InvalidToolConfiguration)?;let metadata=file.metadata().map_err(|_|Failure::InvalidToolConfiguration)?;
            if !metadata.is_file()||metadata.len()>RESOURCE_FILE_LIMIT{return Err(Failure::InvalidToolConfiguration);}
            #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;if executable&&metadata.permissions().mode()&0o111==0{return Err(Failure::InvalidToolConfiguration);}}
            #[cfg(not(unix))]let _=executable;
            let mut hash=Sha256::new();let mut bytes=0;let mut buffer=[0;64*1024];
            loop{let count=file.read(&mut buffer).map_err(|_|Failure::InvalidToolConfiguration)?;if count==0{break;}bytes+=count as u64;total+=count as u64;if bytes>RESOURCE_FILE_LIMIT||total>RESOURCE_TOTAL_LIMIT{return Err(Failure::InvalidToolConfiguration);}hash.update(&buffer[..count]);}
            if bytes!=metadata.len(){return Err(Failure::InvalidToolConfiguration);}
            resources.push(DocumentProviderResource{role,sha256:hex::encode(hash.finalize()),bytes});
        }
        let data=serde_json::json!({"format":"erpx-document-provider-profile/v1","protocol":1,"version":env!("CARGO_PKG_VERSION"),"languages":tools.languages,"resources":resources,"limits":{"input":crate::domain::document_extraction::DOCUMENT_INPUT_LIMIT,"output":EXTRACTION_OUTPUT_LIMIT,"textUtf16":EXTRACTION_TEXT_LIMIT,"seconds":EXTRACTION_DEADLINE_SECONDS}});
        let profile_sha256=hex::encode(Sha256::digest(data.to_string().as_bytes()));
        Ok(DocumentProviderProfile{protocol:1,component_version:env!("CARGO_PKG_VERSION").into(),languages:tools.languages,resources,profile_sha256})
    }).await.map_err(|_|Failure::WorkerUnavailable)?
}
pub(super) fn inputs() -> Result<Vec<(&'static str, DocumentExtractionInput)>, Failure> {
    probes::PROBES
        .iter()
        .map(|probe| {
            let bytes = hex::decode(probe.hex).map_err(|_| Failure::InvalidToolConfiguration)?;
            let checksum_sha256 = hex::encode(Sha256::digest(&bytes));
            Ok((
                probe.label,
                DocumentExtractionInput {
                    checksum_sha256,
                    source: DocumentExtractionSource::Original {
                        media_type: probe.media_type,
                        bytes,
                    },
                },
            ))
        })
        .collect()
}
pub(super) fn require_probe(label: &str, report: &DocumentExtractionReport) -> Result<(), Failure> {
    let expected = probes::PROBES
        .iter()
        .find(|probe| probe.label == label)
        .ok_or(Failure::WorkerProtocol)?;
    if report.method != Some(expected.method)
        || report.pages.len() != expected.pages
        || report.text.trim().is_empty()
        || !report.requires_review
    {
        return Err(Failure::WorkerProtocol);
    }
    Ok(())
}
pub(super) fn ready(
    profile: DocumentProviderProfile,
    checked_at: i64,
    probes: Vec<String>,
) -> DocumentProviderReadiness {
    DocumentProviderReadiness {
        profile,
        checked_at,
        probes,
    }
}
