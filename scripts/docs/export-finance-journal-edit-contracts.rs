use erpx_server_finance::public::{
    JournalLineInput, JournalStatus, ManualJournalEditInput, ManualJournalEditResult,
};
use ts_rs::{Config, TS};

#[derive(utoipa::ToSchema)]
struct JournalEditResponse {
    #[expect(dead_code, reason = "Schema-only HTTP response envelope.")]
    data: ManualJournalEditResult,
}
#[utoipa::path(post, path="/api/v1/holdings/{holding_slug}/finance/journals/{journal_id}/edit", params(("holding_slug"=String,Path),("journal_id"=String,Path)), request_body=ManualJournalEditInput, responses((status=200,body=JournalEditResponse,description="Manual journal edit receipt"),(status=401,description="Current session required"),(status=403,description="Current Finance authority denied"),(status=409,description="Work context or command changed")))]
fn edit_journal() {}
const _: fn() = edit_journal;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(edit_journal),
    components(schemas(
        ManualJournalEditInput,
        JournalLineInput,
        ManualJournalEditResult,
        JournalStatus,
        JournalEditResponse
    ))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut document = Document::openapi();
        document.info.title = "ERPX native Finance manual journal edit".into();
        let mut value = serde_json::to_value(document).unwrap();
        let schemas = &mut value["components"]["schemas"];
        schemas["ManualJournalEditInput"]["properties"]["memo"]["type"] = "string".into();
        schemas["JournalLineInput"]["properties"]["description"]["type"] = "string".into();
        schemas["JournalLineInput"]["properties"]["dimensions"]["type"] = "object".into();
        schemas["ManualJournalEditResult"]["properties"]["status"] =
            serde_json::json!({"type":"string","const":"draft"});
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return;
    }
    assert!(args.is_empty());
    let config = Config::new();
    println!(
        "// Generated from the Rust Finance manual journal edit contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-finance-journal-edit-contracts.ts"
    );
    for declaration in [
        ManualJournalEditInput::decl(&config),
        JournalLineInput::decl(&config),
        ManualJournalEditResult::decl(&config),
        JournalStatus::decl(&config),
    ] {
        println!("export {declaration}");
    }
}
