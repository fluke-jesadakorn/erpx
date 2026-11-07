use erpx_server_commercial::public::{purchases::*,package_review::PackageReview};
use erpx_service::{field_service_package_settlement::{NativeServicePackagePreview,NativeServicePackageOverview},field_service_package_http::{NativeServiceCompany,NativeServiceCompanies,ServicePackagePreviewRequest,ServicePackageApplyRequest}};
use ts_rs::{Config,TS};
#[derive(utoipa::OpenApi)]
#[openapi(components(schemas(PackageDefinition,CompanyPackageGrant,CompanyPackageHistoryRight,CompanyPurchaseFacts,BatchChange,GrantBatch,GrantBatchPreview,PackageReview,NativeServicePackagePreview,NativeServicePackageOverview,NativeServiceCompany,NativeServiceCompanies,ServicePackagePreviewRequest,ServicePackageApplyRequest)))]
struct Document;
fn main(){
 let args=std::env::args().skip(1).collect::<Vec<_>>();
 if args==["--openapi"] {use utoipa::OpenApi; let mut doc=Document::openapi();doc.info.title="ERPX native Service package lifecycle".into();doc.info.description=Some("DTO contracts for the native Service/service-parts-assets lifecycle. Current Platform authority, native MFA, source guards and retained transactions are required by the consuming runtime. Export does not establish owner acceptance.".into());println!("{}",doc.to_pretty_json().unwrap());return;}
 assert!(args.is_empty());let c=Config::new().with_large_int("number");
 println!("// Generated from the Rust Service Commercial contracts. Do not edit by hand.\n// Regenerate: bun scripts/docs/generate-commercial-service-contracts.ts");
 for d in [PackageDefinition::decl(&c),CompanyPackageGrant::decl(&c),CompanyPackageHistoryRight::decl(&c),CompanyPurchaseFacts::decl(&c),BatchChange::decl(&c),GrantBatch::decl(&c),GrantBatchPreview::decl(&c),PackageReview::decl(&c),NativeServicePackagePreview::decl(&c),NativeServicePackageOverview::decl(&c),NativeServiceCompany::decl(&c),NativeServiceCompanies::decl(&c),ServicePackagePreviewRequest::decl(&c),ServicePackageApplyRequest::decl(&c)]{println!("export {d}");}
}
