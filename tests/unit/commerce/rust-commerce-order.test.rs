//! Native Commerce quotation/sales-order policy and sequencing.
use erpx_server_commerce::public::order::{
    AuditEntry, CommerceOrderActor, CommerceOrderCommandRepository, CommerceOrderFuture,
    CommerceOrderReadRepository, CommerceOrderReceipt, ConvertedReadback, LINES_LIMIT, OrderAction,
    OrderCommandReceipt, OutboxEntry, ReceiptEntry, SalesOrderLineRow, SalesOrderRow,
    TransitionCommand, TransitionPlan, order_detail, transition_order, transition_order_guarded,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use serde_json::json;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "commerce-order-command-0001";
const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn legacy_goods_receipt_identity_matches_typescript_key_hash_and_result_order() {
    use erpx_server_commerce::public::receipt::{
        legacy_goods_receipt_number, legacy_goods_receipt_result,
    };
    for (key, number) in [
        ("receipt-one", "GR-1623126585A29ED7E9F75697"),
        ("receipt-two", "GR-2715B4050047A3952DC20E6D"),
        ("A.b:1-2_3", "GR-39B25FE63A08CB1E4559D957"),
    ] {
        let key = CommandKey::new(key).unwrap();
        assert_eq!(legacy_goods_receipt_number(&key), number);
    }
    let key = CommandKey::new("receipt-one").unwrap();
    let order = RecordId::new("order-1").unwrap();
    assert_eq!(
        legacy_goods_receipt_result(&order, &key, 3).unwrap(),
        r#"{"id":"order-1","fulfillmentId":"delivery-receipt-one","version":4}"#
    );
    assert!(legacy_goods_receipt_result(&order, &key, 0).is_err());
    assert_eq!(
        legacy_goods_receipt_result(&order, &key, 9_007_199_254_740_991).unwrap(),
        r#"{"id":"order-1","fulfillmentId":"delivery-receipt-one","version":9007199254740992}"#
    );
    assert!(legacy_goods_receipt_result(&order, &key, 9_007_199_254_740_992).is_err());
}

#[test]
fn legacy_goods_receipt_identity_matches_pinned_node_for_escaped_ids_and_boundaries() {
    use erpx_server_commerce::public::receipt::{
        legacy_goods_receipt_number, legacy_goods_receipt_result,
    };
    use std::process::Command;
    let node = std::env::var_os("ERPX_NODE_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.runtime/node/node-v26.11.1-darwin-arm64/bin/node")
        });
    let script = r#"if(process.version!=='v26.11.1')throw Error('NODE_VERSION');const c=require('node:crypto');const [id,key,version]=process.argv.slice(1);process.stdout.write('GR-'+c.createHash('sha256').update(key).digest('hex').slice(0,24).toUpperCase()+'\n'+JSON.stringify({id,fulfillmentId:`delivery-${key}`,version:Number(version)+1}));"#;
    for (id, key, version) in [
        ("quote\"slash\\id", "receipt-one", 1_i64),
        ("ชื่อ-東京", "A.b:1-2_3", 3),
        (
            "line\u{2028}separator",
            "receipt-two",
            9_007_199_254_740_991,
        ),
    ] {
        let output = Command::new(&node)
            .arg("-e")
            .arg(script)
            .arg(id)
            .arg(key)
            .arg(version.to_string())
            .output()
            .expect("pinned Node executable");
        assert!(output.status.success(), "Node oracle failed");
        let observed = String::from_utf8(output.stdout).unwrap();
        let (number, result) = observed.split_once('\n').unwrap();
        let key = CommandKey::new(key).unwrap();
        let id = RecordId::new(id).unwrap();
        assert_eq!(legacy_goods_receipt_number(&key), number);
        assert_eq!(
            legacy_goods_receipt_result(&id, &key, version).unwrap(),
            result
        );
    }
}

