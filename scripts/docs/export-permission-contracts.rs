fn main() {
    let catalog = erpx_server_identity_access::public::permission_catalog::permission_catalog()
        .expect("validated canonical permission catalog");
    println!(
        "{}",
        serde_json::to_string_pretty(catalog.definitions()).expect("permission metadata JSON")
    );
}
