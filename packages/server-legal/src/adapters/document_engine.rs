//! Offline, process-local extraction engine. Only the bounded dedicated CLI
//! calls this module; API processes never parse an archive or launch tools.
use crate::{
    adapters::document_docx::extract_docx_text,
    domain::document_extraction::{
        DOCUMENT_INPUT_LIMIT, DocumentExtractionFailure as Failure, DocumentExtractionInput,
        DocumentExtractionReport, DocumentExtractionSource, EXTRACTION_DEADLINE_SECONDS,
        ExtractedDocument, ExtractedPage, ExtractionMethod, LegalDocumentMediaType,
        SCANNED_PDF_PAGE_LIMIT,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentToolConfiguration {
    pub pdftotext: PathBuf,
    pub pdfinfo: PathBuf,
    pub pdftoppm: PathBuf,
    pub tesseract: PathBuf,
    pub tessdata: PathBuf,
    pub languages: String,
}
impl DocumentToolConfiguration {
    /// Configuration belongs to the operator; a job can supply none of these
    /// paths or arguments. No shell/path lookup or environment expansion.
    pub fn validate(&self) -> Result<(), Failure> {
        for path in [
            &self.pdftotext,
            &self.pdfinfo,
            &self.pdftoppm,
            &self.tesseract,
        ] {
            if !path.is_absolute() || !path.is_file() {
                return Err(Failure::InvalidToolConfiguration);
            }
        }
        if !self.tessdata.is_absolute()
            || !self.tessdata.is_dir()
            || self.languages.is_empty()
            || self.languages.len() > 128
            || !self
                .languages
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'+' | b'-'))
        {
            return Err(Failure::InvalidToolConfiguration);
        }
        for language in self.languages.split('+') {
            if language.is_empty()
                || !self
                    .tessdata
                    .join(format!("{language}.traineddata"))
                    .is_file()
            {
                return Err(Failure::InvalidToolConfiguration);
            }
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentChildRequest {
    pub schema: u32,
    pub media_type: LegalDocumentMediaType,
    pub checksum_sha256: String,
    pub tools: DocumentToolConfiguration,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "outcome", rename_all = "lowercase", deny_unknown_fields)]
pub enum DocumentChildResponse {
    Extracted { report: DocumentExtractionReport },
    Failed { failure: Failure },
}

pub async fn extract_document_in_owned_child(
    input: DocumentExtractionInput,
    path: &Path,
    tools: &DocumentToolConfiguration,
) -> Result<ExtractedDocument, Failure> {
    if input.bytes().len() > DOCUMENT_INPUT_LIMIT {
        return Err(Failure::InputSizeLimit);
    }
    if hex::encode(Sha256::digest(input.bytes())) != input.checksum_sha256.to_ascii_lowercase() {
        return Err(Failure::InputChecksum);
    }
    let (media_type, bytes) = match input.source {
        DocumentExtractionSource::InlineText { body } => {
            return Ok(ExtractedDocument {
                method: ExtractionMethod::Inline,
                text: body,
                pages: Vec::new(),
            });
        }
        DocumentExtractionSource::Original { media_type, bytes } => (media_type, bytes),
    };
    if media_type == LegalDocumentMediaType::Docx {
        return Ok(ExtractedDocument {
            method: ExtractionMethod::DocxText,
            text: extract_docx_text(&bytes)?,
            pages: Vec::new(),
        });
    }
    tools.validate()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(EXTRACTION_DEADLINE_SECONDS);
    match media_type {
        LegalDocumentMediaType::Pdf => {
            let text = run(
                &tools.pdftotext,
                &["-layout".as_ref(), path.as_os_str(), "-".as_ref()],
                30,
                2 * 1024 * 1024,
                deadline,
            )
            .await?;
            if !text.trim().is_empty() {
                let mut segments: Vec<_> = text.split('\u{c}').collect();
                if segments.last().is_some_and(|part| part.trim().is_empty()) {
                    segments.pop();
                }
                let pages = segments
                    .into_iter()
                    .enumerate()
                    .map(|(index, text)| ExtractedPage {
                        page: index as u32 + 1,
                        text: text.to_owned(),
                    })
                    .collect();
                return Ok(ExtractedDocument {
                    method: ExtractionMethod::PdfText,
                    text,
                    pages,
                });
            }
            let info = run(&tools.pdfinfo, &[path.as_os_str()], 15, 100_000, deadline).await?;
            let pages = info
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Pages:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .filter(|pages| *pages > 0 && *pages <= SCANNED_PDF_PAGE_LIMIT)
                .ok_or(Failure::ScannedPageLimit)?;
            let prefix = path.parent().ok_or(Failure::WorkerProtocol)?.join("page");
            let page_count = pages.to_string();
            run(
                &tools.pdftoppm,
                &[
                    "-png".as_ref(),
                    "-scale-to".as_ref(),
                    "1600".as_ref(),
                    "-f".as_ref(),
                    "1".as_ref(),
                    "-l".as_ref(),
                    page_count.as_ref(),
                    path.as_os_str(),
                    prefix.as_os_str(),
                ],
                90,
                100_000,
                deadline,
            )
            .await?;
            let mut images = Vec::new();
            let mut entries = tokio::fs::read_dir(path.parent().ok_or(Failure::WorkerProtocol)?)
                .await
                .map_err(|_| Failure::ToolFailed)?;
            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|_| Failure::ToolFailed)?
            {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("page-") && name.ends_with(".png") {
                    let index = name.trim_start_matches("page-").trim_end_matches(".png");
                    if !index.is_empty() && index.bytes().all(|c| c.is_ascii_digit()) {
                        let metadata = entry.metadata().await.map_err(|_| Failure::ToolFailed)?;
                        if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
                            return Err(Failure::ToolFailed);
                        }
                        images.push(entry.path());
                    }
                }
            }
            // Preserve Poppler's padded lexical page order. Missing or extra
            // images cannot silently turn a partial document into success.
            images.sort();
            if images.len() != pages {
                return Err(Failure::ToolFailed);
            }
            let mut results = Vec::new();
            for (index, image) in images.iter().enumerate() {
                let text = ocr(image, tools, 20, 1_000_000, deadline).await?;
                results.push(ExtractedPage {
                    page: index as u32 + 1,
                    text,
                });
            }
            let text = results
                .iter()
                .map(|page| page.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(ExtractedDocument {
                method: ExtractionMethod::PdfOcr,
                text,
                pages: results,
            })
        }
        LegalDocumentMediaType::Png | LegalDocumentMediaType::Jpeg => Ok(ExtractedDocument {
            method: ExtractionMethod::ImageOcr,
            text: ocr(path, tools, 45, 2 * 1024 * 1024, deadline).await?,
            pages: Vec::new(),
        }),
        LegalDocumentMediaType::Docx => unreachable!("handled before tools"),
    }
}
async fn ocr(
    path: &Path,
    tools: &DocumentToolConfiguration,
    seconds: u64,
    limit: usize,
    deadline: tokio::time::Instant,
) -> Result<String, Failure> {
    run(
        &tools.tesseract,
        &[
            path.as_os_str(),
            "stdout".as_ref(),
            "-l".as_ref(),
            tools.languages.as_ref(),
            "--tessdata-dir".as_ref(),
            tools.tessdata.as_os_str(),
        ],
        seconds,
        limit,
        deadline,
    )
    .await
}
async fn run(
    binary: &Path,
    args: &[&std::ffi::OsStr],
    seconds: u64,
    limit: usize,
    deadline: tokio::time::Instant,
) -> Result<String, Failure> {
    let mut child = Command::new(binary)
        .args(args)
        .env_clear()
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("OMP_THREAD_LIMIT", "1")
        .env("OMP_NUM_THREADS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Failure::ToolFailed)?;
    let stdout = child.stdout.take().ok_or(Failure::ToolFailed)?;
    let stderr = child.stderr.take().ok_or(Failure::ToolFailed)?;
    let counter = Arc::new(AtomicUsize::new(0));
    let work = async {
        let (out, _, status) = tokio::try_join!(
            read(stdout, counter.clone(), limit),
            read(stderr, counter, limit),
            async { child.wait().await.map_err(|_| Failure::ToolFailed) }
        )?;
        if !status.success() {
            return Err(Failure::ToolFailed);
        }
        String::from_utf8(out).map_err(|_| Failure::InvalidTextEncoding)
    };
    let result = tokio::select! {
        biased;
        _=tokio::time::sleep_until(deadline)=>Err(Failure::DeadlineExceeded),
        _=tokio::time::sleep(Duration::from_secs(seconds))=>Err(Failure::ToolTimeout),
        result=work=>result,
    };
    if result.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    result
}
async fn read(
    mut stream: impl tokio::io::AsyncRead + Unpin,
    counter: Arc<AtomicUsize>,
    limit: usize,
) -> Result<Vec<u8>, Failure> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .map_err(|_| Failure::ToolFailed)?;
        if count == 0 {
            return Ok(output);
        }
        if counter.fetch_add(count, Ordering::Relaxed) + count > limit {
            return Err(Failure::WorkerOutputLimit);
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

/// Called only at the start of the dedicated CLI. The outer supervisor also
/// enforces a wall deadline and aggregate group RSS (macOS limits are sampled).
#[cfg(unix)]
pub fn enforce_document_child_resource_limits() -> Result<(), Failure> {
    use nix::sys::resource::{Resource, setrlimit};
    for (resource, limit) in [
        (Resource::RLIMIT_CPU, EXTRACTION_DEADLINE_SECONDS),
        (Resource::RLIMIT_STACK, 16 * 1024 * 1024),
        (Resource::RLIMIT_NOFILE, 64),
        (Resource::RLIMIT_FSIZE, 16 * 1024 * 1024),
        (Resource::RLIMIT_CORE, 0),
    ] {
        setrlimit(resource, limit, limit).map_err(|_| Failure::ResourceLimitUnavailable)?;
    }
    #[cfg(target_os = "linux")]
    for (resource, limit) in [
        (Resource::RLIMIT_DATA, 512 * 1024 * 1024),
        (Resource::RLIMIT_AS, 1024 * 1024 * 1024),
    ] {
        setrlimit(resource, limit, limit).map_err(|_| Failure::ResourceLimitUnavailable)?;
    }
    Ok(())
}
#[cfg(not(unix))]
pub fn enforce_document_child_resource_limits() -> Result<(), Failure> {
    Err(Failure::ResourceLimitUnavailable)
}
