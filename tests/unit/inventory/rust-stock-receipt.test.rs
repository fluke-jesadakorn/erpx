//! Portable goods-receipt guards for the Commerce-owned purchase command.
//! These cases do not prove IAM, purchase, Quality or a PostgreSQL effect.
use erpx_server_inventory::public::stock_receipt::*;

fn source(tracking: &str, lots: Vec<StockReceiptLot>) -> StockReceiptSource {
    StockReceiptSource {
        company_id: "company".into(),
        order: StockReceiptOrder {
            id: "order-1".into(),
            number: "PO-1".into(),
            kind: "purchase".into(),
            state: "confirmed".into(),
            version: 3,
            warehouse_id: "warehouse".into(),
            branch_id: "branch".into(),
            department_id: None,
            order_date: "2026-11-01".into(),
            currency: "THB".into(),
            party_name: "Supplier".into(),
        },
        lines: vec![StockReceiptLine {
            id: "line-1".into(),
            product_id: "product".into(),
            sku: "SKU".into(),
            description: "Product".into(),
            unit: "pcs".into(),
            tracking_mode: tracking.into(),
            quantity_milli: 10_000,
            fulfilled_milli: 0,
            remaining_milli: 10_000,
            price_minor: 2_500,
            tax_bps: 0,
            ordinal: 1,
            stock: Some(StockReceiptStock {
                quantity_milli: 1_000,
                value_minor: 1_000,
                version: 2,
                reserved_milli: 0,
                quarantined_milli: 0,
            }),
            lots,
            lots_truncated: false,
        }],
    }
}

fn allocation(
    lot: Option<&str>,
    serial: Option<&str>,
    quantity: i64,
    disposition: ReceiptDisposition,
) -> StockReceiptAllocationInput {
    StockReceiptAllocationInput {
        lot_code: lot.map(Into::into),
        serial_number: serial.map(Into::into),
        quantity_milli: quantity,
        disposition,
        quarantine_reason: (disposition == ReceiptDisposition::Quarantine)
            .then(|| "Packaging inspection".into()),
        quarantine_due_date: (disposition == ReceiptDisposition::Quarantine)
            .then(|| "2026-11-10".into()),
        manufactured_date: None,
        expiry_date: None,
    }
}

fn input(
    quantity: i64,
    value: i64,
    allocations: Vec<StockReceiptAllocationInput>,
) -> StockReceiptInput {
    StockReceiptInput {
        expected_order_version: 3,
        date: "2026-11-04".into(),
        lines: vec![StockReceiptLineInput {
            line_id: "line-1".into(),
            quantity_milli: quantity,
            stock_value_minor: value,
            allocations,
        }],
    }
}

