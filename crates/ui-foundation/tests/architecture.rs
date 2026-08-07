const MANIFEST: &str = include_str!("../Cargo.toml");
const SOURCE: &str = concat!(
    include_str!("../src/lib.rs"),
    include_str!("../src/channel.rs"),
    include_str!("../src/error.rs"),
    include_str!("../src/fake.rs"),
    include_str!("../src/flow.rs"),
    include_str!("../src/gateway.rs"),
    include_str!("../src/presenter.rs"),
    include_str!("../src/session.rs"),
);

#[test]
fn dependencies_stay_transport_domain_and_persistence_neutral() {
    let dependency_names: Vec<_> = MANIFEST
        .lines()
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim().starts_with('['))
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.trim()))
        .map(|name| name.strip_suffix(".workspace").unwrap_or(name))
        .collect();
    assert_eq!(
        dependency_names,
        [
            "async-trait",
            "erpx-cqrs",
            "erpx-kernel",
            "serde",
            "thiserror",
        ]
    );
}

#[test]
fn source_has_no_transport_ui_framework_or_domain_aggregate_imports() {
    for forbidden in [
        "sqlx::",
        "tonic::",
        "prost::",
        "dioxus::",
        "erpx_domain",
        "cfg(target_arch",
    ] {
        assert!(
            !SOURCE.contains(forbidden),
            "UI foundation contains forbidden dependency token {forbidden}"
        );
    }
}
