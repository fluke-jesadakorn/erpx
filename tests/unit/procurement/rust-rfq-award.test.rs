use erpx_server_procurement::public::rfq_award::*;
use erpx_server_procurement::public::rfq_request_status::rfq_source_blocked;
use erpx_server_procurement::public::rfq_transition::*;
use erpx_server_procurement::public::rfq_transition_normalization::normalize_rfq_transition;
use erpx_server_records::public::rfq_request_source::RfqRequestFacts;
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::source_ordered_json::SourceOrderedJson;
#[path = "../../../src/composition/src/procurement_rfq_approval.rs"]
mod rfq_approval;
#[path = "../../../src/composition/src/procurement_rfq_quantity.rs"]
mod rfq_quantity;

#[test]
fn requisition_quantities_match_typescript_math_and_exception_classes() {
    use rfq_quantity::{RfqQuantityError, rfq_quotation_quantities};
    let cases = [
        r#"[]"#,
        r#"[{"quantity":"1"},{"quantity":" 1.2500 "}]"#,
        r#"[{"quantity":"9007199254740.991"}]"#,
        r#"[{"quantity":"9007199254740.992"}]"#,
        r#"[{"quantity":"1.0001"}]"#,
        r#"[{"quantity":"-2"}]"#,
        r#"[{"quantity":"0"}]"#,
        r#"[{"quantity":"1."}]"#,
        r#"[{"quantity":"+1"}]"#,
        r#"[{"quantity":"\ufeff1.01\u00a0"}]"#,
        r#"[{"quantity":"\ud800"}]"#,
        r#"[null]"#,
        r#"[{}]"#,
        r#"[{"quantity":null}]"#,
        r#"[{"quantity":5}]"#,
        r#"null"#,
        r#"{}"#,
        "[",
    ];
    let script = r#"const {quantityMilli}=require('./packages/feature-commerce/src/lib/commerce-math.ts');const cases=JSON.parse(process.argv[1]);process.stdout.write(JSON.stringify(cases.map(raw=>{try{const items=JSON.parse(raw);return {values:items.map(item=>quantityMilli(item.quantity))}}catch(e){return {name:e.name,...(e.name==='Error'?{message:e.message}:{})}}})))"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args([
                "--import=tsx",
                "-e",
                script,
                &serde_json::to_string(&cases).unwrap(),
            ])
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let actual:Vec<serde_json::Value>=cases.iter().map(|raw|match rfq_quotation_quantities(raw) {
        Ok(values)=>serde_json::json!({"values":values}),
        Err(RfqQuantityError::JsonSyntax)=>serde_json::json!({"name":"SyntaxError"}),
        Err(RfqQuantityError::ItemsNull|RfqQuantityError::ItemsNotArray|RfqQuantityError::ItemNull|RfqQuantityError::QuantityMissing|RfqQuantityError::QuantityNull|RfqQuantityError::QuantityNotString)=>serde_json::json!({"name":"TypeError"}),
        Err(RfqQuantityError::DecimalInvalid)=>serde_json::json!({"name":"Error","message":"Money must be a decimal string."}),
        Err(RfqQuantityError::DecimalPrecision)=>serde_json::json!({"name":"Error","message":"Money supports at most 3 decimal places."}),
        Err(RfqQuantityError::DecimalRange)=>serde_json::json!({"name":"Error","message":"Money amount is too large."}),
        Err(RfqQuantityError::QuantityNonpositive)=>serde_json::json!({"name":"Error","message":"Quantity must be positive."}),
    }).collect();
    assert_eq!(serde_json::json!(actual), expected);
    let large = format!("[{{\"quantity\":\"{}1\"}}]", "0".repeat(2_100_000));
    assert_eq!(rfq_quotation_quantities(&large), Ok(vec![1000]));
}

