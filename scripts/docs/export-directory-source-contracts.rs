use erpx_service::directory_source_wire::*;
use ts_rs::{Config, TS};
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/source/preview",params(("holding_slug"=String,Path,description="Exact registered Holding slug"),("connection_id"=String,Path),("Origin"=String,Header,description="Required exact registered HTTPS Holding origin"),("Idempotency-Key"=String,Header),("x-erpx-context-company"=String,Header),("x-erpx-context-revision"=String,Header),("x-erpx-context-assignment"=String,Header),("x-erpx-context-assignment-version"=String,Header)),request_body=DirectorySourcePreviewInput,responses((status=200,body=DirectorySourceResponse),(status=400,body=DirectorySourceErrorResponse),(status=401,body=DirectorySourceErrorResponse),(status=403,body=DirectorySourceErrorResponse),(status=404,description="Generic unavailable target"),(status=409,body=DirectorySourceErrorResponse),(status=413,body=DirectorySourceErrorResponse),(status=422,body=DirectorySourceErrorResponse),(status=500,body=DirectorySourceErrorResponse),(status=503,body=DirectorySourceErrorResponse)))]
fn preview() {}
const _: fn() = preview;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/source/reviews/{review_id}/approve",params(("holding_slug"=String,Path),("connection_id"=String,Path),("review_id"=String,Path),("Origin"=String,Header),("Idempotency-Key"=String,Header),("x-erpx-context-company"=String,Header),("x-erpx-context-revision"=String,Header),("x-erpx-context-assignment"=String,Header),("x-erpx-context-assignment-version"=String,Header)),request_body=DirectorySourceApproveInput,responses((status=200,body=DirectorySourceResponse),(status=400,body=DirectorySourceErrorResponse),(status=401,body=DirectorySourceErrorResponse),(status=403,body=DirectorySourceErrorResponse),(status=404,description="Generic unavailable target"),(status=409,body=DirectorySourceErrorResponse),(status=413,body=DirectorySourceErrorResponse),(status=422,body=DirectorySourceErrorResponse),(status=500,body=DirectorySourceErrorResponse),(status=503,body=DirectorySourceErrorResponse)))]
fn approve() {}
const _: fn() = approve;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(preview, approve),
    components(schemas(
        DirectorySourceInteger,
        DirectorySourcePreviewInput,
        DirectorySourceApproveInput,
        DirectorySourceStatus,
        DirectorySourceReviewData,
        DirectorySourceResponse,
        DirectorySourceError,
        DirectorySourceErrorResponse
    ))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut doc = Document::openapi();
        doc.info.title = "ERPX native Directory source administration".into();
        doc.info.description=Some("Additive private, unmounted Holding-only preview and independent approve/apply. Exact registered origin, opaque Holding child session, current Company IAM/CAS and exact-session fresh MFA precede body/schema validation and receipt replay. Source tags, selectors and result data are not authority. Strings use runtime ECMAScript trimming and 3..1000 Unicode-code-point reason units as in pinned Zod; schema scalar length bounds match. Body limit16KiB. No generic review listing/rejection, platform provenance, bootstrap allowlist, UI, live provider or release acceptance.".into());
        println!("{}", doc.to_pretty_json().unwrap());
        return;
    }
    assert!(
        args.is_empty(),
        "DIRECTORY_SOURCE_CONTRACT_ARGUMENTS_INVALID"
    );
    let config = Config::default();
    println!(
        "// Generated from native Rust Directory source-admin wire types. Do not edit.\n// Regenerate: bun scripts/docs/generate-directory-source-contracts.ts"
    );
    for declaration in [
        DirectorySourceInteger::decl(&config),
        DirectorySourcePreviewInput::decl(&config),
        DirectorySourceApproveInput::decl(&config),
        DirectorySourceStatus::decl(&config),
        DirectorySourceReviewData::decl(&config),
        DirectorySourceResponse::decl(&config),
        DirectorySourceError::decl(&config),
        DirectorySourceErrorResponse::decl(&config),
    ] {
        println!("export {declaration}");
    }
}
