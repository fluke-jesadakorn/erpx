#![allow(dead_code, reason="Path anchors generate OpenAPI metadata; runtime handlers live in composition")]
use erpx_service::field_attachment_intake_contracts::*;
use ts_rs::{Config, TS};
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/attachment-sets",params(("holding_slug"=String,Path),("targetType"=String,Query),("targetId"=String,Query),("purposeKey"=Option<String>,Query),("componentKind"=Option<String>,Query),("expectedTargetVersion"=Option<String>,Query)),responses((status=200,body=FieldIntakeListResponse),(status=422,body=FieldIntakeErrorResponse)))]
fn list() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/attachment-sets",params(("holding_slug"=String,Path),("Idempotency-Key"=String,Header)),request_body=FieldIntakeCreateInput,responses((status=201,body=FieldIntakeCreateResponse),(status=422,body=FieldIntakeErrorResponse)))]
fn create() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/attachment-sets/{id}/files",params(("holding_slug"=String,Path),("id"=String,Path),("Idempotency-Key"=String,Header)),request_body=FieldFileReserveInput,responses((status=201,body=FieldFileReservationResponse),(status=422,body=FieldIntakeErrorResponse)))]
fn reserve() {}
#[utoipa::path(put,path="/api/v1/holdings/{holding_slug}/field-service/uploads/{id}/content",params(("holding_slug"=String,Path),("id"=String,Path),("Content-Type"=String,Header)),request_body(content=Vec<u8>,content_type="application/octet-stream"),responses((status=202,body=FieldFileUploadResponse),(status=422,body=FieldIntakeErrorResponse)))]
fn upload() {}
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/attachment-links/{id}/status",params(("holding_slug"=String,Path),("id"=String,Path)),responses((status=200,body=FieldFileStatusResponse)))]
fn status() {}
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/attachment-links/{id}/content",params(("holding_slug"=String,Path),("id"=String,Path),("variant"=Option<String>,Query)),responses((status=200,body=Vec<u8>,content_type="application/octet-stream"),(status=423,body=FieldIntakeErrorResponse)))]
fn content() {}
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(list, create, reserve, upload, status, content),
    components(schemas(
        FieldIntakeTarget,
        FieldIntakeComponent,
        FieldIntakeState,
        FieldIntakeClassification,
        FieldIntakeSourceState,
        FieldIntakeCreateInput,
        FieldIntakeLink,
        FieldIntakeSet,
        FieldIntakeListResponse,
        FieldIntakeCreateResponse,
        FieldFileReserveInput,
        FieldFileReservationData,
        FieldFileReservationResponse,
        FieldFileUploadData,
        FieldFileUploadResponse,
        FieldFileScanStatus,
        FieldFileStatusData,
        FieldFileStatusResponse,
        FieldIntakeError,
        FieldIntakeErrorResponse
    ))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut doc = Document::openapi();
        doc.info.title = "ERPX native Field attachment intake".into();
        let mut value = serde_json::to_value(doc).unwrap();
        for (name, fields) in [
            (
                "FieldFileScanStatus",
                &[
                    "lastResult",
                    "lastError",
                    "signature",
                    "definitionUpdatedAt",
                    "scannedAt",
                ][..],
            ),
            (
                "FieldFileStatusData",
                &["sourceId", "sourceState", "boundAt", "withdrawnAt"][..],
            ),
            ("FieldIntakeSet", &["componentRef"][..]),
            (
                "FieldIntakeLink",
                &["sourceId", "bytes", "sourceState", "caption", "boundAt"][..],
            ),
        ] {
            let required = value["components"]["schemas"][name]["required"]
                .as_array_mut()
                .unwrap();
            for field in fields {
                if !required.iter().any(|v| v.as_str() == Some(field)) {
                    required.push(serde_json::json!(field));
                }
            }
        }
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return;
    }
    assert!(args.is_empty());
    let config = Config::new();
    println!("// Generated from the Rust Field attachment intake contracts. Do not edit.");
    macro_rules! emit {($($t:ty),* $(,)?)=>{$(println!("export {}\n",<$t as TS>::decl(&config));)*};}
    emit!(
        FieldIntakeTarget,
        FieldIntakeComponent,
        FieldIntakeState,
        FieldIntakeClassification,
        FieldIntakeSourceState,
        FieldIntakeCreateInput,
        FieldIntakeLink,
        FieldIntakeSet,
        FieldIntakeListResponse,
        FieldIntakeCreateResponse,
        FieldFileReserveInput,
        FieldFileReservationData,
        FieldFileReservationResponse,
        FieldFileUploadData,
        FieldFileUploadResponse,
        FieldFileScanStatus,
        FieldFileStatusData,
        FieldFileStatusResponse,
        FieldIntakeError,
        FieldIntakeErrorResponse
    );
}
