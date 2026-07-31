//! G9 product-context live acceptance queue.
//!
//! This packet deliberately does not connect to a database or promote source
//! markers to runtime evidence.  It makes the four-context acceptance input
//! contract explicit and fail-closed so a partial Sales/Procurement/
//! Inventory/Logistics setup cannot be mistaken for a complete run.

use std::{env, process::Command};

const ERPX_MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
const RUN_LOCAL_SUITES_ENV: &str = "ERPX_G9_RUN_LOCAL_SUITES";

const CONTEXT_DATABASE_REFERENCES: [(&str, &str); 8] = [
    ("Sales write", "ERPX_TEST_G9_SALES_WRITE_DATABASE_URL"),
    ("Sales read", "ERPX_TEST_G9_SALES_READ_DATABASE_URL"),
    (
        "Procurement write",
        "ERPX_TEST_G9_PROCUREMENT_WRITE_DATABASE_URL",
    ),
    (
        "Procurement read",
        "ERPX_TEST_G9_PROCUREMENT_READ_DATABASE_URL",
    ),
    (
        "Inventory write",
        "ERPX_TEST_G9_INVENTORY_WRITE_DATABASE_URL",
    ),
    ("Inventory read", "ERPX_TEST_G9_INVENTORY_READ_DATABASE_URL"),
    (
        "Logistics write",
        "ERPX_TEST_G9_LOGISTICS_WRITE_DATABASE_URL",
    ),
    ("Logistics read", "ERPX_TEST_G9_LOGISTICS_READ_DATABASE_URL"),
];

fn configured_database_references() -> Vec<(&'static str, &'static str)> {
    CONTEXT_DATABASE_REFERENCES
        .iter()
        .copied()
        .filter(|(_, variable)| {
            env::var(variable)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
        .collect()
}

fn database_url(variable: &str) -> Option<String> {
    env::var(variable)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn run_infrastructure_acceptance(
    context: &str,
    test_target: &str,
    test_name: &str,
    write_url: &str,
    read_url: &str,
) -> Result<(), String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .args([
            "test",
            "--offline",
            "--manifest-path",
            ERPX_MANIFEST,
            "-p",
            "erpx-infrastructure",
            "--test",
            test_target,
            "--",
            "--ignored",
            "--exact",
            test_name,
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CARGO_TERM_COLOR", "never")
        .env("ERPX_TEST_TENANT_WRITE_DATABASE_URL", write_url)
        .env("ERPX_TEST_TENANT_READ_DATABASE_URL", read_url)
        .output()
        .map_err(|error| {
            format!(
                "run G9 {context} authoritative SQLx acceptance {test_target}::{test_name}: {error}"
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "G9 {context} authoritative SQLx acceptance {test_target}::{test_name} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    eprintln!(
        "G9 {context} live SQLx acceptance: {test_target}::{test_name} passed against configured disposable write/read databases"
    );
    Ok(())
}

#[test]
fn g9_context_live_acceptance_is_complete_or_explicitly_queued() {
    let configured = configured_database_references();
    let missing = CONTEXT_DATABASE_REFERENCES
        .iter()
        .copied()
        .filter(|(_, variable)| {
            !configured
                .iter()
                .any(|(_, configured_variable)| configured_variable == variable)
        })
        .map(|(context, variable)| format!("{context} ({variable})"))
        .collect::<Vec<_>>();

    assert!(
        configured.is_empty() || missing.is_empty(),
        "G9 live acceptance must fail closed on partial Sales/Procurement/Inventory/Logistics database configuration: configured={}/{}; missing {}",
        configured.len(),
        CONTEXT_DATABASE_REFERENCES.len(),
        missing.join(", ")
    );

    if configured.is_empty() {
        eprintln!(
            "G9 live SQLx blocker queued: missing disposable write/read database references for Sales, Procurement, Inventory, and Logistics"
        );
    } else {
        eprintln!(
            "G9 live SQLx certification remains queued: all four context write/read references are configured, but this acceptance guard performs no database connection and claims no outbox, read-model, reconciliation, rollback, or cutover evidence"
        );
    }
}

#[test]
fn g9_local_product_context_sqlx_slices_execute_when_explicitly_enabled() {
    if env::var(RUN_LOCAL_SUITES_ENV).ok().as_deref() != Some("1") {
        eprintln!(
            "G9 local product SQLx suites queued: set {RUN_LOCAL_SUITES_ENV}=1 and configure all eight context-specific disposable write/read database references"
        );
        return;
    }

    let required = CONTEXT_DATABASE_REFERENCES
        .iter()
        .map(|(_, variable)| *variable)
        .collect::<Vec<_>>();
    let missing = required
        .iter()
        .copied()
        .filter(|variable| database_url(variable).is_none())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        eprintln!(
            "G9 local product SQLx blocker queued: missing disposable database references: {}",
            missing.join(", ")
        );
        return;
    }

    let mut failures = Vec::new();
    let sales_write = database_url("ERPX_TEST_G9_SALES_WRITE_DATABASE_URL").unwrap();
    let sales_read = database_url("ERPX_TEST_G9_SALES_READ_DATABASE_URL").unwrap();
    if let Err(error) = run_infrastructure_acceptance(
        "Sales",
        "sales_persistence",
        "live_sales_write_outbox_projection_scoped_read_reconciliation_and_rebuild",
        &sales_write,
        &sales_read,
    ) {
        failures.push(error);
    }

    for (context, write_variable, read_variable) in [
        (
            "Procurement",
            "ERPX_TEST_G9_PROCUREMENT_WRITE_DATABASE_URL",
            "ERPX_TEST_G9_PROCUREMENT_READ_DATABASE_URL",
        ),
        (
            "Inventory",
            "ERPX_TEST_G9_INVENTORY_WRITE_DATABASE_URL",
            "ERPX_TEST_G9_INVENTORY_READ_DATABASE_URL",
        ),
        (
            "Logistics",
            "ERPX_TEST_G9_LOGISTICS_WRITE_DATABASE_URL",
            "ERPX_TEST_G9_LOGISTICS_READ_DATABASE_URL",
        ),
    ] {
        let write = database_url(write_variable).unwrap();
        let read = database_url(read_variable).unwrap();
        if let Err(error) = run_infrastructure_acceptance(
            context,
            "scm_persistence",
            "live_scm_write_outbox_projection_scoped_read_and_reconciliation",
            &write,
            &read,
        ) {
            failures.push(error);
        }
    }
    assert!(
        failures.is_empty(),
        "G9 local product SQLx acceptance failures:\n{}",
        failures.join("\n\n")
    );
}