#[test]
fn legacy_fulfillment_normalization_matches_installed_zod_schema_and_pinned_node() {
    use erpx_server_commerce::public::receipt::normalize_legacy_fulfillment_input;
    use std::process::Command;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let node = std::env::var_os("ERPX_NODE_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join(".runtime/node/node-v26.11.1-darwin-arm64/bin/node"));
    let script = r#"if(process.version!=='v26.11.1')throw Error('NODE_VERSION');const fs=require('node:fs'),z=require('zod');if(require('zod/package.json').version!=='4.6.7')throw Error('ZOD_VERSION');const s=fs.readFileSync('packages/server-commerce/src/domain/commerce.ts','utf8');const q=s.slice(s.indexOf('const quantity = z'),s.indexOf('export const orderSchema'));const f=s.slice(s.indexOf('export const fulfillmentSchema'),s.indexOf('export const invoiceFromDeliverySchema')).replace('export const fulfillmentSchema','const fulfillmentSchema');const schema=new Function('z',q+f+';return fulfillmentSchema')(z);const result=schema.safeParse(JSON.parse(process.argv[1]));process.stdout.write(result.success?'ok:'+JSON.stringify(result.data):'invalid');"#;
    let emoji = "😀".repeat(100);
    let long_emoji = "😀".repeat(101);
    let mut cases = vec![
        serde_json::json!({"expectedVersion":3,"date":"0000-02-29","lines":[{"lineId":"line","quantity":"0001.200"}]}),
        serde_json::json!({"unknown":"ignored","lines":[{"quantity":"0.001","lineId":" L ","extra":true,"allocations":[{"quantity":"01.0","lotCode":format!("\u{feff} {emoji} \u{feff}"),"lotId":" lot ","serialNumber":"S","disposition":"quarantine","quarantineReason":" \u{feff} ok \u{feff} ","quarantineDueDate":"2026-11-05","ignored":1}]}],"evidenceNote":"  note  ","date":"9999-12-31","expectedVersion":1,"completionMode":"mixed"}),
        serde_json::json!({"expectedVersion":9_007_199_254_740_991_i64,"date":"2000-02-29","attachmentSetIds":["a","😀"],"evidenceIds":["e"],"lines":[{"lineId":"id","quantity":"123","allocations":[{"quantity":"0","expiryDate":""}]}]}),
        serde_json::json!({"expectedVersion":1,"date":"0000-02-30","lines":[{"lineId":"x","quantity":"1"}]}),
        serde_json::json!({"expectedVersion":0,"date":"2026-01-01","lines":[{"lineId":"x","quantity":"1"}]}),
        serde_json::json!({"expectedVersion":9_007_199_254_740_992_i64,"date":"2026-01-01","lines":[{"lineId":"x","quantity":"1"}]}),
        serde_json::json!({"expectedVersion":1,"date":"2026-01-01","lines":[{"lineId":"x","quantity":"1","allocations":[{"quantity":"1","lotCode":long_emoji}]}]}),
        serde_json::json!({"expectedVersion":1,"date":"2026-01-01","lines":[{"lineId":"x","quantity":"1","allocations":[{"quantity":"1","lotId":null}]}]}),
        serde_json::json!({"expectedVersion":1,"date":"2026-01-01","lines":[{"lineId":"x","quantity":"1.0000"}]}),
        serde_json::json!({"expectedVersion":1,"date":"2026-01-01","lines":[]}),
    ];
    cases.push(serde_json::json!({"expectedVersion":1,"date":"2026-01-01","lines":vec![serde_json::json!({"lineId":"x","quantity":"1"});21]}));
    cases.push(serde_json::json!({"expectedVersion":1,"date":"2026-01-01","lines":[{"lineId":"x","quantity":"1","allocations":vec![serde_json::json!({"quantity":"1"});200]}]}));
    cases.push(serde_json::json!({"expectedVersion":1,"date":"2026-01-01","lines":[{"lineId":"x","quantity":"1","allocations":vec![serde_json::json!({"quantity":"1"});201]}]}));
    let mut raw_cases = cases
        .into_iter()
        .map(|case| serde_json::to_string(&case).unwrap())
        .collect::<Vec<_>>();
    raw_cases.push(r#"{"expectedVersion":0,"expectedVersion":1e0,"date":"2026-01-01","lines":[{"lineId":"x","quantity":"1"}]}"#.into());
    raw_cases.push(r#"{"expectedVersion":1,"date":"2026-01-01","evidenceNote":null,"lines":[{"lineId":"x","quantity":"1"}]}"#.into());
    raw_cases.push(r#"{"\ud800":"ignored\udfff","expectedVersion":1,"date":"2026-01-01","evidenceNote":" \uFEFF\ud800\udfff\ud800 \uFEFF ","evidenceIds":["\udc00"],"lines":[{"lineId":"\ud800","quantity":"1","\udfff":"ignored","allocations":[{"quantity":"1","lotCode":" \uFEFF\udfff \uFEFF "}]}]}"#.into());
    raw_cases.push(format!(r#"{{"expectedVersion":1,"date":"2026-01-01","lines":[{{"lineId":"x","quantity":"1","allocations":[{{"quantity":"1","lotCode":"{}"}}]}}]}}"#,r"\ud800".repeat(100)));
    raw_cases.push(format!(r#"{{"expectedVersion":1,"date":"2026-01-01","lines":[{{"lineId":"x","quantity":"1","allocations":[{{"quantity":"1","lotCode":"{}"}}]}}]}}"#,r"\ud800".repeat(101)));
    for raw in raw_cases {
        let output = Command::new(&node)
            .current_dir(&root)
            .arg("-e")
            .arg(script)
            .arg(&raw)
            .output()
            .expect("pinned Node/Zod oracle");
        assert!(output.status.success(), "Node/Zod schema oracle failed");
        let expected = String::from_utf8(output.stdout).unwrap();
        let actual = normalize_legacy_fulfillment_input(&raw)
            .map(|value| format!("ok:{value}"))
            .unwrap_or_else(|_| "invalid".into());
        assert_eq!(actual, expected, "input {raw}");
    }
}

#[test]
fn legacy_fulfillment_workflow_input_fingerprints_match_pinned_node_formatting_only() {
    use erpx_server_commerce::public::receipt::legacy_fulfillment_workflow_input;
    use erpx_server_workflow::public::commands::{
        CommandActor, CommandField, WorkAuthority, command_fingerprint,
    };
    use std::process::Command;
    // Synthetic formatting data, not selected authority or replay admission.
    let actor = CommandActor {
        user_id: UserId::new("user").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
        role: "manager".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: CommandField::Null,
        department_id: CommandField::Value("ฝ่าย".into()),
        authority: CommandField::Value(WorkAuthority {
            session_id: "session".into(),
            revision: 7,
            assignment_version: 3,
        }),
        iam: CommandField::Omitted,
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let node = std::env::var_os("ERPX_NODE_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join(".runtime/node/node-v26.11.1-darwin-arm64/bin/node"));
    let script = r#"if(process.version!=='v26.11.1')throw Error('NODE_VERSION');const fs=require('node:fs'),z=require('zod'),c=require('node:crypto');if(require('zod/package.json').version!=='4.6.7')throw Error('ZOD_VERSION');const s=fs.readFileSync('packages/server-commerce/src/domain/commerce.ts','utf8');const q=s.slice(s.indexOf('const quantity = z'),s.indexOf('export const orderSchema'));const f=s.slice(s.indexOf('export const fulfillmentSchema'),s.indexOf('export const invoiceFromDeliverySchema')).replace('export const fulfillmentSchema','const fulfillmentSchema');const schema=new Function('z',q+f+';return fulfillmentSchema')(z);const input=schema.parse(JSON.parse(process.argv[1]));const p={actor:'user',assignment:'assignment',role:'manager',authority:{sessionId:'session',revision:7,assignmentVersion:3},company:'company',branch:null,department:'ฝ่าย',operation:'commerce.order.fulfill',recordId:'ใบสั่ง',input};process.stdout.write(c.createHash('sha256').update(JSON.stringify(p)).digest('hex'));"#;
    let cases = [
        r#"{"expectedVersion":3,"date":"0000-02-29","lines":[{"lineId":"line","quantity":"0001.200"}]}"#,
        r#"{"completionMode":"mixed","expectedVersion":1,"date":"2026-11-04","evidenceNote":" \uFEFF note  ","lines":[{"lineId":"ไทย","quantity":"1.00","allocations":[{"lotId":"lot-1","quantity":"0.500","lotCode":" L-A ","quarantineReason":" review ","disposition":"quarantine"}]}]}"#,
        r#"{"\udfff":"discard","expectedVersion":1,"date":"2026-11-04","evidenceNote":"\ud800","lines":[{"lineId":"\udfff","quantity":"1"}]}"#,
        r#"{"\udfff":"discard","expectedVersion":1,"date":"2026-11-04","evidenceNote":"\ud801","lines":[{"lineId":"\udfff","quantity":"1"}]}"#,
    ];
    for raw in cases {
        let output = Command::new(&node)
            .current_dir(&root)
            .arg("-e")
            .arg(script)
            .arg(raw)
            .output()
            .expect("pinned Node/Zod fingerprint oracle");
        assert!(output.status.success(), "Node fingerprint oracle failed");
        let expected = String::from_utf8(output.stdout).unwrap();
        let input = legacy_fulfillment_workflow_input(raw).unwrap();
        assert_eq!(
            command_fingerprint(&actor, "commerce.order.fulfill", "ใบสั่ง", &input).unwrap(),
            expected
        );
    }
}

fn actor() -> CommerceOrderActor {
    CommerceOrderActor {
        user_id: UserId::new("user").unwrap(),
        person_user_ids: vec![],
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: vec![
            "commerce.read".into(),
            "commerce.sales".into(),
            "commerce.approve".into(),
        ],
        modules: vec!["sales".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn order(kind: &str, state: &str, version: i64, created_by: &str) -> SalesOrderRow {
    SalesOrderRow {
        id: "order-1".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        kind: kind.into(),
        number: if kind == "quote" { "QT-1" } else { "SO-1" }.into(),
        party_id: "party-1".into(),
        warehouse_id: Some("warehouse-1".into()),
        currency: "THB".into(),
        order_date: "2026-11-01".into(),
        due_date: "2026-11-08".into(),
        note: String::new(),
        state: state.into(),
        subtotal_minor: 10_000,
        tax_minor: 700,
        source_id: None,
        version,
        created_by: created_by.into(),
        approved_by: None,
        created_at: 1,
        updated_at: 1,
        price_list_id: None,
        price_list_version: None,
        discount_reason: None,
        discount_approval_required: 0,
        discount_approved_by: None,
        discount_approved_at: None,
    }
}

fn line() -> SalesOrderLineRow {
    SalesOrderLineRow {
        id: "line-1".into(),
        order_id: "order-1".into(),
        product_id: "product-1".into(),
        description: "Service".into(),
        quantity_milli: 1_000,
        fulfilled_milli: 0,
        price_minor: 10_000,
        tax_bps: 700,
        subtotal_minor: 10_000,
        tax_minor: 700,
        ordinal: 0,
        list_price_minor: None,
        discount_bps: 0,
        item_type: "service".into(),
        sku: "SVC-1".into(),
        unit: "each".into(),
        tracking_mode: "none".into(),
    }
}

fn submit() -> TransitionCommand {
    TransitionCommand {
        key: CommandKey::new(KEY).unwrap(),
        order_id: RecordId::new("order-1").unwrap(),
        action: OrderAction::Submit,
        expected_version: 1,
        reason: None,
    }
}

fn expected_receipt(state: &str, version: i64, sales_id: Option<&str>) -> CommerceOrderReceipt {
    CommerceOrderReceipt {
        id: "order-1".into(),
        state: state.into(),
        version,
        sales_id: sales_id.map(str::to_owned),
    }
}

#[derive(Default)]
struct ReadProbe {
    authority: bool,
    order: Option<SalesOrderRow>,
    lines: Vec<SalesOrderLineRow>,
    calls: Vec<&'static str>,
}

impl CommerceOrderReadRepository for ReadProbe {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: bool,
        _: bool,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("authority");
        let authority = self.authority;
        Box::pin(async move { Ok(authority) })
    }

    fn order<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        self.calls.push("order");
        let order = self.order.clone();
        Box::pin(async move { Ok(order) })
    }

    fn lines<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Vec<SalesOrderLineRow>, Self::Error> {
        self.calls.push("lines");
        let lines = self.lines.clone();
        Box::pin(async move { Ok(lines) })
    }
}

impl ReadProbe {
    fn ready(order: SalesOrderRow) -> Self {
        Self {
            authority: true,
            order: Some(order),
            lines: vec![line()],
            calls: Vec::new(),
        }
    }
}

#[derive(Default)]
struct CommandProbe {
    authority: bool,
    approve_authority: bool,
    hash: String,
    receipt: Option<OrderCommandReceipt>,
    locked: Option<SalesOrderRow>,
    fulfillments: i64,
    transition_ok: bool,
    insert_converted: bool,
    copied: u64,
    converted: Option<ConvertedReadback>,
    readback: Option<SalesOrderRow>,
    receipt_insert: bool,
    calls: Vec<&'static str>,
    plans: Vec<TransitionPlan>,
    audited: Vec<(String, String, String, i64)>,
    outboxed: Vec<(String, String, String, i64, String, Option<String>)>,
    credit: bool,
    credit_calls: Vec<(String, i64)>,
    release_calls: Vec<String>,
    allow_conversion_preflight: bool,
    save_list: Option<erpx_server_commerce::public::order::SaveOrderPriceList>,
    missing_save_product: bool,
}

impl CommandProbe {
    fn ready(locked: SalesOrderRow, readback: SalesOrderRow) -> Self {
        Self {
            authority: true,
            approve_authority: true,
            hash: HASH.into(),
            locked: Some(locked),
            transition_ok: true,
            insert_converted: true,
            copied: 1,
            readback: Some(readback),
            receipt_insert: true,
            ..Self::default()
        }
    }
}

impl CommerceOrderCommandRepository for CommandProbe {
    type Error = BusinessError;

    fn prepare_converted_order_money<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a TransitionCommand,
        _: &'a SalesOrderRow,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("conversion-money-preflight");
        let allow = self.allow_conversion_preflight;
        Box::pin(async move {
            if allow {
                Ok(())
            } else {
                Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "COMMERCE_CONVERSION_MONEY_REQUIRED",
                    "The Finance conversion Money source is unavailable.",
                ))
            }
        })
    }

    fn current_authority<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        write: bool,
        approve: bool,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push(if !write {
            "authority-read"
        } else if approve {
            "authority-approve"
        } else {
            "authority-write"
        });
        let value = if approve {
            self.approve_authority
        } else {
            self.authority
        };
        Box::pin(async move { Ok(value) })
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("lock-command");
        assert_eq!(key, KEY);
        Box::pin(async move { Ok(()) })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> CommerceOrderFuture<'a, String, Self::Error> {
        self.calls.push("digest");
        assert!(value.contains("\"operation\""), "{value}");
        assert!(value.contains("\"expectedVersion\""), "{value}");
        let hash = self.hash.clone();
        Box::pin(async move { Ok(hash) })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CommerceOrderFuture<'a, Option<OrderCommandReceipt>, Self::Error> {
        self.calls.push("receipt");
        assert_eq!(key, KEY);
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn lock_order<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        self.calls.push("lock");
        let order = self.locked.clone();
        Box::pin(async move { Ok(order) })
    }

    fn count_fulfillments<'a>(
        &'a mut self,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, i64, Self::Error> {
        self.calls.push("fulfillments");
        let count = self.fulfillments;
        Box::pin(async move { Ok(count) })
    }

    fn transition<'a>(
        &'a mut self,
        plan: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("transition");
        self.plans.push(plan.clone());
        let changed = self.transition_ok;
        Box::pin(async move { Ok(changed) })
    }

    fn insert_converted_order<'a>(
        &'a mut self,
        _: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("insert-converted");
        let changed = self.insert_converted;
        Box::pin(async move { Ok(changed) })
    }

    fn copy_converted_lines<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, u64, Self::Error> {
        self.calls.push("copy-lines");
        let copied = self.copied;
        Box::pin(async move { Ok(copied) })
    }

    fn converted_readback<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a CompanyId,
    ) -> CommerceOrderFuture<'a, Option<ConvertedReadback>, Self::Error> {
        self.calls.push("converted-readback");
        let converted = self.converted.clone();
        Box::pin(async move { Ok(converted) })
    }

    fn readback_order<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        self.calls.push("readback");
        let readback = self.readback.clone();
        Box::pin(async move { Ok(readback) })
    }

    fn sales_order_credit_allows<'a>(
        &'a mut self,
        _: &'a CompanyId,
        party_id: &'a str,
        total: i64,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("credit");
        self.credit_calls.push((party_id.to_owned(), total));
        let allowed = self.credit;
        Box::pin(async move { Ok(allowed) })
    }

    fn release_confirmed_sales_reservations<'a>(
        &'a mut self,
        _: &'a CompanyId,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("release-reservations");
        self.release_calls.push(order_id.to_owned());
        Box::pin(async move { Ok(()) })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        _: &'a ReceiptEntry<'a>,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("receipt-insert");
        let changed = self.receipt_insert;
        Box::pin(async move { Ok(changed) })
    }

    fn audit<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        entry: &'a AuditEntry<'a>,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("audit");
        self.audited.push((
            entry.operation.into(),
            entry.record_type.into(),
            entry.record_id.into(),
            entry.version,
        ));
        Box::pin(async move { Ok(()) })
    }

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("outbox");
        self.outboxed.push((
            entry.operation.into(),
            entry.record_type.into(),
            entry.record_id.into(),
            entry.version,
            entry.state.into(),
            entry.sales_id.map(str::to_owned),
        ));
        Box::pin(async move { Ok(()) })
    }
}

