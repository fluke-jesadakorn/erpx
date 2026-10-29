use erpx_server_files::public::count_submission::*;
fn set(target: &str, purpose: &str) -> CountBindingSet {
    CountBindingSet {
        id: "set".into(),
        target_type: target.into(),
        target_id: "count".into(),
        purpose: purpose.into(),
        component: "action".into(),
        component_ref: Some("submit".into()),
        state: "draft".into(),
        expected_version: 1,
        policy_version: 1,
        required_count: 0,
        compatibility_created: false,
        links: vec![CountBindingLink {
            id: "link".into(),
            state: "draft".into(),
            source_id: Some("source".into()),
            source_state: Some("available".into()),
            checksum: Some("checksum".into()),
            media_type: Some("image/png".into()),
            bytes: Some(3),
        }],
    }
}
fn features() -> CountBindingFeatures {
    CountBindingFeatures {
        enabled: true,
        read_only: false,
        uploads_frozen: false,
    }
}
#[test]
fn aliases_contexts_gate_and_order_are_exact() {
    assert_eq!(
        unique_set_ids(&["b".into(), "a".into(), "b".into()]),
        vec!["b", "a"]
    );
    assert!(
        prepare_count_binding(
            &[set("stock_count", "count_photo")],
            "stock_count",
            "count",
            1,
            &features()
        )
        .is_ok()
    );
    let invalid = prepare_count_binding(
        &[set("inventory_stock_count", "count_photo")],
        "inventory_stock_count",
        "count",
        1,
        &features(),
    )
    .unwrap_err();
    assert_eq!(invalid.code, "ATTACHMENT_BINDING_CONTEXT");
    assert!(
        prepare_count_binding(
            &[set("inventory_stock_count", "count_evidence")],
            "inventory_stock_count",
            "count",
            1,
            &features()
        )
        .is_ok()
    );
    let mut disabled = features();
    disabled.enabled = false;
    assert_eq!(
        prepare_count_binding(
            &[set("stock_count", "count_sheet")],
            "stock_count",
            "count",
            1,
            &disabled
        )
        .unwrap_err()
        .code,
        "ATTACHMENTS_DISABLED"
    );
    let mut legacy = set("stock_count", "count_sheet");
    legacy.compatibility_created = true;
    assert!(prepare_count_binding(&[legacy.clone()], "stock_count", "count", 1, &disabled).is_ok());
    disabled.read_only = true;
    assert_eq!(
        prepare_count_binding(&[legacy], "stock_count", "count", 1, &disabled)
            .unwrap_err()
            .code,
        "ATTACHMENTS_READ_ONLY"
    );
    let mut frozen = features();
    frozen.uploads_frozen = true;
    assert_eq!(
        prepare_count_binding(
            &[set("stock_count", "count_sheet")],
            "stock_count",
            "count",
            1,
            &frozen
        )
        .unwrap_err()
        .code,
        "ATTACHMENT_UPLOAD_FREEZE"
    );
}
#[test]
fn requires_exact_version_and_available_nonempty_sources() {
    let mut s = set("stock_count", "count_sheet");
    s.expected_version = 2;
    assert_eq!(
        prepare_count_binding(&[s], "stock_count", "count", 1, &features())
            .unwrap_err()
            .code,
        "ATTACHMENT_SET_STALE"
    );
    let mut s = set("stock_count", "count_sheet");
    s.links[0].source_state = Some("scanning".into());
    assert_eq!(
        prepare_count_binding(&[s], "stock_count", "count", 1, &features())
            .unwrap_err()
            .code,
        "ATTACHMENT_SCAN_PENDING"
    );
    let mut s = set("stock_count", "count_sheet");
    s.links.clear();
    assert_eq!(
        prepare_count_binding(&[s], "stock_count", "count", 1, &features())
            .unwrap_err()
            .code,
        "ATTACHMENT_SET_EMPTY"
    );
}
