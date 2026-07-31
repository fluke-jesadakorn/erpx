use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::procurement_inventory::{
    ProductReference, PurchaseOrder, PurchaseOrderLine, PurchaseRequest, PurchaseRequestLine,
    ScmError, VendorReference,
};
use uuid::Uuid;

#[test]
fn new_purchase_orders_cannot_bypass_goods_receipts_with_prefilled_quantity() {
    let holding_id = Uuid::from_u128(1);
    let company_id = Uuid::from_u128(2);
    let branch_id = Uuid::from_u128(3);
    let scope = TenantScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let context = CommandContext::try_new(
        "procurement-order-create",
        ActorAttribution {
            principal_ref: "procurement-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope.clone(),
        "exercise purchase-order receipt boundary",
    )
    .expect("valid command context");
    let product_id = Uuid::from_u128(10);
    let product = ProductReference {
        product_id,
        product_reference: "PROD-0000000000000000000000000000000A".into(),
        scope: scope.clone(),
        holding_id,
        company_id,
        branch_id: Some(branch_id),
    };
    let vendor_id = Uuid::from_u128(11);
    let vendor = VendorReference::new(
        vendor_id,
        TenantScope::Company {
            holding_id,
            company_id,
        },
        holding_id,
        company_id,
        None,
        "Test vendor",
    )
    .expect("valid vendor");
    let mut request = PurchaseRequest::new(
        Uuid::from_u128(12),
        scope.clone(),
        holding_id,
        company_id,
        Some(branch_id),
        vendor,
        "THB",
        vec![PurchaseRequestLine {
            line_id: Uuid::from_u128(13),
            product: product.clone(),
            quantity: 10,
            estimated_unit_cost_minor: 100,
        }],
        chrono::DateTime::UNIX_EPOCH,
        &context,
    )
    .expect("valid request");
    request
        .submit(&context, chrono::DateTime::UNIX_EPOCH)
        .expect("submit request");
    request
        .approve(&context, chrono::DateTime::UNIX_EPOCH)
        .expect("approve request");

    assert_eq!(
        PurchaseOrder::new_from_request(
            Uuid::from_u128(14),
            &request,
            vec![PurchaseOrderLine {
                line_id: Uuid::from_u128(13),
                product,
                ordered_quantity: 10,
                unit_cost_minor: 100,
                received_quantity: 1,
            }],
            chrono::DateTime::UNIX_EPOCH,
            &context,
        ),
        Err(ScmError::PurchaseOrderStartsWithReceivedQuantity)
    );
}
