use erpx_server_legal::public::{DocumentExtractionFailure, extract_docx_text};
use serde_json::Value;
use std::io::{Cursor, Write};

#[test]
fn frozen_independent_mammoth_raw_text_conformance() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/documents/docx-conformance.v1.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 29);
    for case in cases {
        let bytes = hex::decode(case["docxHex"].as_str().unwrap()).unwrap();
        let result = extract_docx_text(&bytes);
        if case["expected"]["error"].is_string() {
            assert_eq!(
                result,
                Err(DocumentExtractionFailure::DocxTextLimit),
                "{}",
                case["id"]
            );
        } else {
            assert_eq!(
                result.as_deref().map_err(|error| error.code()),
                Ok(case["expected"]["text"].as_str().unwrap()),
                "{}",
                case["id"]
            );
        }
    }
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn archive_and_xml_resource_contract_rejects_hostile_inputs() {
    for bytes in [
        zip(&[("../escape.xml", b"<x/>")]),
        zip(&[("/absolute.xml", b"<x/>")]),
        zip(&[("word/document.xml", b"<!DOCTYPE x [<!ENTITY z 'expanded'>]><x>&z;</x>")]),
        zip(&[("word/document.xml", format!("{}{}", "<x>".repeat(66),"</x>".repeat(66)).as_bytes())]),
        zip(&[("_rels/.rels", b"<Relationships><Relationship Type='http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument' TargetMode='External' Target='https://example.invalid/a'/></Relationships>")]),
    ] {
        assert!(matches!(extract_docx_text(&bytes), Err(DocumentExtractionFailure::DocxInvalid|DocumentExtractionFailure::DocxExpansionLimit)));
    }
    let bytes = zip(&[("bomb.bin", &vec![0; 32 * 1024 * 1024 + 1])]);
    assert!(matches!(
        extract_docx_text(&bytes),
        Err(DocumentExtractionFailure::InputSizeLimit
            | DocumentExtractionFailure::DocxExpansionLimit)
    ));
}
