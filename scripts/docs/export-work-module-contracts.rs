fn main() {
    let catalog = erpx_server_identity_access::public::work_admission::work_module_catalog()
        .expect("validated canonical module catalog");
    println!(
        "{}",
        serde_json::to_string_pretty(catalog.definitions()).expect("module metadata JSON")
    );
}
