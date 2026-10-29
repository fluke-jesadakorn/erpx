use erpx_server_field_service::public::reservation_read::{
    FieldServiceAgreementCreateInput, FieldServiceAgreementReviewAction,
    FieldServiceAgreementReviewEntity, FieldServiceAgreementReviewEvent,
    FieldServiceAgreementReviewInput, FieldServiceBillingAction, FieldServiceBillingInput,
    FieldServiceCancelAction, FieldServiceCancelInput, FieldServiceCompletionAction,
    FieldServiceCompletionInput, FieldServiceCreatedOrderRead, FieldServiceIssueAction,
    FieldServiceIssueAllocation, FieldServiceIssueEvent, FieldServiceIssueInput,
    FieldServiceOrderCreateInput, FieldServiceRecoveryAction, FieldServiceRecoveryInput,
    FieldServiceRequestEntity, FieldServiceRequestEvent, FieldServiceRequestInput,
    FieldServiceRequestOrderRead, FieldServiceRequestResult, FieldServiceRequestTicketRead,
    FieldServiceReservationRead, FieldServiceReserveAction, FieldServiceReserveEvent,
    FieldServiceReserveInput, FieldServiceReserveResult, FieldServiceScheduleAction,
    FieldServiceScheduleInput, FieldServiceStockSnapshot, FieldServiceVisitEvent,
    FieldServiceVisitStepAction, FieldServiceVisitStepInput,
};
use erpx_server_field_service::public::visit::FieldServiceBillingRelease;
use erpx_server_field_service::public::work::FieldServiceWorkItem;
use erpx_server_finance::public::field_service_agreement_money::AgreementMoneyEvidence;
use erpx_service::field_service_detail::{
    FieldServiceDetailSpares, FieldServiceOrderDetail, FieldServiceOrderDetailResponse,
};
use erpx_service::field_service_reservation_read::{
    FieldServiceAgreementCreateReadResponse, FieldServiceAgreementReviewReadResponse,
    FieldServiceBillingReadResponse, FieldServiceIssueReadResponse,
    FieldServiceOrderCreateReadResponse, FieldServiceRenderedContext,
    FieldServiceRequestReadResponse, FieldServiceReservationReadResponse,
    FieldServiceVisitReadResponse,
};
use erpx_service::field_service_source_record::{
    FieldServiceSourceRecord, FieldServiceSourceRecordResponse,
};
use erpx_service::field_service_workspace::{
    FieldServiceWorkResponse, FieldServiceWorkspace, FieldServiceWorkspaceResponse,
    FieldServiceWorkspaceScope,
};
use erpx_service::service_parts_posting_setup::{
    ServicePostingCommandResponse, ServicePostingReadResponse, ServicePostingResult,
    ServicePostingView,
};
use erpx_service::service_parts_prerequisite_setup::{
    ServicePrerequisiteAccountInput, ServicePrerequisiteAccountRequest,
    ServicePrerequisiteAccountResponse, ServicePrerequisiteAccountResult,
    ServicePrerequisiteChartInput, ServicePrerequisiteChartRequest,
    ServicePrerequisiteChartResponse, ServicePrerequisiteChartResult,
    ServicePrerequisitePeriodInput, ServicePrerequisitePeriodRequest,
    ServicePrerequisitePeriodResponse, ServicePrerequisitePeriodResult,
    ServicePrerequisiteRateInput, ServicePrerequisiteRateRequest, ServicePrerequisiteRateResponse,
    ServicePrerequisiteRateResult, ServicePrerequisitesReadResponse, ServicePrerequisitesView,
};
use erpx_service::service_stock_setup::{
    ServiceStockBranch, ServiceStockMasterResponse, ServiceStockMasterResult,
    ServiceStockProductInput, ServiceStockReadResponse, ServiceStockWarehouseInput,
    ServiceStockWorkspace,
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
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/billings/{key}",params(("holding_slug"=String,Path),("key"=String,Path)),responses((status=200,body=FieldServiceBillingReadResponse,description="Current scoped billing release evidence")))]
fn billing_read() {}
const _: fn() = billing_read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/agreements/reviews/{key}",params(("holding_slug"=String,Path),("key"=String,Path)),responses((status=200,body=FieldServiceAgreementReviewReadResponse,description="Current scoped Agreement review evidence")))]
fn agreement_review_read() {}
const _: fn() = agreement_review_read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/agreements/creates/{key}",params(("holding_slug"=String,Path),("key"=String,Path)),responses((status=200,body=FieldServiceAgreementCreateReadResponse,description="Current scoped Agreement create evidence")))]
fn agreement_create_read() {}
const _: fn() = agreement_create_read;
const _: fn() = issue_read;
const _: fn() = reserve;
const _: fn() = read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/workspace",params(("holding_slug"=String,Path)),responses((status=200,body=FieldServiceWorkspaceResponse,description="Current scoped Service workspace")))]
fn workspace_read() {}
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/work",params(("holding_slug"=String,Path)),responses((status=200,body=FieldServiceWorkResponse,description="Current scoped Field Service Work contribution")))]
fn work_read() {}
const _: fn() = work_read;
const _: fn() = workspace_read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/orders/{id}/detail",params(("holding_slug"=String,Path),("id"=String,Path)),responses((status=200,body=FieldServiceOrderDetailResponse,description="Current scoped Service order detail and workspace")))]
fn detail_read() {}
const _: fn() = detail_read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/orders/{id}/source-record",params(("holding_slug"=String,Path),("id"=String,Path),("version"=Option<i64>,Query)),responses((status=200,body=FieldServiceSourceRecordResponse,description="Current scoped Field source record")))]
fn source_record_read() {}
const _: fn() = source_record_read;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup",params(("holding_slug"=String,Path)),responses((status=200,body=ServicePostingReadResponse,description="Current Service parts posting setup")))]
fn parts_posting_read() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup",params(("holding_slug"=String,Path)),responses((status=200,body=ServicePostingCommandResponse,description="Service parts posting initialization receipt")))]
fn parts_posting_initialize() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/existing",params(("holding_slug"=String,Path)),responses((status=200,body=ServicePostingCommandResponse,description="Service parts posting confirmation receipt")))]
fn parts_posting_confirm() {}
const _: fn() = parts_posting_read;
const _: fn() = parts_posting_initialize;
const _: fn() = parts_posting_confirm;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/prerequisites",params(("holding_slug"=String,Path)),responses((status=200,body=ServicePrerequisitesReadResponse,description="Current Company Finance prerequisite choices")))]
fn parts_prerequisites_read() {}
const _: fn() = parts_prerequisites_read;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/charts",params(("holding_slug"=String,Path)),request_body=ServicePrerequisiteChartRequest,responses((status=200,body=ServicePrerequisiteChartResponse,description="Original Finance prerequisite command receipt")))]
fn parts_prerequisite_charts() {}
const _: fn() = parts_prerequisite_charts;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/accounts",params(("holding_slug"=String,Path)),request_body=ServicePrerequisiteAccountRequest,responses((status=200,body=ServicePrerequisiteAccountResponse,description="Original Finance prerequisite command receipt")))]
fn parts_prerequisite_accounts() {}
const _: fn() = parts_prerequisite_accounts;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/periods",params(("holding_slug"=String,Path)),request_body=ServicePrerequisitePeriodRequest,responses((status=200,body=ServicePrerequisitePeriodResponse,description="Original Finance prerequisite command receipt")))]
fn parts_prerequisite_periods() {}
const _: fn() = parts_prerequisite_periods;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/rates",params(("holding_slug"=String,Path)),request_body=ServicePrerequisiteRateRequest,responses((status=200,body=ServicePrerequisiteRateResponse,description="Original Finance prerequisite command receipt")))]
fn parts_prerequisite_rates() {}
const _: fn() = parts_prerequisite_rates;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/stock",params(("holding_slug"=String,Path)),responses((status=200,body=ServiceStockReadResponse,description="Current scoped Service Stock setup")))]
fn parts_stock_read() {}
const _: fn() = parts_stock_read;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/masters/products",params(("holding_slug"=String,Path)),request_body=ServiceStockProductInput,responses((status=200,body=ServiceStockMasterResponse,description="Original scoped product command receipt")))]
fn parts_stock_products() {}
const _: fn() = parts_stock_products;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/parts-setup/masters/warehouses",params(("holding_slug"=String,Path)),request_body=ServiceStockWarehouseInput,responses((status=200,body=ServiceStockMasterResponse,description="Original scoped warehouse command receipt")))]
fn parts_stock_warehouses() {}
const _: fn() = parts_stock_warehouses;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        parts_stock_read,
        parts_stock_products,
        parts_stock_warehouses,
        parts_prerequisites_read,
        parts_prerequisite_charts,
        parts_prerequisite_accounts,
        parts_prerequisite_periods,
        parts_prerequisite_rates,
        parts_posting_read,
        parts_posting_initialize,
        parts_posting_confirm,
        source_record_read,
        detail_read,
        workspace_read,
        work_read,
        reserve,
        read,
        issue_read,
        request_read,
        order_create_read,
        visit_read,
        billing_read,
        agreement_review_read,
        agreement_create_read
    ),
    components(schemas(
        ServiceStockWorkspace,
        ServiceStockBranch,
        ServiceStockReadResponse,
        ServiceStockProductInput,
        ServiceStockWarehouseInput,
        ServiceStockMasterResult,
        ServiceStockMasterResponse,
        ServicePrerequisitesView,
        ServicePrerequisitesReadResponse,
        ServicePrerequisiteChartInput,
        ServicePrerequisiteChartRequest,
        ServicePrerequisiteChartResult,
        ServicePrerequisiteChartResponse,
        ServicePrerequisiteAccountInput,
        ServicePrerequisiteAccountRequest,
        ServicePrerequisiteAccountResult,
        ServicePrerequisiteAccountResponse,
        ServicePrerequisitePeriodInput,
        ServicePrerequisitePeriodRequest,
        ServicePrerequisitePeriodResult,
        ServicePrerequisitePeriodResponse,
        ServicePrerequisiteRateInput,
        ServicePrerequisiteRateRequest,
        ServicePrerequisiteRateResult,
        ServicePrerequisiteRateResponse,
        ServicePostingView,
        ServicePostingResult,
        ServicePostingReadResponse,
        ServicePostingCommandResponse,
        FieldServiceSourceRecord,
        FieldServiceSourceRecordResponse,
        FieldServiceDetailSpares,
        FieldServiceOrderDetail,
        FieldServiceOrderDetailResponse,
        FieldServiceWorkItem,
        FieldServiceWorkResponse,
        FieldServiceWorkspace,
        FieldServiceWorkspaceScope,
        FieldServiceWorkspaceResponse,
        AgreementMoneyEvidence,
        FieldServiceAgreementCreateInput,
        FieldServiceAgreementCreateReadResponse,
        FieldServiceAgreementReviewAction,
        FieldServiceAgreementReviewEntity,
        FieldServiceAgreementReviewInput,
        FieldServiceAgreementReviewEvent,
        FieldServiceAgreementReviewReadResponse,
        FieldServiceBillingAction,
        FieldServiceBillingInput,
        FieldServiceBillingRelease,
        FieldServiceBillingReadResponse,
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
            ["application/json"]["schema"] = serde_json::json!({"oneOf":[{"$ref":"#/components/schemas/FieldServiceReserveInput"},{"$ref":"#/components/schemas/FieldServiceIssueInput"},{"$ref":"#/components/schemas/FieldServiceRequestInput"},{"$ref":"#/components/schemas/FieldServiceOrderCreateInput"},{"$ref":"#/components/schemas/FieldServiceScheduleInput"},{"$ref":"#/components/schemas/FieldServiceVisitStepInput"},{"$ref":"#/components/schemas/FieldServiceCompletionInput"},{"$ref":"#/components/schemas/FieldServiceRecoveryInput"},{"$ref":"#/components/schemas/FieldServiceCancelInput"},{"$ref":"#/components/schemas/FieldServiceBillingInput"},{"$ref":"#/components/schemas/FieldServiceAgreementReviewInput"},{"$ref":"#/components/schemas/FieldServiceAgreementCreateInput"}]});
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return;
    }
    assert!(args.is_empty());
    let config = Config::new();
    println!(
        "// Generated from the Rust Field Service reservation contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-field-service-reserve-contracts.ts"
    );
    for declaration in [
        ServiceStockWorkspace::decl(&config),
        ServiceStockBranch::decl(&config),
        ServiceStockReadResponse::decl(&config),
        ServiceStockProductInput::decl(&config),
        ServiceStockWarehouseInput::decl(&config),
        ServiceStockMasterResult::decl(&config),
        ServiceStockMasterResponse::decl(&config),
        ServicePrerequisitesView::decl(&config),
        ServicePrerequisitesReadResponse::decl(&config),
        ServicePrerequisiteChartInput::decl(&config),
        ServicePrerequisiteChartRequest::decl(&config),
        ServicePrerequisiteChartResult::decl(&config),
        ServicePrerequisiteChartResponse::decl(&config),
        ServicePrerequisiteAccountInput::decl(&config),
        ServicePrerequisiteAccountRequest::decl(&config),
        ServicePrerequisiteAccountResult::decl(&config),
        ServicePrerequisiteAccountResponse::decl(&config),
        ServicePrerequisitePeriodInput::decl(&config),
        ServicePrerequisitePeriodRequest::decl(&config),
        ServicePrerequisitePeriodResult::decl(&config),
        ServicePrerequisitePeriodResponse::decl(&config),
        ServicePrerequisiteRateInput::decl(&config),
        ServicePrerequisiteRateRequest::decl(&config),
        ServicePrerequisiteRateResult::decl(&config),
        ServicePrerequisiteRateResponse::decl(&config),
        ServicePostingView::decl(&config),
        ServicePostingResult::decl(&config),
        ServicePostingReadResponse::decl(&config),
        ServicePostingCommandResponse::decl(&config),
        FieldServiceSourceRecord::decl(&config),
        FieldServiceSourceRecordResponse::decl(&config),
        FieldServiceDetailSpares::decl(&config),
        FieldServiceOrderDetail::decl(&config),
        FieldServiceOrderDetailResponse::decl(&config),
        FieldServiceWorkItem::decl(&config),
        FieldServiceWorkResponse::decl(&config),
        FieldServiceWorkspace::decl(&config),
        FieldServiceWorkspaceScope::decl(&config),
        FieldServiceWorkspaceResponse::decl(&config),
        AgreementMoneyEvidence::decl(&config),
        FieldServiceAgreementCreateInput::decl(&config),
        FieldServiceAgreementCreateReadResponse::decl(&config),
        FieldServiceAgreementReviewAction::decl(&config),
        FieldServiceAgreementReviewEntity::decl(&config),
        FieldServiceAgreementReviewInput::decl(&config),
        FieldServiceAgreementReviewEvent::decl(&config),
        FieldServiceAgreementReviewReadResponse::decl(&config),
        FieldServiceBillingAction::decl(&config),
        FieldServiceBillingInput::decl(&config),
        FieldServiceBillingRelease::decl(&config),
        FieldServiceBillingReadResponse::decl(&config),
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
