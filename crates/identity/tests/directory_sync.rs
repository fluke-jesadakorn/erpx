#![allow(dead_code)]

// Compile and execute the directory slice without changing identity crate
// registration. The coordinator owns publishing these modules through lib.rs.
use erpx_identity::{DirectoryRoleAction, DirectoryRoleMapping, DirectoryScope};

#[path = "../src/directory/feeds.rs"]
mod feeds;
#[path = "../src/directory/rbac.rs"]
mod rbac;
#[path = "../src/directory/sync.rs"]
mod sync;
