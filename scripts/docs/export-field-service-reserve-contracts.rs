use erpx_server_field_service::public::reservation_read::{
    FieldServiceIssueAction, FieldServiceIssueAllocation, FieldServiceIssueEvent,
    FieldServiceIssueInput, FieldServiceReservationRead, FieldServiceReserveAction,
    FieldServiceReserveEvent, FieldServiceReserveInput, FieldServiceReserveResult,
    FieldServiceStockSnapshot,
};
use erpx_service::field_service_reservation_read::{
    FieldServiceIssueReadResponse, FieldServiceRenderedContext, FieldServiceReservationReadResponse,
};
use ts_rs::{Config, TS};
#[derive(utoipa::ToSchema)]
struct ReserveResponse {
    #[expect(dead_code, reason = "Schema-only HTTP envelope.")]
    data: FieldServiceReserveResult,
}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service",params(("holding_slug"=String,Path)),request_body=FieldServiceReserveInput,responses((status=200,body=ReserveResponse,description="Retained reserve command receipt")))]
fn reserve() {}
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/{order_id}/reservations/{reservation_id}",params(("holding_slug"=String,Path),("order_id"=String,Path),("reservation_id"=String,Path)),responses((status=200,body=FieldServiceReservationReadResponse,description="Current scoped reservation readback")))]
fn read() {}
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/{order_id}/issues/{key}",params(("holding_slug"=String,Path),("order_id"=String,Path),("key"=String,Path)),responses((status=200,body=FieldServiceIssueReadResponse,description="Current scoped issue readback")))]
fn issue_read() {}
const _: fn() = issue_read;
const _: fn() = reserve;
const _: fn() = read;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(reserve, read, issue_read),
    components(schemas(
        FieldServiceIssueAction,
        FieldServiceIssueAllocation,
        FieldServiceIssueInput,
        FieldServiceIssueEvent,
        FieldServiceIssueReadResponse,
        FieldServiceReserveAction,
        FieldServiceReserveInput,
        FieldServiceReserveResult,
        FieldServiceReservationRead,
        FieldServiceReserveEvent,
        FieldServiceStockSnapshot,
        FieldServiceRenderedContext,
        FieldServiceReservationReadResponse,
        ReserveResponse
    ))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut document = Document::openapi();
        document.info.title = "ERPX native Field Service reservation".into();
        let mut value = serde_json::to_value(document).unwrap();
        value["components"]["schemas"]["FieldServiceReserveInput"]["properties"]["action"] =
            serde_json::json!({"type":"string","const":"reserve-part"});
        value["components"]["schemas"]["FieldServiceIssueInput"]["properties"]["action"] =
            serde_json::json!({"type":"string","const":"issue-part"});
        value["paths"]["/api/v1/holdings/{holding_slug}/field-service"]["post"]["requestBody"]["content"]
            ["application/json"]["schema"] = serde_json::json!({"oneOf":[{"$ref":"#/components/schemas/FieldServiceReserveInput"},{"$ref":"#/components/schemas/FieldServiceIssueInput"}]});
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return;
    }
    assert!(args.is_empty());
    let config = Config::new();
    println!(
        "// Generated from the Rust Field Service reservation contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-field-service-reserve-contracts.ts"
    );
    for declaration in [
        FieldServiceIssueAction::decl(&config),
        FieldServiceIssueAllocation::decl(&config),
        FieldServiceIssueInput::decl(&config),
        FieldServiceIssueEvent::decl(&config),
        FieldServiceIssueReadResponse::decl(&config),
        FieldServiceReserveAction::decl(&config),
        FieldServiceReserveInput::decl(&config),
        FieldServiceReserveResult::decl(&config),
        FieldServiceReservationRead::decl(&config),
        FieldServiceReserveEvent::decl(&config),
        FieldServiceStockSnapshot::decl(&config),
        FieldServiceRenderedContext::decl(&config),
        FieldServiceReservationReadResponse::decl(&config),
    ] {
        println!("export {declaration}");
    }
}
