use erpx_workflow_server::public::source_ordered_json::SourceOrderedJson;
use std::io::Write;
#[test]
fn borrowed_array_and_truthiness_keep_js_json_values() {
    let value = SourceOrderedJson::parse(r#"[null,false,0,-0,"",[],{},"\ud800",1e400]"#).unwrap();
    let root = value.root();
    assert_eq!(root.array_len(), Some(9));
    let actual: Vec<bool> = (0..9).map(|i| root.element(i).unwrap().truthy()).collect();
    assert_eq!(
        actual,
        [false, false, false, false, false, true, true, true, true]
    );
    assert_eq!(root.element(7).unwrap().utf16(), Some([0xd800].as_slice()));
    assert!(root.element(9).is_none());
    assert_eq!(root.element(6).unwrap().array_len(), None);
}
#[test]
fn actual_node_codec_matches_large_deep_and_ecmascript_cases() {
    let cases = vec![
        r#"{"z":1,"2":"x","1":"y","z":3,"01":4}"#.to_owned(),
        r#"["\ud800","\udc00","\ud83d\ude00",-0,1e400,9007199254740993]"#.into(),
        format!("\"{}\"", "x".repeat(2 * 1024 * 1024 + 1)),
        format!("{}0{}", "[".repeat(200), "]".repeat(200)),
        r#"{"4294967295":1,"4294967294":2,"0":3,"00":4}"#.into(),
    ];
    let node = std::env::var("ERPX_NODE_BIN").expect("pinned Node required");
    let mut child=std::process::Command::new(node).args(["--input-type=module","-e","import {readFileSync} from 'node:fs';import {createHash} from 'node:crypto';const cases=JSON.parse(readFileSync(0,'utf8'));process.stdout.write(JSON.stringify(cases.map(x=>{const s=JSON.stringify(JSON.parse(x));return [s,createHash('sha256').update(s).digest('hex')]}))); "]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
    write!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::to_string(&cases).unwrap()
    )
    .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let expected: Vec<(String, String)> = serde_json::from_slice(&out.stdout).unwrap();
    for (raw, (json, hash)) in cases.iter().zip(expected) {
        let actual = SourceOrderedJson::parse(raw).unwrap();
        assert_eq!(actual.stringify(), json);
        assert_eq!(actual.sha256(), hash);
    }
    for bad in [
        "",
        "[1,]",
        "{\"a\":1,}",
        "01",
        "[",
        "true false",
        r#""\uZZZZ""#,
    ] {
        assert!(SourceOrderedJson::parse(bad).is_err(), "{bad}");
    }
}
#[test]
fn frozen_node_conformance_corpus_matches() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/workflow/rust-commands/conformance.v1.json"
    ))
    .unwrap();
    for row in corpus["json"].as_array().unwrap() {
        let actual = SourceOrderedJson::parse(row["inputJson"].as_str().unwrap()).unwrap();
        assert_eq!(actual.stringify(), row["expectedJson"].as_str().unwrap());
    }
}
#[path = "../../support/workflow/command-actor.rs"]
mod actor_fixture;
#[test]
fn frozen_actor_fingerprints_match_legacy_node() {
    use erpx_workflow_server::public::source_ordered_json::source_ordered_command_fingerprint;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/workflow/rust-commands/conformance.v1.json"
    ))
    .unwrap();
    for row in corpus["fingerprints"].as_array().unwrap() {
        let actor = actor_fixture::parse_frozen_actor(row["actorJson"].as_str().unwrap());
        let input = SourceOrderedJson::parse(row["inputJson"].as_str().unwrap()).unwrap();
        let hash = source_ordered_command_fingerprint(
            &actor,
            row["operation"].as_str().unwrap(),
            row["recordId"].as_str().unwrap(),
            &input,
        )
        .unwrap();
        assert_eq!(hash, row["expectedHash"].as_str().unwrap());
    }
}
#[test]
fn large_surrogate_fingerprint_matches_actual_node() {
    use erpx_workflow_server::public::source_ordered_json::source_ordered_command_fingerprint;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/workflow/rust-commands/conformance.v1.json"
    ))
    .unwrap();
    let row = &corpus["fingerprints"][0];
    let actor = actor_fixture::parse_frozen_actor(row["actorJson"].as_str().unwrap());
    let raw = format!(
        "{{\"z\":\"{}\",\"1\":\"\\ud800\",\"1\":\"\\udc00\"}}",
        "x".repeat(2 * 1024 * 1024 + 1)
    );
    let input = SourceOrderedJson::parse(&raw).unwrap();
    let request = serde_json::json!({"actor":row["actorJson"],"input":raw,"operation":row["operation"],"recordId":row["recordId"]});
    let node = std::env::var("ERPX_NODE_BIN").unwrap();
    let mut child=std::process::Command::new(node).args(["--input-type=module","-e",r#"import {readFileSync} from 'node:fs';import {createHash} from 'node:crypto';const x=JSON.parse(readFileSync(0,'utf8')),a=JSON.parse(x.actor);const {userId,assignmentId,role,authority,companyId,branchId,departmentId}=a;const v={actor:userId,assignment:assignmentId,role,...('authority' in a?{authority}:{}),company:companyId,...('branchId' in a?{branch:branchId}:{}),...('departmentId' in a?{department:departmentId}:{}),operation:x.operation,recordId:x.recordId,input:JSON.parse(x.input)};process.stdout.write(createHash('sha256').update(JSON.stringify(v)).digest('hex'));"#]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
    write!(child.stdin.take().unwrap(), "{request}").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let actual = source_ordered_command_fingerprint(
        &actor,
        row["operation"].as_str().unwrap(),
        row["recordId"].as_str().unwrap(),
        &input,
    )
    .unwrap();
    assert_eq!(actual, String::from_utf8(out.stdout).unwrap());
}
#[test]
fn validated_wrapper_keeps_existing_actor_restrictions() {
    use erpx_workflow_server::public::source_ordered_json::source_ordered_command_fingerprint;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/workflow/rust-commands/conformance.v1.json"
    ))
    .unwrap();
    let row = &corpus["fingerprints"][0];
    let mut actor = actor_fixture::parse_frozen_actor(row["actorJson"].as_str().unwrap());
    let input = SourceOrderedJson::parse("{}").unwrap();
    actor.role = "r".repeat(129);
    assert!(source_ordered_command_fingerprint(&actor, "operation", "record", &input).is_err());
}