#[test]
fn quotation_allocation_numeric_and_duplicate_keys_match_javascript() {
    use erpx_server_procurement::public::rfq_request_allocations::RfqRequestAllocation;
    use rfq_quantity::rfq_quotation_allocation_amounts;
    let rows = vec![
        RfqRequestAllocation {
            ordinal: None,
            allocated_milli_decimal: "5".into(),
        },
        RfqRequestAllocation {
            ordinal: Some(0),
            allocated_milli_decimal: "20".into(),
        },
        RfqRequestAllocation {
            ordinal: Some(1),
            allocated_milli_decimal: "9007199254740993".into(),
        },
        RfqRequestAllocation {
            ordinal: Some(-1),
            allocated_milli_decimal: "1".into(),
        },
    ];
    let actual = rfq_quotation_allocation_amounts(&[1000, 2500], &rows).unwrap();
    let script = "const m=new Map([[Number(null),Number('5')],[Number(0),Number('20')],[Number(1),Number('9007199254740993')],[Number(-1),Number('1')]]);process.stdout.write(JSON.stringify([1000,2500].map((n,i)=>{const a=m.get(i)??0;return [n,a,Math.max(0,n-a)]})))";
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .args(["-e", script])
            .output()
            .unwrap();
    assert!(output.status.success());
    let expected: Vec<(i64, f64, f64)> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        actual
            .iter()
            .map(|v| (v.requested_milli, v.allocated_milli, v.remaining_milli))
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn allocation_materialization_uses_postgres_adapter_safe_number_before_item_mapping() {
    use erpx_server_procurement::public::rfq_request_allocations::RfqRequestAllocation;
    use rfq_quantity::{rfq_quotation_items_json,RfqItemMapError};
    let rows=vec![RfqRequestAllocation{ordinal:Some(99),allocated_milli_decimal:"9007199254740992".into()}];
    assert!(matches!(rfq_quotation_items_json(r#"[{"quantity":"bad"}]"#,&rows),Err(RfqItemMapError::DatabaseNumberOutOfRange)));
    let output=std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args(["--conditions=react-server","--import=tsx","-e","const {safeNumber}=require('./packages/server-runtime/src/adapters/runtime/postgres.ts');try{safeNumber('9007199254740992')}catch(e){process.stdout.write(e.name+':'+e.message)}"])
        .output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(),"RangeError:DATABASE_NUMBER_OUT_OF_RANGE");
}

#[test]
fn quotation_items_preserve_full_javascript_spread_and_unit_fallback() {
    use erpx_server_procurement::public::rfq_request_allocations::RfqRequestAllocation;
    use rfq_quantity::rfq_quotation_items_json;
    let raw = r#"[{"z":"extra","2":"integer-key","quantity":"1.5","unit":"old","requestedMilli":"old","nested":{"x":"\ud800"},"unit":"","\ud800":"key"},{"unit":{"custom":1},"quantity":"2.0000","allocatedMilli":"old","remainingMilli":null,"name":"quoted"},{"quantity":"1","unit":true,"1":3,"__proto__":{"safe":true}},{"quantity":"1","unit":1e999}]"#;
    let huge = "9007199254740991".to_owned();
    let rows = vec![
        RfqRequestAllocation {
            ordinal: Some(0),
            allocated_milli_decimal: "1500".into(),
        },
        RfqRequestAllocation {
            ordinal: Some(1),
            allocated_milli_decimal: "1".into(),
        },
        RfqRequestAllocation {
            ordinal: Some(2),
            allocated_milli_decimal: huge.clone(),
        },
    ];
    let actual = rfq_quotation_items_json(raw, &rows).unwrap();
    let script = r#"const {quantityMilli}=require('./packages/feature-commerce/src/lib/commerce-math.ts');const items=JSON.parse(process.argv[1]), m=new Map([[0,1500],[1,1],[2,Number(process.argv[2])]]);const mapped=items.map((item,index)=>{const requestedMilli=quantityMilli(item.quantity),allocatedMilli=m.get(index)??0;return {...item,unit:item.unit||'each',requestedMilli,allocatedMilli,remainingMilli:Math.max(0,requestedMilli-allocatedMilli)}});process.stdout.write(JSON.stringify({json:JSON.stringify(mapped),fullyAllocated:mapped.every(x=>x.remainingMilli===0),unitMatchesEach:mapped.map(x=>x.unit==='each')}))"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--import=tsx", "-e", script, raw, &huge])
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual.json, expected["json"].as_str().unwrap());
    assert_eq!(
        actual.fully_allocated,
        expected["fullyAllocated"].as_bool().unwrap()
    );
    let each: Vec<u16> = "each".encode_utf16().collect();
    let matches: Vec<bool> = (0..4)
        .map(|index| actual.unit_strictly_equals(index, &each))
        .collect();
    assert_eq!(serde_json::json!(matches), expected["unitMatchesEach"]);
    assert!(
        matches!(actual.effective_unit(3),Some(rfq_quantity::RfqEffectiveUnit::Number(n)) if n.is_infinite())
    );
    assert!(rfq_quotation_items_json("[]", &[]).unwrap().fully_allocated);
}

#[test]
fn designated_request_step_matches_javascript_json_and_scope_projection() {
    use rfq_approval::{RfqDesignatedStep, rfq_designated_step};
    let cases = [
        r#"{}"#,
        "null",
        "false",
        "{",
        r#"{"route":null}"#,
        r#"{"route":{}}"#,
        r#"{"route":{"steps":[]}}"#,
        r#"{"route":{"steps":[{"userId":"actor","snapshot":{"departmentPolicyVersion":3}}]}}"#,
        r#"{"currentStep":1,"route":{"steps":[{}, {"userId":"other","snapshot":{"departmentPolicyVersion":3}}]}}"#,
        r#"{"currentStep":"\ud800","route":{"steps":{"\ud800":{"userId":"actor","snapshot":{"departmentPolicyVersion":"yes"}}}}}"#,
        r#"{"currentStep":"01","route":{"steps":[{"userId":"actor","snapshot":{"departmentPolicyVersion":4}}]}}"#,
        r#"{"currentStep":true,"route":{"steps":{"true":{"userId":"actor","snapshot":{"departmentPolicyVersion":true}}}}}"#,
        r#"{"route":{"steps":[{"userId":"actor","snapshot":{"departmentPolicyVersion":0}}]}}"#,
        r#"{"currentStep":[1],"route":{"steps":{"1":{"userId":"actor","snapshot":{"departmentPolicyVersion":1}}}}}"#,
        r#"{"currentStep":{},"route":{"steps":{"[object Object]":{"userId":"actor","snapshot":{"departmentPolicyVersion":3}}}}}"#,
        r#"{"currentStep":{"toString":null},"route":{"steps":[]}}"#,
        r#"{"currentStep":1e400,"route":{"steps":{"Infinity":{"userId":"actor","snapshot":{"departmentPolicyVersion":4}}}}}"#,
    ];
    let script = r#"const cases=JSON.parse(process.argv[1]);const out=cases.map(s=>{try{let a;try{a=JSON.parse(s)}catch{return ['syntax']};const current=a.route?.steps[a.currentStep??0];const designated=!!current?.snapshot?.departmentPolicyVersion && (current.userId==='actor'||current.userId==='alias');return designated?['policy']:['none']}catch{return ['null-property']}});process.stdout.write(JSON.stringify(out))"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .args(["-e", script, &serde_json::to_string(&cases).unwrap()])
            .output()
            .unwrap();
    assert!(output.status.success());
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let actual: Vec<serde_json::Value> = cases
        .iter()
        .map(
            |raw| match rfq_designated_step(raw, |user| user == "actor" || user == "alias") {
                RfqDesignatedStep::None => serde_json::json!(["none"]),
                RfqDesignatedStep::DepartmentPolicy => serde_json::json!(["policy"]),
                RfqDesignatedStep::JsonSyntax => serde_json::json!(["syntax"]),
                RfqDesignatedStep::NullProperty => serde_json::json!(["null-property"]),
                RfqDesignatedStep::PropertyKeyTypeError => serde_json::json!(["null-property"]),
                RfqDesignatedStep::RuntimeCoercionUnproven => panic!("case needs runtime coercion"),
            },
        )
        .collect();
    assert_eq!(serde_json::json!(actual), expected);
}

#[test]
fn designated_step_property_key_handles_deep_nested_arrays_without_recursion() {
    let depth = 20_000;
    let raw = format!(
        "{{\"currentStep\":{}1{},\"route\":{{\"steps\":[]}}}}",
        "[".repeat(depth),
        "]".repeat(depth)
    );
    assert_eq!(
        rfq_approval::rfq_designated_step(&raw, |_| true),
        rfq_approval::RfqDesignatedStep::RuntimeCoercionUnproven
    );
    let node = std::env::var("ERPX_NODE_BIN").expect("pinned Node required");
    let error_script = "try{const a=JSON.parse(process.argv[1]);void a.route?.steps[a.currentStep??0];process.stdout.write('none')}catch(e){process.stdout.write(e.name)}";
    let deep = std::process::Command::new(&node)
        .args(["-e", error_script, &raw])
        .output()
        .unwrap();
    assert!(deep.status.success());
    assert_eq!(deep.stdout, b"RangeError");
    let normal = format!(
        "{{\"currentStep\":{}1{},\"route\":{{\"steps\":[]}}}}",
        "[".repeat(200),
        "]".repeat(200)
    );
    let script = "const a=JSON.parse(process.argv[1]);const x=a.route?.steps[a.currentStep??0];process.stdout.write(String(x===undefined))";
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .args(["-e", script, &normal])
            .output()
            .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"true");
    assert_eq!(
        rfq_approval::rfq_designated_step(&normal, |_| true),
        rfq_approval::RfqDesignatedStep::RuntimeCoercionUnproven
    );
}

#[test]
fn request_prior_reviewer_matches_javascript_some_and_short_circuit() {
    use rfq_approval::{RfqPriorReviewer, rfq_prior_reviewer};
    let cases = [
        "{}",
        "null",
        "{",
        r#"{"decisions":{}}"#,
        r#"{"decisions":[null]}"#,
        r#"{"decisions":[{"userId":"actor"},null]}"#,
        r#"{"decisions":[{"userId":"other"},null]}"#,
        r#"{"decisions":null,"decidedBy":"alias"}"#,
        r#"{"decisions":[false,4,"text"]}"#,
        r#"{"decisions":[{"userId":"\ud800"}],"decidedBy":"actor"}"#,
    ];
    let script = r#"const cases=JSON.parse(process.argv[1]);const same=x=>typeof x==='string'&&(x==='actor'||x==='alias');process.stdout.write(JSON.stringify(cases.map(s=>{try{let v;try{v=JSON.parse(s)}catch{return 'syntax'};return v.decisions?.some(d=>same(d.userId))||same(v.decidedBy)?'yes':'no'}catch{return 'type'}})))"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .args(["-e", script, &serde_json::to_string(&cases).unwrap()])
            .output()
            .unwrap();
    assert!(output.status.success());
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let actual: Vec<&str> = cases
        .iter()
        .map(
            |raw| match rfq_prior_reviewer(raw, |id| id == "actor" || id == "alias") {
                RfqPriorReviewer::Yes => "yes",
                RfqPriorReviewer::No => "no",
                RfqPriorReviewer::JsonSyntax => "syntax",
                RfqPriorReviewer::PropertyTypeError => "type",
            },
        )
        .collect();
    assert_eq!(actual, expected);
}

fn fixture() -> (RfqAward, RfqPurchaseOrder) {
    (
        RfqAward {
            status: "approved".into(),
            request_id: Some("request".into()),
            selected_offer_id: Some("offer".into()),
            order_id: Some("order".into()),
            warehouse_id: "warehouse".into(),
            needed_date: "2026-12-01".into(),
            demands: vec![RfqAwardDemand {
                id: "line".into(),
                product_id: "product".into(),
                quantity_milli: 1250,
            }],
            offers: vec![RfqAwardOffer {
                id: "offer".into(),
                supplier_id: "supplier".into(),
                valid_until: "2026-11-30".into(),
            }],
            offer_lines: vec![RfqAwardOfferLine {
                offer_id: "offer".into(),
                rfq_line_id: "line".into(),
                price_minor: 1234,
                tax_bps: 700,
            }],
        },
        RfqPurchaseOrder {
            kind: "purchase".into(),
            request_id: Some("request".into()),
            party_id: "supplier".into(),
            warehouse_id: "warehouse".into(),
            due_date: "2026-12-01".into(),
            order_date: "2026-11-29".into(),
            lines: vec![RfqPurchaseOrderLine {
                product_id: "product".into(),
                quantity_milli: 1250,
                price_minor: 1234,
                tax_bps: 700,
            }],
        },
    )
}

#[test]
fn award_comparison_preserves_error_precedence_and_edit_rules() {
    let (mut award, mut order) = fixture();
    assert_eq!(
        assert_rfq_order(&award, &order, "order", false, "2026-11-30")
            .unwrap()
            .id,
        "offer"
    );
    order.kind = "sale".into();
    order.request_id = None;
    assert_eq!(
        assert_rfq_order(&award, &order, "order", false, "2026-12-01")
            .unwrap_err()
            .code,
        "RFQ_AWARD_REQUIRED"
    );
    order.kind = "purchase".into();
    assert_eq!(
        assert_rfq_order(&award, &order, "order", false, "2026-12-01")
            .unwrap_err()
            .code,
        "RFQ_REQUEST_LINK_REQUIRED"
    );
    order.request_id = Some("request".into());
    order.lines[0].tax_bps = 701;
    assert_eq!(
        assert_rfq_order(&award, &order, "order", false, "2026-12-01")
            .unwrap_err()
            .code,
        "RFQ_AWARD_MISMATCH"
    );
    order.lines[0].tax_bps = 700;
    assert_eq!(
        assert_rfq_order(&award, &order, "order", false, "2026-12-01")
            .unwrap_err()
            .code,
        "RFQ_QUOTE_EXPIRED"
    );
    award.status = "ordered".into();
    order.request_id = None;
    assert!(assert_rfq_order(&award, &order, "order", true, "2027-01-01").is_ok());
    assert_eq!(
        assert_rfq_order(&award, &order, "different", true, "2027-01-01")
            .unwrap_err()
            .code,
        "RFQ_AWARD_REQUIRED"
    );
}

#[test]
fn award_comparison_matches_typescript_source() {
    let script = r#"
const {assertRfqOrder}=require('./packages/server-procurement/src/domain/rfqs.ts');
const detail={rfq:{status:'approved',request_id:'request',selected_offer_id:'offer',order_id:'order',warehouse_id:'warehouse',needed_date:'2026-12-01'},lines:[{id:'line',product_id:'product',quantity_milli:1250}],offers:[{id:'offer',supplier_id:'supplier',valid_until:'2026-11-30'}],offerLines:[{offer_id:'offer',rfq_line_id:'line',price_minor:1234,tax_bps:700}]};
const base={kind:'purchase',requestId:'request',partyId:'supplier',warehouseId:'warehouse',dueDate:'2026-12-01',orderDate:'2026-11-29',lines:[{productId:'product',quantity:'1.250',price:'12.34',taxBps:700}]};
const cases=[[{},false,'order','2026-11-30'],[{kind:'sale',requestId:undefined},false,'order','2026-12-01'],[{requestId:undefined},false,'order','2026-12-01'],[{lines:[{...base.lines[0],taxBps:701}]},false,'order','2026-12-01'],[{},false,'order','2026-12-01'],[{requestId:undefined},true,'order','2027-01-01'],[{},true,'different','2027-01-01']];
console.log(JSON.stringify(cases.map(([change,editing,id,today])=>{try{return assertRfqOrder(detail,{...base,...change},id,editing,today).id}catch(e){return e.code}})));
"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server", "--import=tsx", "-e", script])
            .output()
            .unwrap();
    assert!(output.status.success());
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let (award, base) = fixture();
    let mut results = vec![
        assert_rfq_order(&award, &base, "order", false, "2026-11-30")
            .unwrap()
            .id
            .clone(),
    ];
    let mut input = base.clone();
    input.kind = "sale".into();
    input.request_id = None;
    results.push(
        assert_rfq_order(&award, &input, "order", false, "2026-12-01")
            .unwrap_err()
            .code
            .into(),
    );
    input.kind = "purchase".into();
    results.push(
        assert_rfq_order(&award, &input, "order", false, "2026-12-01")
            .unwrap_err()
            .code
            .into(),
    );
    input.request_id = Some("request".into());
    input.lines[0].tax_bps = 701;
    results.push(
        assert_rfq_order(&award, &input, "order", false, "2026-12-01")
            .unwrap_err()
            .code
            .into(),
    );
    results.push(
        assert_rfq_order(&award, &base, "order", false, "2026-12-01")
            .unwrap_err()
            .code
            .into(),
    );
    input = base.clone();
    input.request_id = None;
    results.push(
        assert_rfq_order(&award, &input, "order", true, "2027-01-01")
            .unwrap()
            .id
            .clone(),
    );
    results.push(
        assert_rfq_order(&award, &base, "different", true, "2027-01-01")
            .unwrap_err()
            .code
            .into(),
    );
    assert_eq!(results, expected);
}

