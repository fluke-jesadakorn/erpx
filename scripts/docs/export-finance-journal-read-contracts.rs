use erpx_server_finance::public::{
    journal_detail::*, journal_workspace::FinanceJournalRecordWorkspace,
};
use erpx_service::finance_journal_detail::{
    FinanceJournalReadResponse, FinanceJournalWorkspaceReadResponse, FinanceRenderedWorkContext,
};
use ts_rs::{Config, TS};
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/finance/journals/{id}",params(("holding_slug"=String,Path,description="Exact registered Holding slug"),("id"=String,Path,description="Journal identity")),responses((status=200,body=FinanceJournalReadResponse,description="Journal detail and original render context from one retained authorization unit"),(status=401,description="Current session required"),(status=403,description="Finance read/purchase history denied"),(status=404,description="Unavailable or unsupported source"),(status=409,description="Supplied rendered context is stale")))]
fn get_detail() {}
const _: fn() = get_detail;
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/finance/journals/{id}/workspace",params(("holding_slug"=String,Path),("id"=String,Path)),responses((status=200,body=FinanceJournalWorkspaceReadResponse,description="Exact record workspace references and original render context from one retained unit"),(status=401,description="Current session required"),(status=403,description="Finance read/purchase History denied"),(status=404,description="Unavailable or unsupported source"),(status=409,description="Supplied rendered context is stale")))]
fn get_workspace() {}
const _: fn() = get_workspace;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(get_detail, get_workspace),
    components(schemas(
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
        doc.info.title = "ERPX native Finance journal reads".into();
        doc.info.description=Some("Development-only unmounted detail and record-workspace component. Current finance.read/module and actual Commercial History remain required; no post/reviewer grant. First source scope admits proved Finance manual/opening/manual-root reversal records; missing foreign-owner/secondary-book facts deny. Render context is output data from the same retained authorization unit, never an input authority. No installed route/current page or release acceptance.".into());
        println!("{}", doc.to_pretty_json().unwrap());
        return;
    }
    assert!(args.is_empty());
    let configuration = Config::new();
    println!(
        "// Generated from the Rust Finance journal read contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-finance-journal-read-contracts.ts"
    );
    for d in [
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
