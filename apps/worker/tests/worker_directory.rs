//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "directory_vendor_live.rs"]
mod directory_vendor_live;
#[path = "generic_ldap_live.rs"]
mod generic_ldap_live;
#[path = "item_directory_lineage_acceptance.rs"]
mod item_directory_lineage_acceptance;