fn read(probe: &mut ReadProbe, actor: &CommerceOrderActor) -> Result<(), BusinessError> {
    sqlx::test_block_on(order_detail(actor, "order-1", probe, NOW)).map(|_| ())
}

fn run(
    probe: &mut CommandProbe,
    actor: &CommerceOrderActor,
    command: &TransitionCommand,
) -> Result<CommerceOrderReceipt, BusinessError> {
    sqlx::test_block_on(transition_order(actor, command, probe, NOW))
}

fn run_guarded(
    probe: &mut CommandProbe,
    actor: &CommerceOrderActor,
    command: &TransitionCommand,
) -> Result<CommerceOrderReceipt, BusinessError> {
    sqlx::test_block_on(transition_order_guarded(actor, command, probe, NOW))
}

#[test]
fn read_policy_precedes_every_source_read() {
    let mut no_permission = actor();
    no_permission.permissions.clear();
    let mut probe = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let error = read(&mut probe, &no_permission).unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "COMMERCE_PERMISSION_REQUIRED");
    assert!(probe.calls.is_empty());

    let mut no_module = actor();
    no_module.modules.clear();
    let mut probe = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let error = read(&mut probe, &no_module).unwrap_err();
    assert_eq!(error.code, "COMMERCE_MODULE_FORBIDDEN");
    assert!(probe.calls.is_empty());

    let mut expired = actor();
    expired.expires_at = NOW.get();
    let mut probe = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let error = read(&mut probe, &expired).unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert!(probe.calls.is_empty());

    let mut revoked = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    revoked.authority = false;
    let error = read(&mut revoked, &actor()).unwrap_err();
    assert_eq!(error.code, "COMMERCE_PERMISSION_REQUIRED");
    assert_eq!(revoked.calls, ["authority"]);
}