#[test]
fn transition_order_matches_typescript_application() {
    let script = r#"
const {createRfqApplication}=require('./packages/server-procurement/src/application/rfqs.ts');
const cases=[['submit','collecting',false,true,false,false],['submit','collecting',false,true,false,true],['submit','approved',true,true,false,false],['approve','review',false,true,false,false],['approve','review',false,false,false,false],['approve','collecting',false,true,true,false],['return','collecting',false,true,false,false],['cancel','approved',true,true,false,false],['cancel','approved',false,true,false,false],['cancel','ordered',false,true,true,false],['approve','review',false,true,true,false]];
(async()=>{const out=[];for(const [action,status,blocked,canApprove,deny,expired] of cases){let calls=0;const rfq={id:'rfq',status,request_id:blocked?'request':null,selected_offer_id:'offer',created_by:'creator',submitted_by:'submitter'};const offer={id:'offer',supplier_id:'supplier',recorded_by:'recorder',valid_until:expired?'2026-01-01':'2026-12-01'};const ports={clock:{now:()=>Date.parse('2026-11-15T00:00:00Z')},canPurchase:true,canApprove,canRead:true,samePerson:()=>false,authorizeRead:async()=>{},authorizePurchase:async()=>{},authorizeApprove:async()=>{calls++;if(deny){const e=new Error();e.code='DENIED';throw e}},replay:async()=>null,rfq:async()=>rfq,detail:async()=>({lines:[],offers:[offer],offerLines:[],events:[]}),sourceState:async()=>null,currentQuotationSource:async()=>null,commit:async(m)=>({status:m.status,selected:m.offerId})};try{const result=await createRfqApplication(ports).act('rfq',{action,expectedVersion:1,reason:'reason',offerId:'offer'},'key');out.push({status:result.status,selected:result.selected,calls})}catch(e){out.push({error:e.code,calls})}}console.log(JSON.stringify(out))})().catch(e=>{console.error(e);process.exit(1)});
"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server", "--import=tsx", "-e", script])
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    use RfqTransitionAction::*;
    let cases = [
        (Submit, "collecting", false, true, false, false),
        (Submit, "collecting", false, true, false, true),
        (Submit, "approved", true, true, false, false),
        (Approve, "review", false, true, false, false),
        (Approve, "review", false, false, false, false),
        (Approve, "collecting", false, true, true, false),
        (Return, "collecting", false, true, false, false),
        (Cancel, "approved", true, true, false, false),
        (Cancel, "approved", false, true, false, false),
        (Cancel, "ordered", false, true, true, false),
        (Approve, "review", false, true, true, false),
    ];
    let mut actual = Vec::new();
    for (action, status, blocked, can_approve, deny, expired) in cases {
        let facts = RfqTransitionFacts {
            status: status.into(),
            source_blocked: blocked,
            selected_offer_id: Some("offer".into()),
            current_offers: vec![RfqCurrentOffer {
                id: "offer".into(),
                valid_until: if expired { "2026-01-01" } else { "2026-12-01" }.into(),
            }],
            can_approve,
            creator_is_actor: false,
        };
        let mut calls = 0;
        let result = decide_rfq_transition(&facts, action, Some("offer"), "2026-11-15", || {
            calls += 1;
            if deny {
                Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "DENIED",
                    "denied",
                ))
            } else {
                Ok(())
            }
        });
        actual.push(match result {Ok(decision)=>serde_json::json!({"status":decision.status,"selected":decision.selected_offer_id,"calls":calls}),Err(error)=>serde_json::json!({"error":error.code,"calls":calls})});
    }
    assert_eq!(serde_json::json!(actual), expected);
}

