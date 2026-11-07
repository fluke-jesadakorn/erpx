use erpx_service::finance_field_journal_flow::{FinanceFieldJournalFlowAccess,FinanceFieldJournalFlowRuntime,FinanceFieldJournalFlowResponse,FinanceFieldJournalFlowInput,FinanceFieldJournalFlowResult,FinanceFieldJournalFlowCommandResponse};
use erpx_server_finance::public::service_no_charge::{ServiceNoChargeControls,ServiceNoChargeInput,ServiceNoChargeResult,ServiceNoChargeStatus,ServiceNoChargeResponse};
use erpx_service::finance_presentation::FinancePurchaseAccess;
use erpx_server_finance::public::{
    field_service_invoice_read::{
        FinanceInvoiceOptions, FinanceServiceInvoiceSource, FinanceServiceInvoiceWorkspace,
    },
    journal_detail::*,
    journal_workspace::FinanceJournalRecordWorkspace,
};
use erpx_service::finance_journal_detail::{
    FinanceJournalReadResponse, FinanceJournalWorkspaceReadResponse, FinanceRenderedWorkContext,
    FinanceServiceInvoiceWorkspaceResponse, FinanceFieldWorkResponse,
};
use ts_rs::{Config, TS};
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/finance/journals/{id}",params(("holding_slug"=String,Path,description="Exact registered Holding slug"),("id"=String,Path,description="Journal identity")),responses((status=200,body=FinanceJournalReadResponse,description="Journal detail and original render context from one retained authorization unit"),(status=401,description="Current session required"),(status=403,description="Finance read/purchase history denied"),(status=404,description="Unavailable or unsupported source"),(status=409,description="Supplied rendered context is stale")))]
fn get_detail() {}
const _: fn() = get_detail;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/finance/journals/{id}/workspace",params(("holding_slug"=String,Path),("id"=String,Path)),responses((status=200,body=FinanceJournalWorkspaceReadResponse,description="Exact record workspace references and original render context from one retained unit"),(status=401,description="Current session required"),(status=403,description="Finance read/purchase History denied"),(status=404,description="Unavailable or unsupported source"),(status=409,description="Supplied rendered context is stale")))]
fn get_workspace() {}
const _: fn() = get_workspace;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/finance/invoices/{id}/workspace",params(("holding_slug"=String,Path),("id"=String,Path),("purpose"=Option<String>,Query,description="Optional payment-evidence admission: source-qualified Field receivables return PAYABLE_REQUIRED422; this never grants supplier-bill write authority")),responses((status=422,description="PAYABLE_REQUIRED for Field receivables requesting supplier-payment evidence"),(status=200,body=FinanceServiceInvoiceWorkspaceResponse,description="Current source-qualified Service invoice workspace"),(status=401,description="Current session required"),(status=403,description="Finance read or purchase History denied"),(status=404,description="Unavailable or unsupported source")))]
fn get_service_invoice_workspace() {}
const _: fn() = get_service_invoice_workspace;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/finance/field-work",params(("holding_slug"=String,Path)),responses((status=200,body=FinanceFieldWorkResponse,description="Source-qualified native Field invoice and spare-cost journal Work with the captured rendering context"),(status=401,description="Current Account session required"),(status=403,description="Finance read denied"),(status=409,description="Source or selected identity changed")))]
fn get_field_work(){}
const _:fn()=get_field_work;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/finance/invoices/{id}/confirm-no-charge",params(("holding_slug"=String,Path),("id"=String,Path)),request_body=ServiceNoChargeInput,responses((status=200,body=ServiceNoChargeResponse,description="Exact independently confirmed Field zero-demand receipt"),(status=403,description="Current independent Finance reviewer required"),(status=409,description="Source, context, responsibility or retained proof changed")))]
fn confirm_service_no_charge() {}
const _:fn()=confirm_service_no_charge;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/finance/journals/{id}/flow",params(("holding_slug"=String,Path),("id"=String,Path)),responses((status=200,body=FinanceFieldJournalFlowResponse,description="Source-verified native Field journal Flow or explicit non-Field legacy partition; rendered context from the same authorization unit"),(status=401,description="Current session required"),(status=403,description="Finance source history denied"),(status=409,description="Rendered context or source changed")))]
fn get_field_journal_flow(){}
const _:fn()=get_field_journal_flow;
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/finance/journals/{id}/flow",params(("holding_slug"=String,Path),("id"=String,Path)),request_body=FinanceFieldJournalFlowInput,responses((status=200,body=FinanceFieldJournalFlowCommandResponse,description="Exact native Field journal responsibility receipt"),(status=403,description="Current Company administrator required"),(status=409,description="Source, responsibility or rendered context changed")))]
fn assign_field_journal_flow(){}
const _:fn()=assign_field_journal_flow;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(get_detail, get_workspace, get_service_invoice_workspace,get_field_work,confirm_service_no_charge,get_field_journal_flow,assign_field_journal_flow),
    components(schemas(
        FinanceFieldJournalFlowAccess,
        FinanceFieldJournalFlowRuntime,
        FinanceFieldJournalFlowResponse,
        FinanceFieldJournalFlowInput,
        FinanceFieldJournalFlowResult,
        FinanceFieldJournalFlowCommandResponse,
        ServiceNoChargeInput,
        ServiceNoChargeResult,
        ServiceNoChargeResponse,
        ServiceNoChargeStatus,
        ServiceNoChargeControls,
        FinancePurchaseAccess,
        FinanceInvoiceOptions,
        FinanceServiceInvoiceSource,
        FinanceServiceInvoiceWorkspace,
        FinanceServiceInvoiceWorkspaceResponse,
        FinanceFieldWorkResponse,
        FinanceReadInteger,
        FinanceLedgerValue,
        FinanceJournalDetail,
        FinanceJournalRecordWorkspace,
        FinanceRenderedWorkContext,
        FinanceJournalReadResponse,
        FinanceJournalWorkspaceReadResponse
    ))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut doc = Document::openapi();
        doc.info.title = "ERPX native Finance records".into();
        doc.info.description=Some("Native Finance journal/invoice reads and Field no-charge confirmation on explicitly configured development targets. Reads require current finance.read/module and Commercial History. Confirmation additionally requires current finance.write/approve, independent invoice responsibility, same-unit Field proof and exact receipt/audit/outbox. Render context never grants authority. Contract export alone establishes no owner or production acceptance.".into());
        println!("{}", doc.to_pretty_json().unwrap());
        return;
    }
    assert!(args.is_empty());
    let configuration = Config::new();
    println!(
        "// Generated from the Rust Finance journal read contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-finance-journal-read-contracts.ts"
    );
    for d in [
        FinanceFieldJournalFlowAccess::decl(&configuration),
        FinanceFieldJournalFlowRuntime::decl(&configuration),
        FinanceFieldJournalFlowResponse::decl(&configuration),
        FinanceFieldJournalFlowInput::decl(&configuration),
        FinanceFieldJournalFlowResult::decl(&configuration),
        FinanceFieldJournalFlowCommandResponse::decl(&configuration),
        FinancePurchaseAccess::decl(&configuration),
        FinanceInvoiceOptions::decl(&configuration),
        FinanceServiceInvoiceSource::decl(&configuration),
        FinanceServiceInvoiceWorkspace::decl(&configuration),
        ServiceNoChargeInput::decl(&configuration),
        ServiceNoChargeStatus::decl(&configuration),
        ServiceNoChargeResult::decl(&configuration),
        ServiceNoChargeResponse::decl(&configuration),
        ServiceNoChargeControls::decl(&configuration),
        FinanceServiceInvoiceWorkspaceResponse::decl(&configuration),
        FinanceFieldWorkResponse::decl(&configuration),
        FinanceReadInteger::decl(&configuration),
        FinanceLedgerValue::decl(&configuration),
        FinanceJournalDetail::decl(&configuration),
        FinanceJournalRecordWorkspace::decl(&configuration),
        FinanceRenderedWorkContext::decl(&configuration),
        FinanceJournalReadResponse::decl(&configuration),
        FinanceJournalWorkspaceReadResponse::decl(&configuration),
    ] {
        println!("export {d}");
    }
}