#[test]
fn receipt_quarantine_policy_matches_installed_legacy_schema_and_application_guard() {
    use erpx_server_commerce::public::receipt::{ReceiptAllocationCommand,ReceiptCommand,ReceiptDisposition as CommerceDisposition,ReceiptLineCommand};
    use erpx_server_shared::public::{CommandKey,RecordId};
    use std::process::Command;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let node=std::env::var_os("ERPX_NODE_BIN").map(std::path::PathBuf::from).unwrap_or_else(||root.join(".runtime/node/node-v26.11.1-darwin-arm64/bin/node"));
    let script=r#"if(process.version!=='v26.11.1')throw Error('NODE_VERSION');const fs=require('node:fs'),z=require('zod');if(require('zod/package.json').version!=='4.6.7')throw Error('ZOD_VERSION');const s=fs.readFileSync('packages/server-commerce/src/domain/commerce.ts','utf8');const q=s.slice(s.indexOf('const quantity = z'),s.indexOf('export const orderSchema'));const f=s.slice(s.indexOf('export const fulfillmentSchema'),s.indexOf('export const invoiceFromDeliverySchema')).replace('export const fulfillmentSchema','const fulfillmentSchema');const schema=new Function('z',q+f+';return fulfillmentSchema')(z);const app=fs.readFileSync('packages/server-commerce/src/application/commerce.ts','utf8');const guard="if (incoming && allocation.disposition === 'quarantine' && (!allocation.quarantineReason || !allocation.quarantineDueDate))";if(!app.includes(guard))throw Error('GUARD_CHANGED');const reject=new Function('incoming','allocation','return '+guard.slice(4,-1));const parsed=schema.safeParse(JSON.parse(process.argv[1]));process.stdout.write(parsed.success&&!reject(true,parsed.data.lines[0].allocations[0])?'accepted':'rejected');"#;
    let cases=[(true,Some(" x "),Some("2020-01-01")),(true,Some("😀"),Some("0000-02-29")),(true,Some("  "),Some("2026-11-10")),(true,Some("x"),None),(true,Some("x"),Some("2026-02-30")),(false,Some(" ignored "),Some("2020-01-01")),(false,Some(""),Some(""))];
    for (quarantine,reason,due) in cases {
        let disposition=if quarantine {"quarantine"} else {"available"};
        let mut item=serde_json::json!({"lotCode":"L-A","quantity":"1.000","disposition":disposition});
        if let Some(reason)=reason {item["quarantineReason"]=reason.into();}
        if let Some(due)=due {item["quarantineDueDate"]=due.into();}
        let raw=serde_json::json!({"expectedVersion":3,"date":"2026-11-04","lines":[{"lineId":"line-1","quantity":"1.000","allocations":[item]}]}).to_string();
        let output=Command::new(&node).current_dir(&root).arg("-e").arg(script).arg(raw).output().expect("pinned Node legacy oracle");
        assert!(output.status.success(),"legacy schema/guard oracle failed");
        let expected=output.stdout==b"accepted";
        let inventory_disposition=if quarantine {ReceiptDisposition::Quarantine} else {ReceiptDisposition::Available};
        let mut inventory=allocation(Some("L-A"),None,1000,inventory_disposition);
        inventory.quarantine_reason=reason.map(Into::into);inventory.quarantine_due_date=due.map(Into::into);
        let result=prepare_stock_receipt(&source("lot",vec![]),&input(1000,1000,vec![inventory]));
        assert_eq!(result.is_ok(),expected,"inventory {disposition} {reason:?} {due:?}");
        if let Ok(mut plan)=result { if !quarantine {
            assert_eq!(plan.lines[0].allocations[0].quarantine_reason,None);
            plan.lines[0].allocations[0].quarantine_reason=reason.map(Into::into);
            assert!(validate_stock_receipt_plan(&plan).is_ok());
        } }
        let commerce=ReceiptCommand {key:CommandKey::new("receipt-key").unwrap(),order_id:RecordId::new("order-1").unwrap(),expected_version:3,date:"2026-11-04".into(),lines:vec![ReceiptLineCommand {line_id:"line-1".into(),quantity_milli:1000,allocations:vec![ReceiptAllocationCommand {lot_code:Some("L-A".into()),serial_number:None,quantity_milli:1000,disposition:if quarantine {CommerceDisposition::Quarantine}else{CommerceDisposition::Available},quarantine_reason:reason.map(Into::into),quarantine_due_date:due.map(Into::into),manufactured_date:None,expiry_date:None}]}]};
        assert_eq!(commerce.validate().is_ok(),expected,"commerce {disposition} {reason:?} {due:?}");
    }
}

#[test]
fn prepares_a_mixed_available_and_quarantine_receipt() {
    let plan = prepare_stock_receipt(
        &source("lot", vec![]),
        &input(
            3_000,
            1_000,
            vec![
                allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                allocation(Some("L-B"), None, 1_000, ReceiptDisposition::Quarantine),
                allocation(Some("L-C"), None, 1_000, ReceiptDisposition::Available),
            ],
        ),
    )
    .unwrap();
    assert_eq!(plan.company_id, "company");
    assert_eq!(plan.warehouse_id, "warehouse");
    assert_eq!(plan.branch_id, "branch");
    assert_eq!(plan.total_value_minor, 1_000);
    assert_eq!(plan.lines[0].expected_stock_version, Some(2));
    assert_eq!(
        plan.lines[0]
            .allocations
            .iter()
            .map(|allocation| allocation.value_minor)
            .collect::<Vec<_>>(),
        vec![333, 333, 334]
    );
    let request = quality_request(
        &plan,
        &plan.lines[0],
        &plan.lines[0].allocations[1],
        "lot-allocation-1".into(),
        "1".into(),
        "lot-1".into(),
    )
    .unwrap();
    assert_eq!(request.lot_identity, "L-B");
    assert_eq!(request.allocation_id, "lot-allocation-1");
    assert_eq!(request.source_id, "1");
    assert_eq!(request.reason, "Packaging inspection");
    assert_eq!(request.due_date, "2026-11-10");
}

#[test]
fn rejects_untracked_allocations_serial_quantities_and_duplicate_identities() {
    assert_eq!(
        prepare_stock_receipt(
            &source("none", vec![]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L"),
                    None,
                    1_000,
                    ReceiptDisposition::Available
                )]
            )
        )
        .unwrap_err()
        .code,
        "LOT_ALLOCATION_UNEXPECTED"
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("serial", vec![]),
            &input(
                2_000,
                2_000,
                vec![allocation(
                    None,
                    Some("SN-1"),
                    2_000,
                    ReceiptDisposition::Available
                )]
            )
        )
        .unwrap_err()
        .code,
        "SERIAL_QUANTITY_INVALID"
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![]),
            &input(
                2_000,
                2_000,
                vec![
                    allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                    allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                ]
            )
        )
        .unwrap_err()
        .code,
        "LOT_ALLOCATION_DUPLICATE"
    );
}

