use erpx_server_finance::public::{
    JournalLineInput, JournalStatus, ManualJournalCreateInput, PreparedJournalResult,
};
use ts_rs::{Config, TS};
#[derive(utoipa::ToSchema)]
struct JournalCreateResponse {
    #[expect(dead_code, reason = "Schema-only HTTP response envelope.")]
    data: PreparedJournalResult,
}
#[utoipa::path(post, path="/api/v1/holdings/{holding_slug}/finance/journals", params(("holding_slug"=String,Path)), request_body=ManualJournalCreateInput, responses((status=201,body=JournalCreateResponse,description="Original manual journal creation receipt"),(status=401,description="Current session required"),(status=403,description="Current Finance authority denied"),(status=409,description="Work context or command changed")))]
fn create_journal() {}
const _: fn() = create_journal;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(create_journal),
    components(schemas(
        ManualJournalCreateInput,
        JournalLineInput,
        PreparedJournalResult,
        JournalStatus,
        JournalCreateResponse
    ))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut document = Document::openapi();
        document.info.title = "ERPX native Finance manual journal creation".into();
        let mut value = serde_json::to_value(document).unwrap();
        let schemas = &mut value["components"]["schemas"];
        for field in ["fxRate", "memo", "sourceType", "sourceId"] {
            schemas["ManualJournalCreateInput"]["properties"][field]["type"] = "string".into();
        }
        schemas["ManualJournalCreateInput"]["properties"]["isOpening"]["type"] = "boolean".into();
        schemas["JournalLineInput"]["properties"]["description"]["type"] = "string".into();
        schemas["JournalLineInput"]["properties"]["dimensions"]["type"] = "object".into();
        let properties = &mut value["components"]["schemas"]["PreparedJournalResult"]["properties"];
        properties["status"] = serde_json::json!({"type":"string","const":"draft"});
        properties["version"] =
            serde_json::json!({"type":"integer","const":1,"minimum":1,"maximum":1});
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return;
    }
    assert!(args.is_empty());
    let config = Config::new();
    println!(
        "// Generated from the Rust Finance manual journal creation contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-finance-journal-create-contracts.ts"
    );
    for declaration in [
        ManualJournalCreateInput::decl(&config),
        JournalLineInput::decl(&config),
        PreparedJournalResult::decl(&config),
        JournalStatus::decl(&config),
    ] {
        println!("export {declaration}");
    }
}