#[test]
fn read_rejects_foreign_scope_and_invalid_documents_but_admits_purchase() {
    let mut foreign = ReadProbe::ready(SalesOrderRow {
        company_id: "foreign".into(),
        ..order("quote", "draft", 1, "creator")
    });
    assert_eq!(
        read(&mut foreign, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut branch = ReadProbe::ready(SalesOrderRow {
        branch_id: Some("other-branch".into()),
        ..order("quote", "draft", 1, "creator")
    });
    assert_eq!(
        read(&mut branch, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut purchase = ReadProbe::ready(SalesOrderRow {
        kind: "purchase".into(),
        number: "PO-1".into(),
        ..order("quote", "draft", 1, "creator")
    });
    let document =
        sqlx::test_block_on(order_detail(&actor(), "order-1", &mut purchase, NOW)).unwrap();
    assert_eq!(document.order.kind, "purchase");
    assert_eq!(document.order.number, "PO-1");

    let mut totals = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    totals.lines[0].subtotal_minor = 9_999;
    assert_eq!(
        read(&mut totals, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut unordered = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let mut second = line();
    second.id = "line-2".into();
    second.product_id = "product-2".into();
    second.ordinal = 0;
    unordered.lines.push(second);
    assert_eq!(
        read(&mut unordered, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut missing_product = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    missing_product.lines[0].sku.clear();
    assert_eq!(
        read(&mut missing_product, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut oversized = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    oversized.lines = (0..=LINES_LIMIT)
        .map(|index| SalesOrderLineRow {
            id: format!("line-{index}"),
            product_id: format!("product-{index}"),
            ordinal: index as i64,
            subtotal_minor: 0,
            tax_minor: 0,
            ..line()
        })
        .collect();
    oversized.order.as_mut().unwrap().subtotal_minor = 0;
    oversized.order.as_mut().unwrap().tax_minor = 0;
    assert_eq!(
        read(&mut oversized, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_TOO_LARGE"
    );
}

#[test]
fn read_returns_only_the_exact_owner_columns() {
    let mut probe = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let document = sqlx::test_block_on(order_detail(&actor(), "order-1", &mut probe, NOW)).unwrap();
    assert_eq!(probe.calls, ["authority", "order", "lines"]);
    let value = serde_json::to_value(document.to_wire()).unwrap();
    assert_eq!(value["order"]["id"], "order-1");
    assert_eq!(value["order"]["party_id"], "party-1");
    assert_eq!(
        value["order"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [
            "approved_by",
            "branch_id",
            "company_id",
            "created_at",
            "created_by",
            "currency",
            "department_id",
            "discount_approval_required",
            "discount_approved_at",
            "discount_approved_by",
            "discount_reason",
            "due_date",
            "id",
            "kind",
            "note",
            "number",
            "order_date",
            "party_id",
            "price_list_id",
            "price_list_version",
            "source_id",
            "state",
            "subtotal_minor",
            "tax_minor",
            "updated_at",
            "version",
            "warehouse_id",
        ]
    );
    assert_eq!(
        value["lines"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [
            "description",
            "discount_bps",
            "fulfilled_milli",
            "id",
            "item_type",
            "list_price_minor",
            "order_id",
            "ordinal",
            "price_minor",
            "product_id",
            "quantity_milli",
            "sku",
            "subtotal_minor",
            "tax_bps",
            "tax_minor",
            "tracking_mode",
            "unit",
        ]
    );
}

#[test]
fn submit_writes_one_exact_transition_receipt_audit_and_outbox() {
    let mut probe = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    let receipt = run(&mut probe, &actor(), &submit()).unwrap();
    assert_eq!(receipt, expected_receipt("submitted", 2, None));
    assert_eq!(
        probe.calls,
        [
            "lock-command",
            "digest",
            "receipt",
            "authority-write",
            "lock",
            "transition",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
    let plan = &probe.plans[0];
    assert_eq!(plan.expected_state, "draft");
    assert_eq!(plan.next_state, "submitted");
    assert_eq!(plan.approved_by, None);
    assert_eq!(
        probe.audited,
        [(
            "commerce.order.submit".into(),
            "commerce_order".into(),
            "order-1".into(),
            2
        )]
    );
    assert_eq!(
        probe.outboxed,
        [(
            "commerce.order.submit".into(),
            "commerce_order".into(),
            "order-1".into(),
            2,
            "submitted".into(),
            None
        )]
    );
}

#[test]
fn replay_returns_the_original_receipt_without_a_fresh_write() {
    let mut probe = CommandProbe {
        receipt: Some(OrderCommandReceipt {
            request_hash: HASH.into(),
            result_json: json!({
                "id":"order-1","state":"submitted","version":2,"salesId":null
            })
            .to_string(),
            operation: Some("commerce.order.submit".into()),
            actor_id: Some("user".into()),
            record_id: Some("order-1".into()),
            authority_json: None,
        }),
        ..CommandProbe::ready(
            order("quote", "draft", 1, "creator"),
            order("quote", "submitted", 2, "creator"),
        )
    };
    let receipt = run(&mut probe, &actor(), &submit()).unwrap();
    assert_eq!(receipt, expected_receipt("submitted", 2, None));
    assert_eq!(probe.calls, ["lock-command", "digest", "receipt"]);

    let mut conflict = CommandProbe {
        receipt: Some(OrderCommandReceipt {
            request_hash: "b".repeat(64),
            result_json: json!({
                "id":"order-1","state":"submitted","version":2,"salesId":null
            })
            .to_string(),
            operation: Some("commerce.order.submit".into()),
            actor_id: Some("user".into()),
            record_id: Some("order-1".into()),
            authority_json: None,
        }),
        ..CommandProbe::ready(
            order("quote", "draft", 1, "creator"),
            order("quote", "submitted", 2, "creator"),
        )
    };
    assert_eq!(
        run(&mut conflict, &actor(), &submit()).unwrap_err().code,
        "IDEMPOTENCY_CONFLICT"
    );
}

#[test]
fn command_denials_precede_effects() {
    let mut no_authority = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    no_authority.authority = false;
    assert_eq!(
        run(&mut no_authority, &actor(), &submit())
            .unwrap_err()
            .code,
        "COMMERCE_PERMISSION_REQUIRED"
    );
    assert_eq!(
        no_authority.calls,
        ["lock-command", "digest", "receipt", "authority-write"]
    );

    for command in [
        TransitionCommand {
            expected_version: 0,
            ..submit()
        },
        TransitionCommand {
            reason: Some("x".repeat(1_001)),
            ..submit()
        },
    ] {
        let mut probe = CommandProbe::ready(
            order("quote", "draft", 1, "creator"),
            order("quote", "submitted", 2, "creator"),
        );
        assert_eq!(
            run(&mut probe, &actor(), &command).unwrap_err().code,
            "COMMERCE_ORDER_REQUEST_INVALID"
        );
        assert!(probe.calls.is_empty());
    }

    let mut not_found = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    not_found.locked = None;
    assert_eq!(
        run(&mut not_found, &actor(), &submit()).unwrap_err().code,
        "ORDER_NOT_FOUND"
    );
}

#[test]
fn quotation_approval_requires_independence_discount_reason_and_approver_permission() {
    let mut discount = order("quote", "submitted", 2, "creator");
    discount.discount_approval_required = 1;
    let mut create_self =
        CommandProbe::ready(discount.clone(), order("quote", "confirmed", 3, "creator"));
    let command = TransitionCommand {
        action: OrderAction::Approve,
        expected_version: 2,
        ..submit()
    };
    let mut self_actor = actor();
    self_actor.user_id = UserId::new("creator").unwrap();
    assert_eq!(
        run(&mut create_self, &self_actor, &command)
            .unwrap_err()
            .code,
        "INDEPENDENT_APPROVER_REQUIRED"
    );

    let mut missing_reason =
        CommandProbe::ready(discount.clone(), order("quote", "confirmed", 3, "creator"));
    assert_eq!(
        run(&mut missing_reason, &actor(), &command)
            .unwrap_err()
            .code,
        "DISCOUNT_APPROVAL_REASON_REQUIRED"
    );

    let mut no_approve =
        CommandProbe::ready(discount.clone(), order("quote", "confirmed", 3, "creator"));
    no_approve.approve_authority = false;
    assert_eq!(
        run(&mut no_approve, &actor(), &command).unwrap_err().code,
        "COMMERCE_APPROVAL_REQUIRED"
    );

    let mut approved = CommandProbe::ready(
        discount,
        SalesOrderRow {
            state: "confirmed".into(),
            version: 3,
            approved_by: Some("user".into()),
            discount_approved_by: Some("user".into()),
            discount_approved_at: Some(NOW.get()),
            ..order("quote", "confirmed", 3, "creator")
        },
    );
    let receipt = run(
        &mut approved,
        &actor(),
        &TransitionCommand {
            reason: Some("Independent discount review".into()),
            ..command
        },
    )
    .unwrap();
    assert_eq!(receipt, expected_receipt("confirmed", 3, None));
    assert_eq!(approved.plans[0].approved_by.as_deref(), Some("user"));
    assert_eq!(
        approved.plans[0].discount_approved_by.as_deref(),
        Some("user")
    );
}

#[test]
fn sales_approval_and_confirmed_sales_cancellation_fail_closed() {
    let mut approve = CommandProbe::ready(
        order("sales", "submitted", 2, "creator"),
        order("sales", "confirmed", 3, "creator"),
    );
    assert_eq!(
        run(
            &mut approve,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Approve,
                expected_version: 2,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "COMMERCE_SALES_APPROVAL_FINANCE_GUARD"
    );

    let mut cancel = CommandProbe::ready(
        order("sales", "confirmed", 3, "creator"),
        order("sales", "cancelled", 4, "creator"),
    );
    assert_eq!(
        run(
            &mut cancel,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Cancel,
                expected_version: 3,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "ORDER_REVERSAL_REQUIRED"
    );
}

#[test]
fn native_sales_approval_uses_finance_credit_and_cancel_releases_reservations() {
    let mut denied = CommandProbe::ready(
        order("sales", "submitted", 2, "creator"),
        order("sales", "confirmed", 3, "creator"),
    );
    assert_eq!(
        run_guarded(
            &mut denied,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Approve,
                expected_version: 2,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "CONFLICT"
    );
    assert_eq!(denied.credit_calls, [("party-1".into(), 10_700)]);
    assert!(!denied.calls.contains(&"transition"));

    let mut approved = CommandProbe::ready(
        order("sales", "submitted", 2, "creator"),
        SalesOrderRow {
            approved_by: Some("user".into()),
            ..order("sales", "confirmed", 3, "creator")
        },
    );
    approved.credit = true;
    assert_eq!(
        run_guarded(
            &mut approved,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Approve,
                expected_version: 2,
                ..submit()
            }
        )
        .unwrap(),
        expected_receipt("confirmed", 3, None)
    );
    assert_eq!(approved.credit_calls, [("party-1".into(), 10_700)]);
    assert_eq!(approved.plans[0].approved_by.as_deref(), Some("user"));

    let mut fulfilled = CommandProbe::ready(
        order("sales", "confirmed", 3, "creator"),
        order("sales", "cancelled", 4, "creator"),
    );
    fulfilled.fulfillments = 1;
    assert_eq!(
        run_guarded(
            &mut fulfilled,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Cancel,
                expected_version: 3,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "ORDER_REVERSAL_REQUIRED"
    );
    assert!(fulfilled.release_calls.is_empty());

    let mut cancelled = CommandProbe::ready(
        order("sales", "confirmed", 3, "creator"),
        order("sales", "cancelled", 4, "creator"),
    );
    assert_eq!(
        run_guarded(
            &mut cancelled,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Cancel,
                expected_version: 3,
                ..submit()
            }
        )
        .unwrap(),
        expected_receipt("cancelled", 4, None)
    );
    assert_eq!(cancelled.release_calls, ["order-1"]);
    assert!(
        cancelled
            .calls
            .iter()
            .position(|call| *call == "release-reservations")
            < cancelled
                .calls
                .iter()
                .position(|call| *call == "transition")
    );
}

#[test]
fn sales_credit_guard_precedes_stale_cas_after_authority_and_discount_checks() {
    let command = TransitionCommand {
        action: OrderAction::Approve,
        expected_version: 2,
        ..submit()
    };
    let row = order("sales", "submitted", 3, "creator");
    let mut denied = CommandProbe::ready(row.clone(), order("sales", "confirmed", 4, "creator"));
    let error = run_guarded(&mut denied, &actor(), &command).unwrap_err();
    assert_eq!(error.code, "CONFLICT");
    assert_eq!(
        error.kind,
        erpx_server_shared::public::FailureKind::Conflict
    );
    assert_eq!(
        error.message,
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again."
    );
    assert_eq!(denied.credit_calls, [("party-1".into(), 10_700)]);
    assert!(!denied.calls.contains(&"transition"));
    let mut allowed = CommandProbe::ready(row.clone(), order("sales", "confirmed", 4, "creator"));
    allowed.credit = true;
    assert_eq!(
        run_guarded(&mut allowed, &actor(), &command)
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    assert_eq!(allowed.credit_calls, [("party-1".into(), 10_700)]);
    assert!(!allowed.calls.contains(&"transition"));
    let mut discount = row.clone();
    discount.discount_approval_required = 1;
    let mut invalid = CommandProbe::ready(discount, order("sales", "confirmed", 4, "creator"));
    assert_eq!(
        run_guarded(&mut invalid, &actor(), &command)
            .unwrap_err()
            .code,
        "DISCOUNT_APPROVAL_REASON_REQUIRED"
    );
    assert!(invalid.credit_calls.is_empty());
    let mut denied_authority = CommandProbe::ready(row, order("sales", "confirmed", 4, "creator"));
    denied_authority.approve_authority = false;
    assert_eq!(
        run_guarded(&mut denied_authority, &actor(), &command)
            .unwrap_err()
            .code,
        "COMMERCE_APPROVAL_REQUIRED"
    );
    assert!(denied_authority.credit_calls.is_empty());
}

#[test]
fn retained_identity_aliases_enforce_independent_approval_and_cancellation() {
    let mut selected = actor();
    selected.person_user_ids = vec![UserId::new("creator-alias").unwrap()];
    for (action, state, code) in [
        (
            OrderAction::Approve,
            "submitted",
            "INDEPENDENT_APPROVER_REQUIRED",
        ),
        (
            OrderAction::Cancel,
            "confirmed",
            "INDEPENDENT_CANCELLER_REQUIRED",
        ),
    ] {
        let command = TransitionCommand {
            action,
            expected_version: 3,
            ..submit()
        };
        for creator in ["user", "creator-alias"] {
            let mut repository = CommandProbe::ready(
                order("quote", state, 3, creator),
                order("quote", "cancelled", 4, creator),
            );
            assert_eq!(
                run_guarded(&mut repository, &selected, &command)
                    .unwrap_err()
                    .code,
                code,
                "{state}/{creator}"
            );
            assert!(repository.release_calls.is_empty());
            assert!(!repository.calls.contains(&"transition"));
        }
        let next = if action == OrderAction::Approve {
            "confirmed"
        } else {
            "cancelled"
        };
        let mut repository = CommandProbe::ready(
            order("quote", state, 3, "other-person"),
            order("quote", next, 4, "other-person"),
        );
        if action == OrderAction::Approve {
            repository.readback.as_mut().unwrap().approved_by = Some("user".into());
        }
        assert_eq!(
            run_guarded(&mut repository, &selected, &command).unwrap(),
            expected_receipt(next, 4, None)
        );
    }
}

#[test]
fn transition_preconditions_match_actual_ts_before_stale_version_guard() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let module = root.join("packages/server-commerce/src/application/commerce.ts");
    let script = r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const cases=[['approve','submitted','alias',true,false,false],['approve','submitted','other',false,false,false],['approve','submitted','other',true,true,false],['approve','submitted','other',true,false,false],['cancel','confirmed','alias',true,false,false],['cancel','confirmed','other',false,false,false],['cancel','partially_fulfilled','other',true,false,true]];const result=[];for(const [action,state,creator,approve,discount,fulfillment] of cases){const actor={userId:'user',companyId:'company',branchId:null,departmentId:null,iam:{permissions:['commerce.sales',...(approve?['commerce.approve']:[])],personUserIds:['alias']}};const row={kind:'quote',state,created_by:creator,discount_approval_required:discount?1:0};const err=(status,code,message)=>Object.assign(new Error(message),{status,code});const writer={replayCommand:async()=>null,readCommerceOrders1:async()=>row,readCommerceFulfillments4:async()=>fulfillment?{id:'f'}:null,guardRule1:()=>({}),prepareInventoryOrderReservations3:()=>({}),prepareCommerceOrders4:()=>({}),executeCommand:async()=>{throw err(409,'CONFLICT','stale')}};const app=m.createCommerceApplication({clock:{now:()=>1},require:a=>a,error:err,hash:async()=>'',forActor:async()=>writer});try{await app.transitionOrder(actor,'order-1',{action,expectedVersion:2},'command-key');result.push('allowed')}catch(e){result.push(e.code)}}process.stdout.write(JSON.stringify(result));"#;
    let oracle =
        std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(root)
            .args(["--conditions=react-server", "-e", script])
            .arg(module)
            .output()
            .unwrap();
    assert!(
        oracle.status.success(),
        "actual Commerce TS application oracle: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let expected: Vec<String> = serde_json::from_slice(&oracle.stdout).unwrap();
    let cases = [
        (
            OrderAction::Approve,
            "submitted",
            "alias",
            true,
            false,
            false,
        ),
        (
            OrderAction::Approve,
            "submitted",
            "other",
            false,
            false,
            false,
        ),
        (
            OrderAction::Approve,
            "submitted",
            "other",
            true,
            true,
            false,
        ),
        (
            OrderAction::Approve,
            "submitted",
            "other",
            true,
            false,
            false,
        ),
        (
            OrderAction::Cancel,
            "confirmed",
            "alias",
            true,
            false,
            false,
        ),
        (
            OrderAction::Cancel,
            "confirmed",
            "other",
            false,
            false,
            false,
        ),
        (
            OrderAction::Cancel,
            "partially_fulfilled",
            "other",
            true,
            false,
            true,
        ),
    ];
    let mut actual = Vec::new();
    for (action, state, creator, approve, discount, fulfillment) in cases {
        let mut selected = actor();
        selected.person_user_ids = vec![UserId::new("alias").unwrap()];
        if !approve {
            selected.permissions.retain(|p| p != "commerce.approve");
        }
        let mut row = order("quote", state, 3, creator);
        row.discount_approval_required = i64::from(discount);
        let mut repository = CommandProbe::ready(row, order("quote", "cancelled", 4, creator));
        repository.fulfillments = i64::from(fulfillment);
        let command = TransitionCommand {
            action,
            expected_version: 2,
            ..submit()
        };
        actual.push(
            run_guarded(&mut repository, &selected, &command)
                .unwrap_err()
                .code
                .to_owned(),
        );
        assert!(!repository.calls.contains(&"transition"));
        assert!(repository.release_calls.is_empty());
    }
    assert_eq!(actual, expected);
}

#[test]
fn native_cancellation_releases_reservations_for_each_allowed_kind_and_state() {
    for (kind, state) in [
        ("quote", "draft"),
        ("sales", "draft"),
        ("quote", "confirmed"),
        ("sales", "confirmed"),
    ] {
        let mut repository = CommandProbe::ready(
            order(kind, state, 3, "creator"),
            order(kind, "cancelled", 4, "creator"),
        );
        let command = TransitionCommand {
            action: OrderAction::Cancel,
            expected_version: 3,
            ..submit()
        };
        assert_eq!(
            run_guarded(&mut repository, &actor(), &command).unwrap(),
            expected_receipt("cancelled", 4, None),
            "{kind}/{state}"
        );
        assert_eq!(repository.release_calls, ["order-1"], "{kind}/{state}");
        assert!(
            repository
                .calls
                .iter()
                .position(|call| *call == "release-reservations")
                < repository
                    .calls
                    .iter()
                    .position(|call| *call == "transition")
        );
    }
    // The native OrderKind parser currently admits quote/sales only, while
    // legacy TS also admits purchase. Preserve this gap explicitly rather
    // than claiming the purchase transition is migrated.
    for state in ["draft", "confirmed"] {
        let mut repository = CommandProbe::ready(
            order("purchase", state, 3, "creator"),
            order("purchase", "cancelled", 4, "creator"),
        );
        let command = TransitionCommand {
            action: OrderAction::Cancel,
            expected_version: 3,
            ..submit()
        };
        assert_eq!(
            run_guarded(&mut repository, &actor(), &command)
                .unwrap_err()
                .code,
            "COMMERCE_ORDER_SOURCE_INVALID"
        );
        assert!(repository.release_calls.is_empty());
        assert!(!repository.calls.contains(&"transition"));
    }
}

#[test]
fn confirmed_quotation_cancel_is_independent_and_requires_no_fulfillment() {
    let mut fulfilled = CommandProbe::ready(
        order("quote", "confirmed", 3, "creator"),
        order("quote", "cancelled", 4, "creator"),
    );
    fulfilled.fulfillments = 1;
    assert_eq!(
        run(
            &mut fulfilled,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Cancel,
                expected_version: 3,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "ORDER_REVERSAL_REQUIRED"
    );

    let mut self_cancel = CommandProbe::ready(
        order("quote", "confirmed", 3, "creator"),
        order("quote", "cancelled", 4, "creator"),
    );
    let mut self_actor = actor();
    self_actor.user_id = UserId::new("creator").unwrap();
    assert_eq!(
        run(
            &mut self_cancel,
            &self_actor,
            &TransitionCommand {
                action: OrderAction::Cancel,
                expected_version: 3,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "INDEPENDENT_CANCELLER_REQUIRED"
    );

    let mut cancelled = CommandProbe::ready(
        order("quote", "confirmed", 3, "creator"),
        order("quote", "cancelled", 4, "creator"),
    );
    let receipt = run(
        &mut cancelled,
        &actor(),
        &TransitionCommand {
            action: OrderAction::Cancel,
            expected_version: 3,
            ..submit()
        },
    )
    .unwrap();
    assert_eq!(receipt, expected_receipt("cancelled", 4, None));
    assert_eq!(
        cancelled.calls,
        [
            "lock-command",
            "digest",
            "receipt",
            "authority-write",
            "lock",
            "authority-approve",
            "fulfillments",
            "transition",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
}

#[test]
fn conversion_copies_the_quote_into_a_draft_sales_order() {
    let quote = order("quote", "confirmed", 3, "creator");
    let converted_order = SalesOrderRow {
        id: format!("order-{KEY}"),
        kind: "sales".into(),
        number: format!("SO-{}", HASH[..24].to_ascii_uppercase()),
        state: "draft".into(),
        version: 1,
        source_id: Some("order-1".into()),
        approved_by: None,
        created_by: "user".into(),
        created_at: NOW.get(),
        updated_at: NOW.get(),
        ..quote.clone()
    };
    let mut probe = CommandProbe::ready(
        quote.clone(),
        SalesOrderRow {
            state: "converted".into(),
            version: 4,
            ..quote.clone()
        },
    );
    probe.converted = Some(ConvertedReadback {
        order: converted_order,
        line_count: 1,
        line_subtotal_minor: quote.subtotal_minor,
        line_tax_minor: quote.tax_minor,
    });
    let receipt = run(
        &mut probe,
        &actor(),
        &TransitionCommand {
            action: OrderAction::Convert,
            expected_version: 3,
            ..submit()
        },
    )
    .unwrap();
    assert_eq!(
        receipt,
        CommerceOrderReceipt {
            id: "order-1".into(),
            state: "converted".into(),
            version: 4,
            sales_id: Some(format!("order-{KEY}")),
        }
    );
    assert_eq!(
        probe.calls,
        [
            "lock-command",
            "digest",
            "receipt",
            "authority-write",
            "lock",
            "transition",
            "insert-converted",
            "copy-lines",
            "converted-readback",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
    assert_eq!(
        probe.outboxed[0].5.as_deref(),
        Some(format!("order-{KEY}").as_str())
    );
}

#[test]
fn conversion_requires_quote_before_stale_transition_guard() {
    let command = TransitionCommand {
        action: OrderAction::Convert,
        expected_version: 1,
        ..submit()
    };
    for (kind, code) in [("sales", "QUOTE_REQUIRED"), ("quote", "CONFLICT")] {
        let mut repository = CommandProbe::ready(
            order(kind, "submitted", 2, "creator"),
            order(kind, "converted", 3, "creator"),
        );
        repository.allow_conversion_preflight = true;
        let error = run_guarded(&mut repository, &actor(), &command).unwrap_err();
        assert_eq!(error.code, code, "{kind}");
        assert_eq!(
            repository.calls.contains(&"conversion-money-preflight"),
            kind == "quote"
        );
        assert!(!repository.calls.contains(&"transition"));
        assert!(!repository.calls.contains(&"insert-converted"));
    }
}

#[test]
fn stale_cas_and_bad_readback_fail_without_a_receipt() {
    let mut stale = CommandProbe::ready(
        order("quote", "draft", 2, "creator"),
        order("quote", "submitted", 3, "creator"),
    );
    assert_eq!(
        run(&mut stale, &actor(), &submit()).unwrap_err().code,
        "CONFLICT"
    );

    let mut changed = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    changed.transition_ok = false;
    assert_eq!(
        run(&mut changed, &actor(), &submit()).unwrap_err().code,
        "CONFLICT"
    );

    let mut bad = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    bad.readback.as_mut().unwrap().version = 3;
    assert_eq!(
        run(&mut bad, &actor(), &submit()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut duplicate_receipt = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    duplicate_receipt.receipt_insert = false;
    assert_eq!(
        run(&mut duplicate_receipt, &actor(), &submit())
            .unwrap_err()
            .code,
        "IDEMPOTENCY_CONFLICT"
    );
}

#[test]
fn discount_reason_and_long_reason_use_utf16_js_trim_parity() {
    let mut discount = order("quote", "submitted", 2, "creator");
    discount.discount_approval_required = 1;
    let command = TransitionCommand {
        action: OrderAction::Approve,
        expected_version: 2,
        ..submit()
    };
    let readback = SalesOrderRow {
        state: "confirmed".into(),
        version: 3,
        approved_by: Some("user".into()),
        discount_approved_by: Some("user".into()),
        discount_approved_at: Some(NOW.get()),
        ..order("quote", "confirmed", 3, "creator")
    };
    // Three astral symbols are six UTF-16 units (chars count would be 3).
    let mut emoji = CommandProbe::ready(discount.clone(), readback.clone());
    let receipt = run(
        &mut emoji,
        &actor(),
        &TransitionCommand {
            reason: Some("💯💯💯".into()),
            ..command.clone()
        },
    )
    .unwrap();
    assert_eq!(receipt, expected_receipt("confirmed", 3, None));

    // U+FEFF is JavaScript whitespace: padded "abcd" trims to 4 units.
    let mut padded = CommandProbe::ready(discount.clone(), readback.clone());
    assert_eq!(
        run(
            &mut padded,
            &actor(),
            &TransitionCommand {
                reason: Some("\u{FEFF}abcd\u{FEFF}".into()),
                ..command.clone()
            }
        )
        .unwrap_err()
        .code,
        "DISCOUNT_APPROVAL_REASON_REQUIRED"
    );

    // 1,000 Thai symbols are 1,000 UTF-16 units (3,000 UTF-8 bytes).
    let mut thai = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    let receipt = run(
        &mut thai,
        &actor(),
        &TransitionCommand {
            reason: Some("ก".repeat(1_000)),
            ..submit()
        },
    )
    .unwrap();
    assert_eq!(receipt, expected_receipt("submitted", 2, None));

    let mut over = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    assert_eq!(
        run(
            &mut over,
            &actor(),
            &TransitionCommand {
                reason: Some("ก".repeat(1_001)),
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "COMMERCE_ORDER_REQUEST_INVALID"
    );
    assert!(over.calls.is_empty());
}

mod purchase_receipt {
    //! Portable Commerce purchase-receipt facts: valuation rounding, request
    //! validation, idempotency replay and persisted receipt shape.
    use erpx_server_commerce::public::order::{CommerceOrderActor, OrderCommandReceipt};
    use erpx_server_commerce::public::receipt::{
        CommerceReceipt, CommerceReceiptLine, ReceiptAllocationCommand, ReceiptCommand,
        ReceiptDisposition, ReceiptFulfillmentLinePlan, ReceiptLineCommand, ReceiptOrder,
        ReceiptOrderLine, calculate_legacy_goods_receipt, expected_fulfilled_lines,
        expected_order_state, line_value, prior_receipt,
    };
    use erpx_server_shared::public::{
        AssignmentId, CommandKey, CompanyId, HoldingId, RecordId, UserId,
    };

    fn actor() -> CommerceOrderActor {
        CommerceOrderActor {
            user_id: UserId::new("receiver").unwrap(),
            person_user_ids: vec![],
            holding_id: HoldingId::new("holding").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: Some(RecordId::new("branch").unwrap()),
            department_id: None,
            assignment_id: AssignmentId::new("assignment").unwrap(),
            role: Some("warehouse_manager".into()),
            authority_revision: Some(1),
            assignment_version: Some(1),
            permissions: vec!["commerce.read".into(), "inventory.prepare".into()],
            modules: vec!["inventory".into()],
            expires_at: 2_000_000_000_000,
        }
    }

    fn order(currency: &str, base_currency: &str) -> ReceiptOrder {
        ReceiptOrder {
            id: "order-1".into(),
            state: "confirmed".into(),
            version: 3,
            warehouse_id: "warehouse".into(),
            branch_id: "branch".into(),
            department_id: None,
            currency: currency.into(),
            base_currency: base_currency.into(),
            order_date: "2026-11-01".into(),
            lines: vec![ReceiptOrderLine {
                id: "line-1".into(),
                product_id: "product".into(),
                item_type: "stock".into(),
                description: "Product".into(),
                quantity_milli: 3_000,
                fulfilled_milli: 0,
                price_minor: 3_333,
                tax_bps: 700,
                subtotal_minor: 10_000,
                tax_minor: 700,
                prior_subtotal_minor: 0,
                prior_tax_minor: 0,
                tracking_mode: "lot".into(),
            }],
        }
    }

    fn line(allocations: Vec<ReceiptAllocationCommand>) -> ReceiptLineCommand {
        ReceiptLineCommand {
            line_id: "line-1".into(),
            quantity_milli: 1_000,
            allocations,
        }
    }

    fn allocation(identity: &str) -> ReceiptAllocationCommand {
        ReceiptAllocationCommand {
            lot_code: Some(identity.into()),
            serial_number: None,
            quantity_milli: 1_000,
            disposition: ReceiptDisposition::Available,
            quarantine_reason: None,
            quarantine_due_date: None,
            manufactured_date: None,
            expiry_date: None,
        }
    }

    fn command(allocations: Vec<ReceiptAllocationCommand>) -> ReceiptCommand {
        ReceiptCommand {
            key: CommandKey::new("receipt-one").unwrap(),
            order_id: RecordId::new("order-1").unwrap(),
            expected_version: 3,
            date: "2026-11-04".into(),
            lines: vec![line(allocations)],
        }
    }

    fn receipt() -> CommerceReceipt {
        CommerceReceipt {
            order_id: "order-1".into(),
            fulfillment_id: "delivery-receipt-one".into(),
            state: "partially_fulfilled".into(),
            version: 4,
            total_value_minor: 3_333,
            lines: vec![CommerceReceiptLine {
                line_id: "line-1".into(),
                quantity_milli: 1_000,
                value_minor: 3_333,
            }],
        }
    }

    #[test]
    fn legacy_receipt_calculation_has_no_ledger_effect_and_retains_typed_source() {
        let input = command(vec![allocation("L-A")]);
        let source = order("THB", "THB");
        let calculation = calculate_legacy_goods_receipt(
            &actor(),
            &input,
            &source,
            erpx_server_shared::public::TimestampMillis::new(7),
        )
        .unwrap();
        assert_eq!(calculation.order, source);
        assert_eq!(calculation.fulfillment.id, "delivery-receipt-one");
        assert_eq!(
            calculation.fulfillment.number,
            "GR-1623126585A29ED7E9F75697"
        );
        assert_eq!(calculation.fulfillment.lines[0].value_minor, 3_333);
        assert_eq!(calculation.receipt.state, "partially_fulfilled");
        assert_eq!(
            calculation.workflow_result_json,
            r#"{"id":"order-1","fulfillmentId":"delivery-receipt-one","version":4}"#
        );
        let mut wrong = source.clone();
        wrong.id = "other".into();
        assert_eq!(
            calculate_legacy_goods_receipt(
                &actor(),
                &input,
                &wrong,
                erpx_server_shared::public::TimestampMillis::new(7)
            )
            .unwrap_err()
            .code,
            "COMMERCE_ORDER_CHANGED"
        );
        assert_eq!(
            calculate_legacy_goods_receipt(
                &actor(),
                &input,
                &order("USD", "THB"),
                erpx_server_shared::public::TimestampMillis::new(7)
            )
            .unwrap_err()
            .code,
            "RECEIPT_FX_GUARD_REQUIRED"
        );
    }

    #[test]
    fn legacy_receipt_total_checks_safe_integer_across_multiple_lines() {
        let mut source = order("THB", "THB");
        let first = &mut source.lines[0];
        first.tracking_mode = "none".into();
        first.quantity_milli = 1000;
        first.subtotal_minor = 9_007_199_254_740_990;
        first.tax_minor = 0;
        let mut second = first.clone();
        second.id = "line-2".into();
        second.product_id = "product-2".into();
        second.subtotal_minor = 1;
        source.lines.push(second);
        let mut input = command(vec![]);
        input.lines.push(ReceiptLineCommand {
            line_id: "line-2".into(),
            quantity_milli: 1000,
            allocations: vec![],
        });
        let accepted = calculate_legacy_goods_receipt(
            &actor(),
            &input,
            &source,
            erpx_server_shared::public::TimestampMillis::new(7),
        )
        .unwrap();
        assert_eq!(accepted.receipt.total_value_minor, 9_007_199_254_740_991);
        source.lines[1].subtotal_minor = 2;
        assert_eq!(
            calculate_legacy_goods_receipt(
                &actor(),
                &input,
                &source,
                erpx_server_shared::public::TimestampMillis::new(7)
            )
            .unwrap_err()
            .code,
            "MONEY_OVERFLOW"
        );
    }

    #[test]
    fn receipt_rounding_and_prior_remaining_match_installed_typescript_node_oracle() {
        use std::process::Command;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let node = std::env::var_os("ERPX_NODE_BIN")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| root.join(".runtime/node/node-v26.11.1-darwin-arm64/bin/node"));
        let script = r#"if(process.version!=='v26.11.1')throw Error('NODE_VERSION');const fs=require('node:fs'),s=fs.readFileSync('packages/feature-commerce/src/lib/commerce-math.ts','utf8');let a=s.slice(s.indexOf('export function scaledAmount'),s.indexOf('export function lineAmounts')).replace('export function','function').replaceAll(':number','');let b=s.slice(s.indexOf('export function lineAmounts')).replace('export function','function').replaceAll(':number','');const lineAmounts=new Function(a+b+';return lineAmounts')();const [qty,price,taxBps,total,fulfilled,subtotal,tax,priorSubtotal,priorTax]=JSON.parse(process.argv[1]);const amounts=qty===total-fulfilled?{subtotal:subtotal-priorSubtotal,tax:tax-priorTax}:lineAmounts(qty,price,taxBps);process.stdout.write(JSON.stringify(amounts));"#;
        for (qty, price, tax_bps, total, fulfilled, subtotal, tax, prior_subtotal, prior_tax) in [
            (1000, 3333, 700, 3000, 0, 10000, 700, 0, 0),
            (1000, 3333, 700, 3000, 2000, 10000, 700, 6667, 467),
            (1, 500, 0, 1000, 0, 500, 0, 0, 0),
        ] {
            let data = serde_json::json!([
                qty,
                price,
                tax_bps,
                total,
                fulfilled,
                subtotal,
                tax,
                prior_subtotal,
                prior_tax
            ])
            .to_string();
            let result = Command::new(&node)
                .current_dir(&root)
                .arg("-e")
                .arg(script)
                .arg(data)
                .output()
                .expect("pinned Node commerce math oracle");
            assert!(result.status.success(), "Node commerce math oracle failed");
            let expected: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
            let mut source = order("THB", "THB");
            let line = &mut source.lines[0];
            line.quantity_milli = total;
            line.fulfilled_milli = fulfilled;
            line.price_minor = price;
            line.tax_bps = tax_bps;
            line.subtotal_minor = subtotal;
            line.tax_minor = tax;
            line.prior_subtotal_minor = prior_subtotal;
            line.prior_tax_minor = prior_tax;
            let actual = line_value(&source, &source.lines[0], qty).unwrap();
            assert_eq!(
                actual.subtotal_minor,
                expected["subtotal"].as_i64().unwrap()
            );
            assert_eq!(actual.tax_minor, expected["tax"].as_i64().unwrap());
        }
    }

    #[test]
    fn partial_and_full_receipt_values_are_half_up_and_use_the_remaining_subtotal() {
        let order = order("THB", "THB");
        let partial = line_value(&order, &order.lines[0], 1_000).unwrap();
        assert_eq!((partial.subtotal_minor, partial.tax_minor), (3_333, 233));
        let full = line_value(&order, &order.lines[0], 3_000).unwrap();
        assert_eq!((full.subtotal_minor, full.tax_minor), (10_000, 700));
        let mut continuing = order.clone();
        continuing.lines[0].fulfilled_milli = 2_000;
        continuing.lines[0].prior_subtotal_minor = 6_667;
        continuing.lines[0].prior_tax_minor = 467;
        let final_part = line_value(&continuing, &continuing.lines[0], 1_000).unwrap();
        assert_eq!(
            (final_part.subtotal_minor, final_part.tax_minor),
            (3_333, 233)
        );
    }

    #[test]
    fn foreign_currency_fails_closed_until_finance_valuation_is_mounted() {
        let order = order("USD", "THB");
        let error = line_value(&order, &order.lines[0], 1_000).unwrap_err();
        assert_eq!(error.code, "RECEIPT_FX_GUARD_REQUIRED");
    }

    #[test]
    fn over_receipt_and_invalid_identity_are_refused_before_any_write() {
        let order = order("THB", "THB");
        let error = line_value(&order, &order.lines[0], 3_001).unwrap_err();
        assert_eq!(error.code, "OVER_FULFILLMENT");
        let duplicate = command(vec![
            allocation("L-A"),
            ReceiptAllocationCommand {
                lot_code: Some(" L-A ".into()),
                ..allocation("L-B")
            },
        ]);
        assert_eq!(
            duplicate.validate().unwrap_err().code,
            "LOT_ALLOCATION_DUPLICATE"
        );
        let mut quarantine = allocation("L-B");
        quarantine.disposition = ReceiptDisposition::Quarantine;
        assert_eq!(
            command(vec![quarantine]).validate().unwrap_err().code,
            "QUARANTINE_EVIDENCE_REQUIRED"
        );
        assert!(command(vec![allocation("L-A")]).validate().is_ok());
    }

    #[test]
    fn result_json_round_trips_and_changed_replay_conflicts() {
        let receipt = receipt();
        let json = receipt.to_result_json().unwrap();
        assert_eq!(CommerceReceipt::from_result_json(&json).unwrap(), receipt);
        let prior = OrderCommandReceipt {
            request_hash: "hash-one".into(),
            result_json: json,
            operation: Some("commerce.order.fulfill".into()),
            actor_id: Some("receiver".into()),
            record_id: Some("order-1".into()),
            authority_json: None,
        };
        assert_eq!(
            prior_receipt(Some(prior.clone()), "hash-one", "order-1")
                .unwrap()
                .unwrap(),
            receipt
        );
        assert_eq!(
            prior_receipt(Some(prior), "hash-two", "order-1")
                .unwrap_err()
                .code,
            "IDEMPOTENCY_CONFLICT"
        );
    }

    #[test]
    fn state_and_fulfilled_lines_follow_only_the_selected_lines() {
        let mut order = order("THB", "THB");
        order.lines.push(ReceiptOrderLine {
            id: "line-2".into(),
            product_id: "product-2".into(),
            item_type: "stock".into(),
            description: "Second".into(),
            quantity_milli: 1_000,
            fulfilled_milli: 0,
            price_minor: 1_000,
            tax_bps: 0,
            subtotal_minor: 1_000,
            tax_minor: 0,
            prior_subtotal_minor: 0,
            prior_tax_minor: 0,
            tracking_mode: "none".into(),
        });
        let first = receipt();
        assert_eq!(expected_order_state(&order, &first), "partially_fulfilled");
        let fulfillment = vec![ReceiptFulfillmentLinePlan {
            index: 0,
            order_line_id: "line-1".into(),
            quantity_milli: 1_000,
            value_minor: 3_333,
            subtotal_minor: 3_333,
            tax_minor: 233,
            stock_currency: "THB".into(),
        }];
        assert_eq!(
            expected_fulfilled_lines(&order, &fulfillment),
            vec![1_000, 0]
        );
        let complete = CommerceReceipt {
            lines: vec![
                CommerceReceiptLine {
                    line_id: "line-1".into(),
                    quantity_milli: 3_000,
                    value_minor: 10_000,
                },
                CommerceReceiptLine {
                    line_id: "line-2".into(),
                    quantity_milli: 1_000,
                    value_minor: 1_000,
                },
            ],
            ..first
        };
        assert_eq!(expected_order_state(&order, &complete), "fulfilled");
    }

    #[test]
    fn order_validation_keeps_current_company_scope_and_receivable_state() {
        let order = order("THB", "THB");
        assert!(order.validate(&actor()).is_ok());
        let foreign = CommerceOrderActor {
            company_id: CompanyId::new("foreign").unwrap(),
            ..actor()
        };
        // Company identity is enforced by the repository scope; the order shape
        // itself still rejects an out-of-scope branch or department.
        let other_branch = CommerceOrderActor {
            branch_id: Some(RecordId::new("other-branch").unwrap()),
            ..actor()
        };
        assert_eq!(
            order.validate(&other_branch).unwrap_err().code,
            "COMMERCE_RECEIPT_SOURCE_INVALID"
        );
        let mut service = order.clone();
        service.lines[0].item_type = "service".into();
        assert!(service.validate(&actor()).is_ok());
        let mut invalid = order;
        invalid.state = "draft".into();
        assert_eq!(
            invalid.validate(&foreign).unwrap_err().code,
            "COMMERCE_RECEIPT_SOURCE_INVALID"
        );
    }
}

// Save sequencing probe; the Finance methods intentionally use the trait's
// fail-closed defaults, while every simulated write is recorded.
impl erpx_server_commerce::public::order::CommerceOrderSaveRepository for CommandProbe {
    fn setup_currency<'a>(
        &'a mut self,
        _: &'a CompanyId,
    ) -> CommerceOrderFuture<'a, Option<String>, Self::Error> {
        self.calls.push("save-settings");
        Box::pin(async { Ok(Some("THB".into())) })
    }
    fn next_quote_number<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, String, Self::Error> {
        self.calls.push("save-number");
        Box::pin(async { Ok("QT-1".into()) })
    }
    fn save_warehouse<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a str,
    ) -> CommerceOrderFuture<
        'a,
        Option<erpx_server_commerce::public::order::SaveOrderWarehouse>,
        Self::Error,
    > {
        self.calls.push("save-warehouse");
        Box::pin(async { Ok(None) })
    }
    fn save_product<'a>(
        &'a mut self,
        _: &'a CompanyId,
        id: &'a str,
    ) -> CommerceOrderFuture<
        'a,
        Option<erpx_server_commerce::public::order::SaveOrderProduct>,
        Self::Error,
    > {
        self.calls.push("save-product");
        let missing = self.missing_save_product;
        Box::pin(async move {
            Ok(
                (!missing).then(|| erpx_server_commerce::public::order::SaveOrderProduct {
                    id: id.into(),
                    name: "Service".into(),
                    item_type: "service".into(),
                    version: 1,
                }),
            )
        })
    }
    fn save_party_valid<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("save-party");
        Box::pin(async { Ok(true) })
    }
    fn save_price_list<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: Option<&'a str>,
    ) -> CommerceOrderFuture<
        'a,
        Option<erpx_server_commerce::public::order::SaveOrderPriceList>,
        Self::Error,
    > {
        self.calls.push("save-pricing");
        let list = self.save_list.clone();
        Box::pin(async move { Ok(list) })
    }
    fn insert_order<'a>(
        &'a mut self,
        _: &'a erpx_server_commerce::public::order::SaveOrderPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("save-insert");
        Box::pin(async { Ok(true) })
    }
    fn update_order<'a>(
        &'a mut self,
        _: &'a erpx_server_commerce::public::order::SaveOrderPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("save-update");
        Box::pin(async { Ok(true) })
    }
    fn delete_order_lines<'a>(
        &'a mut self,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("save-delete-lines");
        Box::pin(async { Ok(()) })
    }
    fn insert_order_line<'a>(
        &'a mut self,
        _: &'a erpx_server_commerce::public::order::SaveOrderLinePlan,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("save-insert-line");
        Box::pin(async { Ok(()) })
    }
    fn readback_lines<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Vec<SalesOrderLineRow>, Self::Error> {
        self.calls.push("save-readback-lines");
        Box::pin(async { Ok(vec![]) })
    }
}

#[test]
fn native_save_without_money_port_fails_closed_but_replay_and_retained_modes_do_not_require_it() {
    use erpx_server_commerce::public::order::{
        OrderKind, SaveOrderCommand, SaveOrderInput, SaveOrderLineInput, save_order_outcome,
        save_order_outcome_native,
    };
    let command = SaveOrderCommand {
        key: CommandKey::new(KEY).unwrap(),
        order_id: None,
        input: SaveOrderInput {
            kind: OrderKind::Quote,
            party_id: "party".into(),
            warehouse_id: None,
            order_date: "2026-11-01".into(),
            due_date: "2026-11-10".into(),
            note: String::new(),
            price_list_id: None,
            discount_reason: None,
            expected_version: None,
            lines: vec![SaveOrderLineInput {
                product_id: "product".into(),
                quantity: "1".into(),
                price: "100".into(),
                tax_bps: 0,
            }],
        },
    };
    let mut fresh = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "draft", 1, "creator"),
    );
    let error = sqlx::test_block_on(save_order_outcome_native(
        &actor(),
        &command,
        &mut fresh,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "COMMERCE_SAVE_MONEY_REQUIRED");
    for write in [
        "save-insert",
        "save-update",
        "save-delete-lines",
        "save-insert-line",
        "receipt-insert",
        "audit",
        "outbox",
    ] {
        assert!(!fresh.calls.contains(&write), "{write}");
    }
    let mut replay = CommandProbe {
        hash: HASH.into(),
        receipt: Some(OrderCommandReceipt {
            request_hash: HASH.into(),
            result_json: json!({"id":format!("order-{KEY}"),"version":1}).to_string(),
            operation: Some("commerce.order.create".into()),
            actor_id: Some("user".into()),
            record_id: Some(format!("order-{KEY}")),
            authority_json: None,
        }),
        ..CommandProbe::default()
    };
    let outcome = sqlx::test_block_on(save_order_outcome_native(
        &actor(),
        &command,
        &mut replay,
        NOW,
    ))
    .unwrap();
    assert!(outcome.plan.is_none());
    assert_eq!(outcome.receipt.id, format!("order-{KEY}"));
    assert_eq!(replay.calls, ["lock-command", "digest", "receipt"]);
    let mut retained = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "draft", 1, "creator"),
    );
    retained.readback = None;
    let error = sqlx::test_block_on(save_order_outcome(&actor(), &command, &mut retained, NOW))
        .unwrap_err();
    assert_ne!(error.code, "COMMERCE_SAVE_MONEY_REQUIRED");
    assert!(retained.calls.contains(&"save-insert"));
    assert!(retained.calls.contains(&"receipt-insert"));
}

#[test]
fn native_save_validates_warehouse_and_entire_price_list_before_product_reads() {
    use erpx_server_commerce::public::order::{
        OrderKind, SaveOrderCommand, SaveOrderInput, SaveOrderLineInput, SaveOrderPriceItem,
        SaveOrderPriceList, save_order_outcome_native,
    };
    let mut command = SaveOrderCommand {
        key: CommandKey::new(KEY).unwrap(),
        order_id: None,
        input: SaveOrderInput {
            kind: OrderKind::Quote,
            party_id: "party".into(),
            warehouse_id: Some("missing-warehouse".into()),
            order_date: "2026-11-01".into(),
            due_date: "2026-11-10".into(),
            note: String::new(),
            price_list_id: Some("missing-list".into()),
            discount_reason: None,
            expected_version: None,
            lines: vec![
                SaveOrderLineInput {
                    product_id: "first".into(),
                    quantity: "1".into(),
                    price: "100".into(),
                    tax_bps: 700,
                },
                SaveOrderLineInput {
                    product_id: "second".into(),
                    quantity: "1".into(),
                    price: "100".into(),
                    tax_bps: 0,
                },
            ],
        },
    };
    let mut warehouse = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "draft", 1, "creator"),
    );
    let error = sqlx::test_block_on(save_order_outcome_native(
        &actor(),
        &command,
        &mut warehouse,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "WAREHOUSE_INVALID");
    assert!(!warehouse.calls.contains(&"save-pricing"));
    assert!(!warehouse.calls.contains(&"save-product"));
    command.input.warehouse_id = None;
    command.input.price_list_id = Some("list".into());
    let list = SaveOrderPriceList {
        id: "list".into(),
        version: 1,
        max_auto_discount_bps: 0,
        items: vec![
            SaveOrderPriceItem {
                product_id: "first".into(),
                price_minor: 10000,
                tax_bps: 700,
            },
            SaveOrderPriceItem {
                product_id: "second".into(),
                price_minor: 10000,
                tax_bps: 700,
            },
        ],
    };
    let mut tax = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "draft", 1, "creator"),
    );
    tax.save_list = Some(list.clone());
    tax.missing_save_product = true;
    let error = sqlx::test_block_on(save_order_outcome_native(&actor(), &command, &mut tax, NOW))
        .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_TAX_REQUIRED");
    assert!(!tax.calls.contains(&"save-product"));
    command.input.lines[1].product_id = "not-listed".into();
    command.input.lines[1].tax_bps = 700;
    let mut missing = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "draft", 1, "creator"),
    );
    missing.save_list = Some(list.clone());
    missing.missing_save_product = true;
    let error = sqlx::test_block_on(save_order_outcome_native(
        &actor(),
        &command,
        &mut missing,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_ITEM_REQUIRED");
    assert!(!missing.calls.contains(&"save-product"));
    command.input.lines[1].product_id = "second".into();
    let mut product = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "draft", 1, "creator"),
    );
    product.save_list = Some(list);
    product.missing_save_product = true;
    let error = sqlx::test_block_on(save_order_outcome_native(
        &actor(),
        &command,
        &mut product,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "PRODUCT_INVALID");
    assert!(product.calls.contains(&"save-product"));
    for probe in [&warehouse, &tax, &missing, &product] {
        for write in [
            "save-insert",
            "save-update",
            "save-insert-line",
            "receipt-insert",
            "audit",
            "outbox",
        ] {
            assert!(!probe.calls.contains(&write), "{write}");
        }
    }
}

#[test]
fn native_save_kind_lock_precedes_stale_warehouse_and_pricing() {
    use erpx_server_commerce::public::order::{
        OrderKind, SaveOrderCommand, SaveOrderInput, SaveOrderLineInput, save_order_outcome_native,
    };
    let command = SaveOrderCommand {
        key: CommandKey::new(KEY).unwrap(),
        order_id: Some(RecordId::new("order-1").unwrap()),
        input: SaveOrderInput {
            kind: OrderKind::Sales,
            party_id: "party".into(),
            warehouse_id: Some("missing-warehouse".into()),
            order_date: "2026-11-01".into(),
            due_date: "2026-11-10".into(),
            note: String::new(),
            price_list_id: Some("missing-list".into()),
            discount_reason: None,
            expected_version: Some(99),
            lines: vec![SaveOrderLineInput {
                product_id: "first".into(),
                quantity: "1".into(),
                price: "100".into(),
                tax_bps: 700,
            }],
        },
    };
    let mut probe = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "draft", 1, "creator"),
    );
    let error = sqlx::test_block_on(save_order_outcome_native(
        &actor(),
        &command,
        &mut probe,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "ORDER_KIND_LOCKED");
    for forbidden in [
        "save-party",
        "save-warehouse",
        "save-pricing",
        "save-product",
        "save-insert",
        "save-update",
        "receipt-insert",
        "audit",
        "outbox",
    ] {
        assert!(!probe.calls.contains(&forbidden), "{forbidden}");
    }
}

#[test]
fn native_save_kind_precedes_date_and_duplicate_semantics_without_weakening_shape() {
    use erpx_server_commerce::public::order::{
        OrderKind, SaveOrderCommand, SaveOrderInput, SaveOrderLineInput, save_order_outcome_native,
    };
    let line = SaveOrderLineInput {
        product_id: "first".into(),
        quantity: "1".into(),
        price: "100".into(),
        tax_bps: 700,
    };
    let base = SaveOrderCommand {
        key: CommandKey::new(KEY).unwrap(),
        order_id: Some(RecordId::new("order-1").unwrap()),
        input: SaveOrderInput {
            kind: OrderKind::Sales,
            party_id: "party".into(),
            warehouse_id: None,
            order_date: "2026-11-01".into(),
            due_date: "2026-11-10".into(),
            note: String::new(),
            price_list_id: None,
            discount_reason: None,
            expected_version: Some(99),
            lines: vec![line.clone()],
        },
    };
    for duplicate in [false, true] {
        let mut command = base.clone();
        if duplicate {
            command.input.lines.push(line.clone());
        } else {
            command.input.due_date = "2026-10-31".into();
        }
        let mut changed_kind = CommandProbe::ready(
            order("quote", "draft", 1, "creator"),
            order("quote", "draft", 1, "creator"),
        );
        assert_eq!(
            sqlx::test_block_on(save_order_outcome_native(
                &actor(),
                &command,
                &mut changed_kind,
                NOW
            ))
            .unwrap_err()
            .code,
            "ORDER_KIND_LOCKED"
        );
        assert!(!changed_kind.calls.contains(&"save-warehouse"));
        assert!(!changed_kind.calls.contains(&"save-insert"));
        assert!(!changed_kind.calls.contains(&"receipt-insert"));
        let mut same_kind = CommandProbe::ready(
            order("sales", "draft", 1, "creator"),
            order("sales", "draft", 1, "creator"),
        );
        let error = sqlx::test_block_on(save_order_outcome_native(
            &actor(),
            &command,
            &mut same_kind,
            NOW,
        ))
        .unwrap_err();
        assert_eq!(
            error.code,
            if duplicate {
                "DUPLICATE_PRODUCT"
            } else {
                "INVALID_DATES"
            }
        );
        assert!(!same_kind.calls.contains(&"save-warehouse"));
        assert!(!same_kind.calls.contains(&"receipt-insert"));
    }
    let mut zero = base.clone();
    zero.input.lines[0].quantity = "0".into();
    let mut invalid = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "draft", 1, "creator"),
    );
    let error = sqlx::test_block_on(save_order_outcome_native(
        &actor(),
        &zero,
        &mut invalid,
        NOW,
    ))
    .unwrap_err();
    assert_ne!(error.code, "ORDER_KIND_LOCKED");
    assert!(!invalid.calls.contains(&"lock"));
    assert!(!invalid.calls.contains(&"receipt-insert"));
}
