use std::process::Command;
#[test]
fn compiled_composition_reports_pending_owners_without_creating_a_server() {
    let output = Command::new(env!("CARGO_BIN_EXE_erpx-server"))
        .arg("owners")
        .output()
        .unwrap();
    assert!(output.status.success());
    let state: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(state["inventory"]["backendMigrated"], false);
    assert_eq!(state["readiness"]["ready"], false);
    assert_eq!(state["inventory"]["owners"].as_array().unwrap().len(), 41);
}
#[test]
fn compiled_composition_refuses_reserved_installed_and_public_listeners_before_binding() {
    for address in [
        "127.0.0.1:3100",
        "127.0.0.1:3101",
        "127.0.0.1:3000",
        "127.0.0.1:8080",
        "0.0.0.0:3199",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_erpx-server"))
            .args(["api", "--listen", address])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
        assert!(output.stdout.is_empty());
    }
}