#[test]
fn rejects_stale_order_version_and_over_receipt() {
    let mut stale = input(
        1_000,
        1_000,
        vec![allocation(
            Some("L-A"),
            None,
            1_000,
            ReceiptDisposition::Available,
        )],
    );
    stale.expected_order_version = 2;
    assert_eq!(
        prepare_stock_receipt(&source("lot", vec![]), &stale)
            .unwrap_err()
            .code,
        "STOCK_RECEIPT_SOURCE_CHANGED"
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![]),
            &input(
                11_000,
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    11_000,
                    ReceiptDisposition::Available
                )]
            )
        )
        .unwrap_err()
        .code,
        "OVER_FULFILLMENT"
    );
}

#[test]
fn rejects_a_tampered_plan_before_the_write_boundary() {
    let plan = prepare_stock_receipt(
        &source("lot", vec![]),
        &input(
            3_000,
            1_000,
            vec![
                allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                allocation(Some("L-B"), None, 1_000, ReceiptDisposition::Quarantine),
                allocation(Some("L-C"), None, 1_000, ReceiptDisposition::Available),
            ],
        ),
    )
    .unwrap();
    validate_stock_receipt_plan(&plan).unwrap();

    let mut total = plan.clone();
    total.total_value_minor += 1;
    assert_eq!(
        validate_stock_receipt_plan(&total).unwrap_err().code,
        "STOCK_RECEIPT_VALUE_INVALID"
    );

    let mut quantity = plan.clone();
    quantity.lines[0].allocations[0].quantity_milli -= 1;
    assert_eq!(
        validate_stock_receipt_plan(&quantity).unwrap_err().code,
        "LOT_ALLOCATION_REQUIRED"
    );

    let mut index = plan.clone();
    index.lines[0].allocations[1].allocation_index = 0;
    assert_eq!(
        validate_stock_receipt_plan(&index).unwrap_err().code,
        "STOCK_RECEIPT_PLAN_INVALID"
    );

    let mut duplicate = plan.clone();
    duplicate.lines.push(plan.lines[0].clone());
    assert_eq!(
        validate_stock_receipt_plan(&duplicate).unwrap_err().code,
        "DUPLICATE_LINE"
    );

    let mut evidence = plan.clone();
    evidence.lines[0].allocations[1].quarantine_reason = None;
    assert_eq!(
        validate_stock_receipt_plan(&evidence).unwrap_err().code,
        "QUARANTINE_EVIDENCE_REQUIRED"
    );

    let mut untracked = plan;
    untracked.lines[0].tracking_mode = "none".into();
    assert_eq!(
        validate_stock_receipt_plan(&untracked).unwrap_err().code,
        "LOT_ALLOCATION_UNEXPECTED"
    );
}

#[test]
fn refuses_available_receipt_into_a_quarantined_lot_but_allows_quarantine() {
    let held = StockReceiptLot {
        id: "lot-1".into(),
        lot_code: Some("L-A".into()),
        serial_number: None,
        status: "active".into(),
        quantity_milli: 1_000,
        value_minor: 1_000,
        version: 2,
        quality_hold_milli: 1_000,
        quality_status: Some("held".into()),
    };
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![held.clone()]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    1_000,
                    ReceiptDisposition::Quarantine
                )]
            )
        )
        .unwrap()
        .lines[0]
            .allocations[0]
            .existing_lot_id
            .as_deref(),
        Some("lot-1")
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![held]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    1_000,
                    ReceiptDisposition::Available
                )]
            )
        )
        .unwrap_err()
        .code,
        "LOT_UNAVAILABLE"
    );
}

#[test]
fn trims_receipt_identities_before_dedupe_and_lookup() {
    let plan = prepare_stock_receipt(
        &source("lot", vec![]),
        &input(
            1_000,
            1_000,
            vec![allocation(
                Some("  L-A  "),
                None,
                1_000,
                ReceiptDisposition::Available,
            )],
        ),
    )
    .unwrap();
    assert_eq!(
        plan.lines[0].allocations[0].lot_code.as_deref(),
        Some("L-A")
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![]),
            &input(
                2_000,
                2_000,
                vec![
                    allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                    allocation(
                        Some("\u{00a0}L-A"),
                        None,
                        1_000,
                        ReceiptDisposition::Available
                    ),
                ],
            ),
        )
        .unwrap_err()
        .code,
        "LOT_ALLOCATION_DUPLICATE"
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("   "),
                    None,
                    1_000,
                    ReceiptDisposition::Available,
                )],
            ),
        )
        .unwrap_err()
        .code,
        "STOCK_RECEIPT_IDENTITY_REQUIRED"
    );
}

#[test]
fn receipt_identities_use_utf16_bounds_like_the_transport() {
    let emoji_lot = "😀".repeat(40);
    assert!(prepare_stock_receipt(
        &source("lot", vec![]),
        &input(
            1_000,
            1_000,
            vec![allocation(
                Some(emoji_lot.as_str()),
                None,
                1_000,
                ReceiptDisposition::Available,
            )],
        ),
    )
    .is_ok());
}
