use erpx_service::field_attachment::{
    FieldAttachmentBindInput, FieldAttachmentBindResponse, FieldAttachmentBindResult,
    FieldAttachmentManifestItem, FieldAttachmentScan, FieldAttachmentStatus,
    FieldAttachmentStatusResponse, FieldAttachmentWithdrawInput,
};
use erpx_service::field_service_reservation_read::FieldServiceRenderedContext;
use ts_rs::{Config, TS};
#[utoipa::path(get,path="/api/v1/holdings/{holding_slug}/attachment-links/{link_id}/status",params(("holding_slug"=String,Path),("link_id"=String,Path)),responses((status=200,body=FieldAttachmentStatusResponse,description="Current authorized attachment status")))]
fn status() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/attachment-links/{link_id}/withdraw",params(("holding_slug"=String,Path),("link_id"=String,Path)),request_body=FieldAttachmentWithdrawInput,responses((status=200,body=FieldAttachmentStatusResponse,description="Canonical status after withdrawal")))]
fn withdraw() {}
#[utoipa::path(post,path="/api/v1/holdings/{holding_slug}/attachment-sets/{set_id}/bind",params(("holding_slug"=String,Path),("set_id"=String,Path)),request_body=FieldAttachmentBindInput,responses((status=200,body=FieldAttachmentBindResponse,description="Bound Field visit record evidence")))]
fn bind() {}
const _: fn() = bind;
const _: fn() = status;
const _: fn() = withdraw;
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(status, withdraw, bind),
    components(schemas(
        FieldAttachmentBindInput,
        FieldAttachmentManifestItem,
        FieldAttachmentBindResult,
        FieldAttachmentBindResponse,
        FieldAttachmentWithdrawInput,
        FieldAttachmentScan,
        FieldAttachmentStatus,
        FieldAttachmentStatusResponse,
        FieldServiceRenderedContext
    ))
)]
struct Api;
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--openapi"] {
        use utoipa::OpenApi;
        println!("{}", serde_json::to_string_pretty(&Api::openapi()).unwrap());
        return;
    }
    assert!(args.is_empty());
    let c = Config::new();
    println!("// Generated from Rust Field attachment contracts. Do not edit by hand.");
    for declaration in [
        FieldServiceRenderedContext::decl(&c),
        FieldAttachmentBindInput::decl(&c),
        FieldAttachmentManifestItem::decl(&c),
        FieldAttachmentBindResult::decl(&c),
        FieldAttachmentBindResponse::decl(&c),
        FieldAttachmentWithdrawInput::decl(&c),
        FieldAttachmentScan::decl(&c),
        FieldAttachmentStatus::decl(&c),
        FieldAttachmentStatusResponse::decl(&c),
    ] {
        println!("export {declaration}");
    }
}
