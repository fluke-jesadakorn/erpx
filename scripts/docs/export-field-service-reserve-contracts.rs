use erpx_server_field_service::public::reservation_read::{
    FieldServiceAgreementReviewAction, FieldServiceAgreementReviewEntity,
    FieldServiceAgreementReviewEvent, FieldServiceAgreementReviewInput, FieldServiceCancelAction,
    FieldServiceCancelInput, FieldServiceCompletionAction, FieldServiceCompletionInput,
    FieldServiceCreatedOrderRead, FieldServiceIssueAction, FieldServiceIssueAllocation,
    FieldServiceIssueEvent, FieldServiceIssueInput, FieldServiceOrderCreateInput,
    FieldServiceRecoveryAction, FieldServiceRecoveryInput, FieldServiceRequestEntity,
    FieldServiceRequestEvent, FieldServiceRequestInput, FieldServiceRequestOrderRead,
    FieldServiceRequestResult, FieldServiceRequestTicketRead, FieldServiceReservationRead,
    FieldServiceReserveAction, FieldServiceReserveEvent, FieldServiceReserveInput,
    FieldServiceReserveResult, FieldServiceScheduleAction, FieldServiceScheduleInput,
    FieldServiceStockSnapshot, FieldServiceVisitEvent, FieldServiceVisitStepAction,
    FieldServiceVisitStepInput,
};
use erpx_service::field_service_reservation_read::{
    FieldServiceAgreementReviewReadResponse, FieldServiceIssueReadResponse,
    FieldServiceOrderCreateReadResponse, FieldServiceRenderedContext,
    FieldServiceRequestReadResponse, FieldServiceReservationReadResponse,
    FieldServiceVisitReadResponse,
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
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/requests/{key}",params(("holding_slug"=String,Path),("key"=String,Path)),responses((status=200,body=FieldServiceRequestReadResponse,description="Current scoped request evidence")))]
fn request_read() {}
const _: fn() = request_read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/orders/{key}",params(("holding_slug"=String,Path),("key"=String,Path)),responses((status=200,body=FieldServiceOrderCreateReadResponse,description="Current scoped created order evidence")))]
fn order_create_read() {}
const _: fn() = order_create_read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/visits/{key}",params(("holding_slug"=String,Path),("key"=String,Path)),responses((status=200,body=FieldServiceVisitReadResponse,description="Current scoped visit evidence")))]
fn visit_read() {}
const _: fn() = visit_read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/agreements/reviews/{key}",params(("holding_slug"=String,Path),("key"=String,Path)),responses((status=200,body=FieldServiceAgreementReviewReadResponse,description="Current scoped Agreement review evidence")))]
fn agreement_review_read() {}
const _: fn() = agreement_review_read;
const _: fn() = issue_read;
const _: fn() = reserve;
const _: fn() = read;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        reserve,
        read,
        issue_read,
        request_read,
        order_create_read,
        visit_read,
        agreement_review_read
    ),
    components(schemas(
        FieldServiceAgreementReviewAction,
        FieldServiceAgreementReviewEntity,
        FieldServiceAgreementReviewInput,
        FieldServiceAgreementReviewEvent,
        FieldServiceAgreementReviewReadResponse,
        FieldServiceCancelAction,
        FieldServiceCancelInput,
        FieldServiceRecoveryAction,
        FieldServiceRecoveryInput,
        FieldServiceCompletionAction,
        FieldServiceCompletionInput,
        FieldServiceScheduleAction,
        FieldServiceVisitStepAction,
        FieldServiceScheduleInput,
        FieldServiceVisitStepInput,
        FieldServiceVisitEvent,
        FieldServiceVisitReadResponse,
        FieldServiceOrderCreateInput,
        FieldServiceCreatedOrderRead,
        FieldServiceOrderCreateReadResponse,
        FieldServiceRequestEntity,
        FieldServiceRequestInput,
        FieldServiceRequestResult,
        FieldServiceRequestOrderRead,
        FieldServiceRequestTicketRead,
        FieldServiceRequestEvent,
        FieldServiceRequestReadResponse,
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
        value["components"]["schemas"]["FieldServiceRequestInput"]["properties"]["entity"] =
            serde_json::json!({"type":"string","const":"request"});
        value["components"]["schemas"]["FieldServiceReserveInput"]["properties"]["action"] =
            serde_json::json!({"type":"string","const":"reserve-part"});
        value["components"]["schemas"]["FieldServiceIssueInput"]["properties"]["action"] =
            serde_json::json!({"type":"string","const":"issue-part"});
        value["paths"]["/api/v1/holdings/{holding_slug}/field-service"]["post"]["requestBody"]["content"]
            ["application/json"]["schema"] = serde_json::json!({"oneOf":[{"$ref":"#/components/schemas/FieldServiceReserveInput"},{"$ref":"#/components/schemas/FieldServiceIssueInput"},{"$ref":"#/components/schemas/FieldServiceRequestInput"},{"$ref":"#/components/schemas/FieldServiceOrderCreateInput"},{"$ref":"#/components/schemas/FieldServiceScheduleInput"},{"$ref":"#/components/schemas/FieldServiceVisitStepInput"},{"$ref":"#/components/schemas/FieldServiceCompletionInput"},{"$ref":"#/components/schemas/FieldServiceRecoveryInput"},{"$ref":"#/components/schemas/FieldServiceCancelInput"},{"$ref":"#/components/schemas/FieldServiceAgreementReviewInput"}]});
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return;
    }
    assert!(args.is_empty());
    let config = Config::new();
    println!(
        "// Generated from the Rust Field Service reservation contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-field-service-reserve-contracts.ts"
    );
    for declaration in [
        FieldServiceAgreementReviewAction::decl(&config),
        FieldServiceAgreementReviewEntity::decl(&config),
        FieldServiceAgreementReviewInput::decl(&config),
        FieldServiceAgreementReviewEvent::decl(&config),
        FieldServiceAgreementReviewReadResponse::decl(&config),
        FieldServiceCancelAction::decl(&config),
        FieldServiceCancelInput::decl(&config),
        FieldServiceRecoveryAction::decl(&config),
        FieldServiceRecoveryInput::decl(&config),
        FieldServiceCompletionAction::decl(&config),
        FieldServiceCompletionInput::decl(&config),
        FieldServiceScheduleAction::decl(&config),
        FieldServiceVisitStepAction::decl(&config),
        FieldServiceScheduleInput::decl(&config),
        FieldServiceVisitStepInput::decl(&config),
        FieldServiceVisitEvent::decl(&config),
        FieldServiceVisitReadResponse::decl(&config),
        FieldServiceOrderCreateInput::decl(&config),
        FieldServiceCreatedOrderRead::decl(&config),
        FieldServiceOrderCreateReadResponse::decl(&config),
        FieldServiceRequestEntity::decl(&config),
        FieldServiceRequestInput::decl(&config),
        FieldServiceRequestResult::decl(&config),
        FieldServiceRequestOrderRead::decl(&config),
        FieldServiceRequestTicketRead::decl(&config),
        FieldServiceRequestEvent::decl(&config),
        FieldServiceRequestReadResponse::decl(&config),
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
