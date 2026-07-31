//! Focused directory-RBAC identity-linking contract tests.
//!
//! These tests are deliberately transport- and database-independent. They
//! prove that role identity keys follow the directory's immutable authority
//! and object value, never a mutable external ID or login attribute.

use erpx_identity::directory::rbac::DirectoryRoleIdentity;
use erpx_identity::directory::sync::{
    DirectoryUserRecord, ImmutableIdentityLink, immutable_identity_database_key,
};

#[test]
fn canonical_role_key_survives_external_id_and_login_rename() {
    let immutable =
        ImmutableIdentityLink::new("ad:objectGUID", "guid-001").expect("stable AD object key");
    let before = DirectoryUserRecord::builder("CN=Old Name,DC=example,DC=test", immutable.clone())
        .login_name("old.name@example.test")
        .email("old.name@example.test")
        .build()
        .expect("initial directory record");
    let after = DirectoryUserRecord::builder("CN=New Name,DC=example,DC=test", immutable.clone())
        .login_name("new.name@example.test")
        .email("new.name@example.test")
        .build()
        .expect("renamed directory record");

    let before_key = immutable_identity_database_key(before.immutable_link());
    let after_key = immutable_identity_database_key(after.immutable_link());
    assert_eq!(before_key, after_key);
    assert_eq!(
        before_key,
        DirectoryRoleIdentity::from_immutable_link(immutable).database_key()
    );
}

#[test]
fn changing_the_immutable_directory_object_key_creates_a_different_identity() {
    let first =
        ImmutableIdentityLink::new("ldap:entryUUID", "uuid-001").expect("stable LDAP entryUUID");
    let replacement =
        ImmutableIdentityLink::new("ldap:entryUUID", "uuid-002").expect("different LDAP entryUUID");

    assert_ne!(
        immutable_identity_database_key(&first),
        immutable_identity_database_key(&replacement)
    );
}
