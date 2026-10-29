use erpx_server_finance::public::journal_review::{JournalReviewInput, JournalReviewResult};
use ts_rs::{Config, TS};
#[derive(utoipa::ToSchema)]
struct JournalPostResponse {
    #[expect(
        dead_code,
        reason = "Schema-only HTTP envelope; no runtime value is constructed."
    )]
    data: JournalReviewResult,
}
#[utoipa::path(post, path="/api/v1/holdings/{holding_slug}/finance/journals/{id}/post", params(("holding_slug"=String,Path),("id"=String,Path)), request_body=JournalReviewInput, responses((status=200,body=JournalPostResponse,description="Original journal posting receipt data"),(status=401,description="Current session required"),(status=403,description="Current review authority denied"),(status=409,description="Work or journal changed")))]
fn post_journal() {}
const _: fn() = post_journal;
#[utoipa::path(post, path="/api/v1/holdings/{holding_slug}/finance/journals/{id}/reverse", params(("holding_slug"=String,Path),("id"=String,Path)), request_body=JournalReviewInput, responses((status=201,body=JournalPostResponse,description="Original reversal receipt data"),(status=401,description="Current session required"),(status=403,description="Current review authority denied"),(status=409,description="Work or journal changed")))]
fn reverse_journal() {}
const _: fn() = reverse_journal;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(post_journal, reverse_journal),
    components(schemas(JournalReviewInput, JournalReviewResult, JournalPostResponse))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut doc = Document::openapi();
        doc.info.title = "ERPX native Finance journal review".into();
        let mut value = serde_json::to_value(doc).unwrap();
        // Select the existing owner variants by their unique fields, retaining
        // their generated field constraints rather than copying a second DTO.
        fn variant(doc: &serde_json::Value, name: &str, field: &str) -> serde_json::Value {
            let variants = doc["components"]["schemas"][name]["oneOf"]
                .as_array()
                .unwrap();
            let matches: Vec<_> = variants
                .iter()
                .filter(|v| v["properties"].get(field).is_some())
                .collect();
            assert_eq!(matches.len(), 1);
            matches[0].clone()
        }
        let mut reverse_input = variant(&value, "JournalReviewInput", "periodId");
        reverse_input["properties"]["memo"]["type"] = "string".into();
        reverse_input["properties"]["entryDate"]["format"] = "date".into();
        let reverse_result = variant(&value, "JournalReviewResult", "reversalJournalId");
        let input = variant(&value, "JournalReviewInput", "expectedVersion");
        let mut result = variant(&value, "JournalReviewResult", "id");
        result["properties"]["status"]["const"] = "posted".into();
        let schemas = value["components"]["schemas"].as_object_mut().unwrap();
        let mut reverse_envelope = schemas["JournalPostResponse"].clone();
        reverse_envelope["properties"]["data"]["$ref"] =
            "#/components/schemas/JournalReverseResult".into();
        schemas.insert("JournalReverseResponse".into(), reverse_envelope);
        schemas.insert("JournalReverseInput".into(), reverse_input);
        schemas.insert("JournalReverseResult".into(), reverse_result);
        schemas.insert("JournalPostInput".into(), input);
        schemas.insert("JournalPostResult".into(), result);
        schemas.get_mut("JournalPostResponse").unwrap()["properties"]["data"]["$ref"] =
            "#/components/schemas/JournalPostResult".into();
        schemas.remove("JournalReviewInput");
        schemas.remove("JournalReviewResult");
        value["paths"]["/api/v1/holdings/{holding_slug}/finance/journals/{id}/post"]["post"]["requestBody"]
            ["content"]["application/json"]["schema"]["$ref"] =
            "#/components/schemas/JournalPostInput".into();
        let reverse = &mut value["paths"]["/api/v1/holdings/{holding_slug}/finance/journals/{id}/reverse"]
            ["post"];
        reverse["requestBody"]["content"]["application/json"]["schema"]["$ref"] =
            "#/components/schemas/JournalReverseInput".into();
        reverse["responses"]["201"]["content"]["application/json"]["schema"]["$ref"] =
            "#/components/schemas/JournalReverseResponse".into();
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return;
    }
    assert!(args.is_empty());
    let cfg = Config::new();
    println!(
        "// Generated from the Rust Finance journal review contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-finance-journal-post-contracts.ts"
    );
    for d in [
        JournalReviewInput::decl(&cfg),
        JournalReviewResult::decl(&cfg),
    ] {
        println!("export {d}");
    }
}
