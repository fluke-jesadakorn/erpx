use erpx_server_finance::public::journal_detail::*;
use erpx_service::finance_journal_detail::{
    FinanceJournalReadResponse, FinanceRenderedWorkContext,
};
use ts_rs::{Config, TS};
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/finance/journals/{id}",params(("holding_slug"=String,Path,description="Exact registered Holding slug"),("id"=String,Path,description="Journal identity")),responses((status=200,body=FinanceJournalReadResponse,description="Journal detail and original render context from one retained authorization unit"),(status=401,description="Current session required"),(status=403,description="Finance read/purchase history denied"),(status=404,description="Unavailable or unsupported source"),(status=409,description="Supplied rendered context is stale")))]
fn get_detail() {}
const _: fn() = get_detail;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(get_detail),
    components(schemas(
        FinanceReadInteger,
        FinanceLedgerValue,
        FinanceJournalDetail,
        FinanceRenderedWorkContext,
        FinanceJournalReadResponse
    ))
)]
struct Document;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        let mut doc = Document::openapi();
        doc.info.title = "ERPX native Finance journal detail".into();
        doc.info.description=Some("Development-only unmounted detail component. Current finance.read/module and actual Commercial History remain required; no post/reviewer grant. First source scope admits proved Finance manual/opening/manual-root reversal records; missing foreign-owner/secondary-book facts deny. Render context is output data from the same retained authorization unit, never an input authority. No installed route/current page or release acceptance.".into());
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
        FinanceRenderedWorkContext::decl(&configuration),
        FinanceJournalReadResponse::decl(&configuration),
    ] {
        println!("export {d}");
    }
}
