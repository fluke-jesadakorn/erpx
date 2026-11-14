//! Owner-local tests for the production PostgreSQL collaboration units that do
//! not require a database: scope facts, stable keys, cursors and port shapes.
use erpx_server_collaboration::public::inbox::{
    InboxCursor, InboxScope, InboxTargetPort, PgInboxUnit,
};
use erpx_server_collaboration::public::messaging_native::{
    MessagingScope, PgMessagingUnit, direct_key_input,
};
use erpx_server_collaboration::public::messaging_teams::PgMessagingTeamUnit;
use erpx_server_collaboration::public::social::{
    PgSocialUnit, SocialRecordPort, SocialScope, SocialUnit, UnavailableSocialRecords,
};

fn assert_social_unit<T: SocialUnit>() {}
fn assert_social_records<T: SocialRecordPort + ?Sized>() {}
fn assert_inbox_targets<T: InboxTargetPort + ?Sized>() {}

#[test]
fn production_units_implement_the_owner_ports() {
    assert_social_unit::<PgSocialUnit<'static>>();
    assert_social_records::<UnavailableSocialRecords>();
    assert_social_records::<dyn SocialRecordPort>();
    assert_inbox_targets::<dyn InboxTargetPort>();
    let _ = std::mem::size_of::<PgMessagingUnit<'static>>();
    let _ = std::mem::size_of::<PgMessagingTeamUnit<'static>>();
    let _ = std::mem::size_of::<PgInboxUnit<'static>>();
}

#[test]
fn social_scope_maps_the_current_branch_and_role_flags() {
    let scope = SocialScope::new("company", "user", "assignment", "  ", 1_700_000_000_000)
        .unwrap()
        .branch(Some("branch"))
        .unwrap()
        .department(Some("department"))
        .unwrap()
        .assignment_version(4)
        .share_company(true)
        .moderate(true);
    let actor = scope.actor();
    assert_eq!(actor.display_name, "user");
    assert_eq!(actor.branch_id.as_deref(), Some("branch"));
    assert_eq!(actor.department_id.as_deref(), Some("department"));
    assert!(actor.can_share_company);
    assert!(actor.can_moderate);
    assert_eq!(scope.assignment_version, 4);
    assert!(SocialScope::new("company", "", "assignment", "User", 1).is_err());
    assert!(SocialScope::new("", "user", "assignment", "User", 1).is_err());
}

#[test]
fn messaging_scope_and_direct_key_are_stable() {
    let scope = MessagingScope::new("company", "user", "assignment", "User", 1)
        .unwrap()
        .assignment_version(3);
    assert_eq!(scope.actor().user_id, "user");
    assert_eq!(scope.assignment_version, 3);
    assert_eq!(
        direct_key_input("company", &["a".into(), "b".into()]),
        "company:[\"a\",\"b\"]"
    );
    assert!(MessagingScope::new("company", "", "assignment", "User", 1).is_err());
}

#[test]
fn inbox_cursor_is_opaque_bounded_and_round_trips() {
    let cursor = InboxCursor {
        at: 1_700_000_000_000,
        id: "delivery-1".into(),
    };
    let encoded = cursor.encode();
    assert!(!encoded.contains(':'));
    assert_eq!(InboxCursor::decode(&encoded).unwrap(), cursor);
    for invalid in ["", "not base64!", "e30", "W10", "eyJhdCI6MX0"] {
        assert_eq!(
            InboxCursor::decode(invalid).unwrap_err().code,
            "INVALID_CURSOR"
        );
    }
    assert!(InboxScope::new("company", "user", "assignment", 1).is_ok());
    assert!(InboxScope::new("company", "", "assignment", 1).is_err());
}
