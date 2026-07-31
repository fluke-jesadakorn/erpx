//! Deterministic directory feed capacity and revocation-window contracts.
//!
//! These tests exercise only the identity-owned feed seam.  They do not
//! contact or certify LDAP, AD DS, or UCS.

use chrono::{Duration, TimeZone, Utc};
use erpx_identity::directory::feeds::{
    ChangeFeedRequest, FeedCursor, FeedError, FullImportPageRequest, ImportCapacity,
    MAX_FULL_IMPORT_USERS, REVOCATION_WINDOW_SECONDS, RevocationWindow,
};
use erpx_identity::directory::sync::ChangeKind;
use uuid::Uuid;

const CONNECTOR_ID: Uuid = Uuid::from_u128(0xfeed_0001);
const IMPORT_ID: Uuid = Uuid::from_u128(0xfeed_0002);

fn fixed_at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 27, 12, 0, 0)
        .single()
        .expect("fixed contract timestamp")
}

#[test]
fn full_import_capacity_accepts_exactly_one_million_users_without_buffering_records() {
    let mut capacity = ImportCapacity::full_import();
    assert_eq!(capacity.maximum_users(), 1_000_000);
    assert_eq!(capacity.maximum_users(), MAX_FULL_IMPORT_USERS);

    for page in 0..200 {
        assert_eq!(
            capacity
                .accept_page(5_000)
                .expect("page remains within bound"),
            MAX_FULL_IMPORT_USERS - ((page + 1) * 5_000) as u64
        );
    }

    assert_eq!(capacity.users_seen(), MAX_FULL_IMPORT_USERS);
    assert_eq!(capacity.remaining_users(), 0);
    assert!(matches!(
        capacity.accept_page(1),
        Err(FeedError::ImportCapacityExceeded {
            requested: 1_000_001,
            maximum: 1_000_000
        })
    ));
}

#[test]
fn full_import_requests_and_pages_fail_closed_at_capacity_boundaries() {
    assert!(ImportCapacity::new(MAX_FULL_IMPORT_USERS).is_ok());
    assert!(matches!(
        ImportCapacity::new(MAX_FULL_IMPORT_USERS + 1),
        Err(FeedError::ImportCapacityExceeded {
            requested: 1_000_001,
            maximum: 1_000_000
        })
    ));

    let exact = FullImportPageRequest::new(
        CONNECTOR_ID,
        IMPORT_ID,
        FeedCursor::Start,
        5_000,
        MAX_FULL_IMPORT_USERS,
    )
    .expect("the one-million-user ceiling is inclusive");
    exact
        .validate_page_len(5_000)
        .expect("maximum configured page is accepted");
    assert!(matches!(
        exact.validate_page_len(5_001),
        Err(FeedError::PageTooLarge)
    ));

    assert!(matches!(
        FullImportPageRequest::new(
            CONNECTOR_ID,
            IMPORT_ID,
            FeedCursor::Start,
            5_000,
            MAX_FULL_IMPORT_USERS + 1,
        ),
        Err(FeedError::ImportCapacityExceeded {
            requested: 1_000_001,
            maximum: 1_000_000
        })
    ));
    assert!(matches!(
        FullImportPageRequest::new(CONNECTOR_ID, IMPORT_ID, FeedCursor::Start, 5_000, 0,),
        Err(FeedError::ImportCapacityExceeded {
            requested: 0,
            maximum: 1_000_000
        })
    ));

    let two_remaining =
        FullImportPageRequest::new(CONNECTOR_ID, IMPORT_ID, FeedCursor::Start, 5_000, 2)
            .expect("remaining capacity is positive");
    assert!(matches!(
        two_remaining.validate_page_len(3),
        Err(FeedError::PageTooLarge)
    ));

    assert!(ChangeFeedRequest::new(CONNECTOR_ID, FeedCursor::Start, 5_000).is_ok());
}

#[test]
fn disable_delete_revocation_is_due_at_five_minutes_and_expires_after_boundary() {
    assert_eq!(REVOCATION_WINDOW_SECONDS, 5 * 60);
    let observed_at = fixed_at();
    let deadline = observed_at + Duration::minutes(5);

    for kind in [ChangeKind::Disable, ChangeKind::Delete] {
        let window = RevocationWindow::for_change(kind, observed_at)
            .expect("revoking changes have a valid schedule")
            .expect("disable/delete changes schedule revocation");
        assert_eq!(window.observed_at(), observed_at);
        assert_eq!(window.deadline(), deadline);
        assert!(!window.is_due_at(observed_at - Duration::nanoseconds(1)));
        assert!(window.is_due_at(deadline));
        assert!(!window.is_expired_at(deadline));
        assert!(window.is_expired_at(deadline + Duration::nanoseconds(1)));
    }

    assert_eq!(
        RevocationWindow::for_change(ChangeKind::Upsert, observed_at)
            .expect("upsert does not create a revocation schedule"),
        None
    );
}
