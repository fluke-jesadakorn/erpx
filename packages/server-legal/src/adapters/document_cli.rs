//! Inbound offline process adapter. Invoked only by BoundedDocumentExtractor.
use erpx_server_legal::public::{
    DOCUMENT_INPUT_LIMIT, DocumentChildRequest, DocumentChildResponse,
    DocumentExtractionFailure as Failure, DocumentExtractionInput, DocumentExtractionKind,
    DocumentExtractionSource, EXTRACTION_OUTPUT_LIMIT, enforce_document_child_resource_limits,
    extract_document_in_owned_child, finish_document_extraction,
};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn main() {
    if enforce_document_child_resource_limits().is_err() {
        std::process::exit(2);
    }
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 2 {
        std::process::exit(2);
    }
    let result = (|| -> Result<_, Failure> {
        let input = read(Path::new(&arguments[0]), DOCUMENT_INPUT_LIMIT)?;
        let request: DocumentChildRequest =
            serde_json::from_slice(&read(Path::new(&arguments[1]), 64 * 1024)?)
                .map_err(|_| Failure::WorkerProtocol)?;
        if request.schema != 1 {
            return Err(Failure::WorkerProtocol);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| Failure::WorkerUnavailable)?;
        let raw = runtime.block_on(extract_document_in_owned_child(
            DocumentExtractionInput {
                checksum_sha256: request.checksum_sha256.clone(),
                source: DocumentExtractionSource::Original {
                    media_type: request.media_type,
                    bytes: input,
                },
            },
            Path::new(&arguments[0]),
            &request.tools,
        ))?;
        finish_document_extraction(
            &request.checksum_sha256,
            DocumentExtractionKind::Original(request.media_type),
            raw,
        )
    })();
    let response = match result {
        Ok(report) => DocumentChildResponse::Extracted { report },
        Err(failure) => DocumentChildResponse::Failed { failure },
    };
    let Ok(mut output) = serde_json::to_vec(&response) else {
        std::process::exit(2)
    };
    if output.len() > EXTRACTION_OUTPUT_LIMIT {
        output = serde_json::to_vec(&DocumentChildResponse::Failed {
            failure: Failure::WorkerOutputLimit,
        })
        .unwrap_or_default();
    }
    if std::io::stdout().write_all(&output).is_err() {
        std::process::exit(2);
    }
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>, Failure> {
    let file = fs::File::open(path).map_err(|_| Failure::WorkerUnavailable)?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::WorkerUnavailable)?;
    if bytes.len() > limit {
        return Err(Failure::InputSizeLimit);
    }
    Ok(bytes)
}
