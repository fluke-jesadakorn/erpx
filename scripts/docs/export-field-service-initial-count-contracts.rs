#![allow(dead_code, reason="Path anchors generate OpenAPI metadata; runtime handlers live in composition")]
use erpx_service::field_service_reservation_read::FieldServiceRenderedContext;
use erpx_service::service_initial_physical_stock_contracts::*;
use ts_rs::{Config, TS};
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock",params(("holding_slug"=String,Path), ("warehouseId"=Option<String>,Query), ("productId"=Option<String>,Query), ("countDate"=Option<String>,Query)),responses((status=200,body=ServiceInitialCountReadResponse,description="Current authorized Service Count workspace")))]
fn workspace() {}
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}",params(("holding_slug"=String,Path), ("id"=String,Path)),responses((status=200,body=ServiceInitialCountReadResponse,description="Current authorized Service Count record")))]
fn record() {}
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count",params(("holding_slug"=String,Path), ("id"=String,Path)),responses((status=200,body=ServiceInitialCountReadResponse,description="Current authorized generic Service Count detail")))]
fn generic_detail() {}
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/flow",params(("holding_slug"=String,Path), ("id"=String,Path)),responses((status=200,body=ServiceInitialCountReadResponse,description="Current authorized Service Count flow")))]
fn flow() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock",params(("holding_slug"=String,Path)),request_body=CountCreateInput,responses((status=201,body=ServiceInitialCountCommandResponse,description="Current authorized Service Count create")))]
fn create() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/update",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=CountUpdateInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Service Count draft update receipt")))]
fn update() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/flow",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=CountAssignmentInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Current authorized Service Count assign")))]
fn assign() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/submit",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=CountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Current authorized Service Count action_submit")))]
fn action_submit() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/approve",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=CountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Current authorized Service Count action_approve")))]
fn action_approve() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/post",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=CountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Current authorized Service Count action_post")))]
fn action_post() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/reject",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=CountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Current authorized Service Count action_reject")))]
fn action_reject() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/cancel",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=CountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Current authorized Service Count action_cancel")))]
fn action_cancel() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count/submit",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=GenericCountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Generic Count submit receipt")))]
fn generic_submit() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count/approve",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=GenericCountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Generic Count approve receipt")))]
fn generic_approve() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count/post",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=GenericCountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Generic Count post receipt")))]
fn generic_post() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count/reject",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=GenericCountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Generic Count reject receipt")))]
fn generic_reject() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count/cancel",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=GenericCountActionInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Generic Count cancel receipt")))]
fn generic_cancel() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count/flow",params(("holding_slug"=String,Path), ("id"=String,Path)),request_body=GenericCountAssignmentInput,responses((status=200,body=ServiceInitialCountCommandResponse,description="Generic Count Flow assignment receipt")))]
fn generic_flow() {}
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        workspace,
        generic_detail,
        record,
        flow,
        create,
        update,
        assign,
        action_submit,
        action_approve,
        action_post,
        action_reject,
        action_cancel,
        generic_submit,
        generic_approve,
        generic_post,
        generic_reject,
        generic_cancel,
        generic_flow
    ),
    components(schemas(
        CountReadiness,
        CountSelection,
        CountWarehouse,
        CountBranch,
        CountProduct,
        CountLot,
        CountDecision,
        CountRecord,
        CountWorkspace,
        CountGenericDetail,
        CountGenericValue,
        CountFlowPerson,
        CountFlowStep,
        CountFlowHistory,
        CountFlow,
        CountActionResult,
        CountCreateResult,
        CountAssignmentResult,
        ServiceInitialCountReadData,
        ServiceInitialCountCommandData,
        ServiceInitialCountReadResponse,
        ServiceInitialCountCommandResponse,
        CountFlowKind,
        CountSourceKind,
        CountUnavailableReason,
        CountCreateInput,
        CountDraftInput,
        CountLineInput,
        CountAllocationInput,
        CountUpdateInput,
        CountUpdateLine,
        CountUpdateAllocation,
        CountActionInput,
        CountAssignmentInput,
        GenericCountActionInput,
        GenericCountAssignmentInput,
        FieldServiceRenderedContext
    ))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut document = Document::openapi();
        document.info.title = "ERPX native Service initial physical Count".into();
        let mut value = serde_json::to_value(document).unwrap();
        value["paths"]["/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/update"]["post"]["requestBody"]["description"] = serde_json::json!("The private ingress accepts at most 2 MiB. Unknown object keys are stripped by the legacy-compatible update projection.");
        value["components"]["schemas"]["CountCreateInput"]["properties"]["expectedVersion"] =
            serde_json::json!({"type":"integer","const":0});
        value["components"]["schemas"]["CountAssignmentInput"]["properties"]["saveDefault"] =
            serde_json::json!({"type":"boolean","const":false});
        if let Some(variants)=value["components"]["schemas"]["CountGenericValue"]["anyOf"].as_array_mut() {
            for variant in variants { if variant["type"] == "integer" { variant["minimum"]=(-9007199254740991i64).into(); variant["maximum"]=9007199254740991i64.into(); } }
        }
        let update = &mut value["components"]["schemas"];
        update["CountUpdateInput"]["properties"]["warehouseId"]["minLength"] = 1.into();
        update["CountUpdateInput"]["properties"]["countDate"]["format"] = "date".into();
        update["CountUpdateInput"]["properties"]["reason"]["minLength"] = 5.into();
        update["CountUpdateInput"]["properties"]["reason"]["maxLength"] = 2000.into();
        update["CountUpdateLine"]["properties"]["productId"]["minLength"] = 1.into();
        update["CountUpdateAllocation"]["properties"]["lotId"]["minLength"] = 1.into();
        for (key, maximum) in [("lotCode", 100), ("serialNumber", 160)] {
            update["CountUpdateAllocation"]["properties"][key]["minLength"] = 1.into();
            update["CountUpdateAllocation"]["properties"][key]["maxLength"] = maximum.into();
        }
        for name in ["CountUpdateLine", "CountUpdateAllocation"] {
            update[name]["properties"]["countedQuantity"]["pattern"] = r"^\d{1,12}(\.\d{1,3})?$".into();
            update[name]["properties"]["unitCost"]["pattern"] = r"^\d{1,12}(\.\d{1,2})?$".into();
        }
        for key in ["manufacturedDate", "expiryDate"] {
            update["CountUpdateAllocation"]["properties"][key] = serde_json::json!({"oneOf":[{"type":"string","format":"date"},{"type":"string","const":""}],"default":""});
        }
        update["CountUpdateLine"]["properties"]["allocations"]["default"] = serde_json::json!([]);
        let generic = &mut value["components"]["schemas"];
        generic["GenericCountActionInput"]["properties"]["attachmentSetIds"]["items"]["minLength"] = 1.into();
        generic["GenericCountActionInput"]["properties"]["attachmentSetIds"]["default"] = serde_json::json!([]);
        for name in ["GenericCountActionInput", "GenericCountAssignmentInput"] {
            generic[name]["properties"]["reason"]["minLength"] = 3.into();
            generic[name]["properties"]["reason"]["maxLength"] = 2000.into();
        }
        generic["GenericCountAssignmentInput"]["properties"]["stepKey"]["minLength"] = 1.into();
        generic["GenericCountAssignmentInput"]["properties"]["stepKey"]["maxLength"] = 100.into();
        generic["GenericCountAssignmentInput"]["properties"]["userId"]["minLength"] = 1.into();
        generic["GenericCountAssignmentInput"]["properties"]["userId"]["maxLength"] = 180.into();
        generic["GenericCountAssignmentInput"]["properties"]["saveDefault"]["default"] = false.into();
        if let Some(schemas) = value["components"]["schemas"].as_object_mut() {
            for name in [
                "CountCreateInput",
                "CountDraftInput",
                "CountLineInput",
                "CountAllocationInput",
                "CountActionInput",
                "CountAssignmentInput",
            ] {
                schemas.get_mut(name).unwrap()["additionalProperties"] = false.into();
            }
        }
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
        return;
    }
    assert!(args.is_empty());
    let config = Config::new();
    println!(
        "// Generated from the Rust Service initial physical Count contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-field-service-initial-count-contracts.ts\nimport type {{ FieldServiceRenderedContext }} from './field-service-reserve.generated';"
    );
    for declaration in [
        CountReadiness::decl(&config),
        CountSelection::decl(&config),
        CountWarehouse::decl(&config),
        CountBranch::decl(&config),
        CountProduct::decl(&config),
        CountLot::decl(&config),
        CountDecision::decl(&config),
        CountRecord::decl(&config),
        CountWorkspace::decl(&config),
        CountGenericValue::decl(&config),
        CountGenericDetail::decl(&config),
        CountFlowPerson::decl(&config),
        CountFlowStep::decl(&config),
        CountFlowHistory::decl(&config),
        CountFlow::decl(&config),
        CountActionResult::decl(&config),
        CountCreateResult::decl(&config),
        CountAssignmentResult::decl(&config),
        ServiceInitialCountReadData::decl(&config),
        ServiceInitialCountCommandData::decl(&config),
        ServiceInitialCountReadResponse::decl(&config),
        ServiceInitialCountCommandResponse::decl(&config),
        CountFlowKind::decl(&config),
        CountSourceKind::decl(&config),
        CountUnavailableReason::decl(&config),
        CountCreateInput::decl(&config),
        CountDraftInput::decl(&config),
        CountLineInput::decl(&config),
        CountAllocationInput::decl(&config),
        CountUpdateInput::decl(&config),
        CountUpdateLine::decl(&config),
        CountUpdateAllocation::decl(&config),
        CountActionInput::decl(&config),
        CountAssignmentInput::decl(&config),
        GenericCountActionInput::decl(&config),
        GenericCountAssignmentInput::decl(&config),
    ] {
        println!("export {declaration}");
    }
}