#[test]
fn normalized_transition_matches_actual_zod_property_order_and_boundaries() {
    let cases=vec![r#"{"reason":"  reason  ","extra":true,"offerId":"id","expectedVersion":3,"action":"submit"}"#.to_owned(),r#"{"action":"cancel","expectedVersion":1,"reason":"\ufeff😀ab\u00a0"}"#.into(),r#"{"action":"return","expectedVersion":9007199254740991,"offerId":"\ud800","reason":"abc"}"#.into(),r#"{"action":"offer","expectedVersion":1,"reason":"abc"}"#.into(),r#"{"action":"submit","expectedVersion":0,"reason":"abc"}"#.into(),r#"{"action":"submit","expectedVersion":1.5,"reason":"abc"}"#.into(),r#"{"action":"submit","expectedVersion":9007199254740992,"reason":"abc"}"#.into(),r#"{"action":"submit","expectedVersion":1,"offerId":"","reason":"abc"}"#.into(),r#"{"action":"submit","expectedVersion":1,"reason":"😀a"}"#.into(),format!("{{\"action\":\"submit\",\"expectedVersion\":1,\"offerId\":\"{}\",\"reason\":\"abc\"}}","x".repeat(2*1024*1024+20))];
    let script = r#"const fs=require('node:fs');const {rfqActionSchema}=require('./packages/server-procurement/src/domain/rfqs.ts');const cases=JSON.parse(fs.readFileSync(0,'utf8'));process.stdout.write(JSON.stringify(cases.map(raw=>{try{return JSON.stringify(rfqActionSchema.parse(JSON.parse(raw)))}catch{return null}})));"#;
    use std::io::Write;
    let mut child =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server", "--import=tsx", "-e", script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::to_string(&cases).unwrap()
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let expected: Vec<Option<String>> = serde_json::from_slice(&output.stdout).unwrap();
    let actual: Vec<Option<String>> = cases
        .iter()
        .map(|raw| {
            normalize_rfq_transition(&SourceOrderedJson::parse(raw).unwrap())
                .ok()
                .map(|v| v.json().stringify())
        })
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn request_status_matches_typescript_short_circuit_and_json_semantics() {
    let script = r#"const {sourceBlocked}=require('./packages/server-procurement/src/domain/rfqs.ts');const base={request_id:'request',status:'review',request_evidence_version:7,request_items_json:'[]'},s={state:'in_progress',evidence_version:7,items_json:'[]',approval_json:'{\"decision\":\"approved\"}'};const cases=[[{},s],[{status:'ordered'},{...s,approval_json:'invalid'}],[{request_id:''},null],[{},null],[{},{...s,state:'draft',approval_json:'invalid'}],[{},{...s,evidence_version:8,approval_json:'invalid'}],[{},{...s,items_json:'[ ]',approval_json:'invalid'}],[{},{...s,approval_json:'{\"decision\":\"denied\",\"decision\":\"approved\"}'}],[{},{...s,approval_json:'{\"x\":\"\\ud800\",\"decision\":\"approved\"}'}],[{},{...s,approval_json:'null'}],[{},{...s,approval_json:'invalid'}],[{request_evidence_version:9007199254740992},{...s,evidence_version:9007199254740993}]];console.log(JSON.stringify(cases.map(([r,v])=>{try{return sourceBlocked({...base,...r},v)}catch{return 'error'}})));"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server", "--import=tsx", "-e", script])
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let base = RfqRequestFacts {
        kind: "purchase".into(),
        state: "in_progress".into(),
        evidence_version: 7,
        items_json: "[]".into(),
        approval_json: r#"{"decision":"approved"}"#.into(),
    };
    let result = |request: Option<&str>,
                  status: &str,
                  version: Option<i64>,
                  source: Option<&RfqRequestFacts>| match rfq_source_blocked(
        request,
        status,
        version,
        Some("[]"),
        source,
    ) {
        Ok(v) => serde_json::json!(v),
        Err(_) => serde_json::json!("error"),
    };
    let mut actual = vec![result(Some("request"), "review", Some(7), Some(&base))];
    let mut source = base.clone();
    source.approval_json = "invalid".into();
    actual.push(result(Some("request"), "ordered", Some(7), Some(&source)));
    actual.push(result(Some(""), "review", Some(7), None));
    actual.push(result(Some("request"), "review", Some(7), None));
    source.state = "draft".into();
    actual.push(result(Some("request"), "review", Some(7), Some(&source)));
    source.state = "in_progress".into();
    source.evidence_version = 8;
    actual.push(result(Some("request"), "review", Some(7), Some(&source)));
    source.evidence_version = 7;
    source.items_json = "[ ]".into();
    actual.push(result(Some("request"), "review", Some(7), Some(&source)));
    source = base.clone();
    source.approval_json = r#"{"decision":"denied","decision":"approved"}"#.into();
    actual.push(result(Some("request"), "review", Some(7), Some(&source)));
    source.approval_json = r#"{"x":"\ud800","decision":"approved"}"#.into();
    actual.push(result(Some("request"), "review", Some(7), Some(&source)));
    source.approval_json = "null".into();
    actual.push(result(Some("request"), "review", Some(7), Some(&source)));
    source.approval_json = "invalid".into();
    actual.push(result(Some("request"), "review", Some(7), Some(&source)));
    source = base.clone();
    source.evidence_version = 9_007_199_254_740_993;
    actual.push(result(
        Some("request"),
        "review",
        Some(9_007_199_254_740_992),
        Some(&source),
    ));
    assert_eq!(serde_json::json!(actual), expected);
}
