//! Deterministic identity-owned certification evidence for directory volume,
//! native cursors, revocation, role history, and break-glass boundaries.
//!
//! These tests deliberately stop at the identity domain seams.  They do not
//! claim that a live LDAP, AD DS, or UCS server was contacted; vendor
//! certification remains an external acceptance step.

use std::{collections::BTreeSet, sync::OnceLock};

use chrono::{DateTime, Duration, Utc};
use erpx_identity::directory::auth::{
    BreakGlassPolicy, DirectoryAuthenticator, DirectoryConnectorConfig,
    DirectoryProvider as AuthProvider, LocalBreakGlassError, LocalBreakGlassGrant,
    LocalBreakGlassOrigin, LocalBreakGlassRequest, LocalBreakGlassVerifier, RedactedSecret,
};
use erpx_identity::directory::feeds::{FeedCursor, FeedKind};
use erpx_identity::directory::rbac::{
    DirectoryGroupRef, DirectoryManagedRoleMapping, DirectoryRbacRollbackPlan,
    DirectoryRoleFactAction, DirectoryRoleFactAppendOutcome, DirectoryRoleFactInput,
    DirectoryRoleFactLedger, DirectoryRoleFactReason, DirectoryRoleIdentity,
    DirectoryRoleMappingSet, DirectoryRoleRef,
};
use erpx_identity::directory::revocation::{
    DirectoryRevocationAppendOutcome, DirectoryRevocationFactInput, DirectoryRevocationLedger,
    DirectoryRevocationReadModel, DirectoryRevocationReason,
    DirectoryRevocationState as DurableRevocationState, RevocationGate,
};
use erpx_identity::directory::sync::{
    ConnectorScope, DirectoryPage, DirectoryUserRecord, FeedVendor, ImmutableIdentityLink,
    ImportCheckpoint, ImportConfig, ImportPlan, ImportState, SyncError, TransportSecurity,
};
use erpx_identity::directory::{
    DIRECTORY_REVOCATION_SLA, DirectoryChange as LegacyDirectoryChange, DirectoryError,
    DirectoryFeedKind, DirectoryProvider, DirectorySyncCursor, DirectorySyncPage, DirectoryUser,
};
use uuid::Uuid;

const TOTAL_USERS: usize = 1_000_000;
const PAGE_SIZE: usize = 5_000;
const IMPORT_PAGE_COUNT: usize = TOTAL_USERS / PAGE_SIZE;
const FIXED_CONNECTOR_ID: Uuid = Uuid::from_u128(0x1000);
const FIXED_IMPORT_ID: Uuid = Uuid::from_u128(0x2000);

fn fixed_at() -> DateTime<Utc> {
    static AT: OnceLock<DateTime<Utc>> = OnceLock::new();
    *AT.get_or_init(|| Utc::now() - Duration::hours(1))
}

fn import_connector() -> erpx_identity::directory::sync::DirectoryConnector {
    erpx_identity::directory::sync::DirectoryConnector::new(
        FIXED_CONNECTOR_ID,
        FeedVendor::GenericLdap,
        FeedKind::GenericLdapPolling,
        ConnectorScope::Platform,
        "ldaps://directory.example.test:636",
        TransportSecurity::Ldaps,
        PAGE_SIZE as u32,
        8,
    )
    .expect("bounded TLS-only connector")
}

fn import_record(index: usize) -> DirectoryUserRecord {
    let external_id = format!("user-{index:07}");
    DirectoryUserRecord::builder(
        external_id.clone(),
        ImmutableIdentityLink::new("ldap:entryUUID", format!("entry-{index:07}"))
            .expect("immutable link"),
    )
    .login_name(format!("user-{index:07}"))
    .email(format!("user-{index:07}@example.test"))
    .source_version(format!("modify-{index:07}"))
    .build()
    .expect("valid deterministic directory record")
}

fn import_page(
    page_number: usize,
    checkpoint: &ImportCheckpoint,
    _at: DateTime<Utc>,
) -> DirectoryPage {
    let first = page_number * PAGE_SIZE;
    let records = (first..first + PAGE_SIZE).map(import_record).collect();
    let terminal = page_number + 1 == IMPORT_PAGE_COUNT;
    let next_cursor = (!terminal).then_some(FeedCursor::LdapPaged(
        format!("ldap-cookie-{page_number:04}").into_bytes(),
    ));
    DirectoryPage::new(
        checkpoint.cursor().clone(),
        records,
        next_cursor,
        terminal,
        Some(format!("ldap-watermark-{page_number:04}")),
    )
    .expect("valid bounded import page")
}

#[test]
fn million_user_import_is_bounded_and_checkpoint_restart_replay_is_deterministic() {
    let config = ImportConfig::new(PAGE_SIZE as u32, 8).expect("bounded import config");
    assert_eq!(config.max_users(), TOTAL_USERS as u64);
    assert_eq!(config.page_size(), PAGE_SIZE as u32);
    assert!(config.max_in_flight_pages() <= 64);

    let plan = ImportPlan::with_id(
        FIXED_IMPORT_ID,
        import_connector(),
        config.clone(),
        fixed_at(),
    )
    .expect("one-million-user plan");
    assert!(ImportConfig::new(PAGE_SIZE as u32, 8).is_ok());

    let mut checkpoint = ImportCheckpoint::initial(&plan);
    let restart_after_page = 73;
    let mut replay_page = None;

    for page_number in 0..IMPORT_PAGE_COUNT {
        let page = import_page(page_number, &checkpoint, fixed_at());
        let page_key = page
            .idempotency_key(plan.import_id())
            .expect("deterministic page idempotency key");
        let next = checkpoint
            .accept_page(
                &page,
                &config,
                fixed_at() + Duration::seconds(page_number as i64),
            )
            .expect("checkpoint accepts exactly one bounded page");

        if page_number == restart_after_page {
            replay_page = Some((page, next.clone(), page_key));
            let encoded = serde_json::to_vec(&next).expect("checkpoint serialization");
            checkpoint = serde_json::from_slice(&encoded).expect("checkpoint restart replay");
            assert_eq!(checkpoint, next);
        } else {
            checkpoint = next;
        }
    }

    assert_eq!(checkpoint.users_seen(), TOTAL_USERS as u64);
    assert_eq!(checkpoint.pages_applied(), IMPORT_PAGE_COUNT as u64);
    assert_eq!(checkpoint.last_external_id(), Some("user-0999999"));
    assert_eq!(checkpoint.state(), &ImportState::Finalizing);
    assert_eq!(checkpoint.cursor(), &FeedCursor::EndOfSnapshot);

    let (previous_page, committed_checkpoint, page_key) =
        replay_page.expect("restart checkpoint was exercised");
    assert_eq!(
        previous_page
            .idempotency_key(plan.import_id())
            .expect("replay key"),
        page_key
    );
    assert!(matches!(
        committed_checkpoint.accept_page(&previous_page, &config, fixed_at()),
        Err(SyncError::StaleCursor)
    ));

    assert!(ImportConfig::new(0, 8).is_err());
}

fn state_page(
    connector_id: Uuid,
    provider_feed: DirectoryFeedKind,
    sequence: u64,
    external_id: String,
) -> DirectorySyncPage {
    let user = DirectoryUser {
        external_object_id: external_id.clone(),
        principal_ref: format!("principal-{sequence:04}"),
        email: Some(format!("principal-{sequence:04}@example.test")),
        display_name: Some(format!("Principal {sequence:04}")),
        groups: BTreeSet::from(["group:directory-users".to_owned()]),
        active: true,
        source_revision: sequence,
    };
    DirectorySyncPage {
        connector_id,
        cursor: DirectorySyncCursor {
            connector_id,
            feed: provider_feed,
            sequence,
            opaque_cookie: Some(format!("native-cookie-{sequence:04}")),
            observed_at: fixed_at() + Duration::seconds(sequence as i64),
        },
        changes: vec![LegacyDirectoryChange::Upsert(user)],
    }
}

#[test]
fn native_ldap_ad_and_ucs_cursors_are_monotonic_across_restart_and_replay() {
    let matrix = [
        (DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling),
        (DirectoryProvider::GenericLdap, DirectoryFeedKind::Rfc4533),
        (
            DirectoryProvider::ActiveDirectoryDomainServices,
            DirectoryFeedKind::AdDirSync,
        ),
        (
            DirectoryProvider::UniventionCorporateServer,
            DirectoryFeedKind::UcsProvisioningService,
        ),
    ];

    for (matrix_index, (provider, feed)) in matrix.into_iter().enumerate() {
        let connector_id = Uuid::from_u128(0x3000 + matrix_index as u128);
        let mut previous = None;
        for sequence in 1..=256_u64 {
            let page = state_page(
                connector_id,
                feed,
                sequence,
                format!("native-{matrix_index:02}-{sequence:04}"),
            );
            page.validate(provider, previous.as_ref())
                .expect("native cursor advances monotonically");
            previous = Some(page.cursor.clone());

            if sequence == 128 {
                let encoded = serde_json::to_vec(&previous).expect("cursor serialization");
                previous = Some(serde_json::from_slice(&encoded).expect("cursor restart"));
            }
        }

        let previous = previous.expect("cursor history");
        let regressed = state_page(
            connector_id,
            feed,
            previous.sequence - 1,
            format!("native-{matrix_index:02}-regressed"),
        );
        assert_eq!(
            regressed.validate(provider, Some(&previous)),
            Err(DirectoryError::CursorRegressed)
        );

        let ahead = state_page(
            connector_id,
            feed,
            previous.sequence,
            format!("native-{matrix_index:02}-ahead"),
        );
        let mut ahead = ahead;
        if let LegacyDirectoryChange::Upsert(user) = &mut ahead.changes[0] {
            user.source_revision = previous.sequence + 1;
        }
        assert_eq!(
            ahead.validate(provider, None),
            Err(DirectoryError::ChangeAheadOfCursor)
        );
    }
}

fn revocation_link(value: &str) -> ImmutableIdentityLink {
    ImmutableIdentityLink::new("directory:immutable-id", value).expect("immutable revocation link")
}

#[test]
fn disable_and_delete_revoke_immediately_and_have_exact_five_minute_deadlines() {
    let at = fixed_at();
    let identity_link = revocation_link("object-0001");
    let active = DurableRevocationState::try_new(
        FIXED_CONNECTOR_ID,
        "external-0001",
        identity_link.clone(),
        true,
        at,
        None,
        1,
    )
    .expect("active state");
    assert!(active.authorization_allowed(at + DIRECTORY_REVOCATION_SLA));
    assert!(
        !active.authorization_allowed(at + DIRECTORY_REVOCATION_SLA + Duration::nanoseconds(1))
    );

    let mut ledger = DirectoryRevocationLedger::default();
    let reasons = [
        DirectoryRevocationReason::AccountDisabled,
        DirectoryRevocationReason::AccountDeleted,
    ];
    for (index, reason) in reasons.into_iter().enumerate() {
        let observed_at = at + Duration::seconds(index as i64);
        let input = DirectoryRevocationFactInput {
            connector_id: FIXED_CONNECTOR_ID,
            external_id: "external-0001".to_owned(),
            identity_link: identity_link.clone(),
            reason,
            source_revision: index as u64 + 2,
            idempotency_key: format!("directory-revocation-cert-{index}"),
            observed_at,
        };
        let (outcome, fact) = ledger
            .append(input.clone())
            .expect("append revocation fact");
        assert_eq!(outcome, DirectoryRevocationAppendOutcome::Applied);
        assert_eq!(fact.deadline(), observed_at + DIRECTORY_REVOCATION_SLA);
        assert!(!fact.is_late_at(fact.deadline()));
        assert!(fact.is_late_at(fact.deadline() + Duration::nanoseconds(1)));

        let (replay, replayed) = ledger.append(input).expect("replay is idempotent");
        assert_eq!(replay, DirectoryRevocationAppendOutcome::AlreadyApplied);
        assert_eq!(replayed.event_id(), fact.event_id());
    }
    assert_eq!(ledger.facts().len(), 2);
    assert_eq!(ledger.facts()[0].sequence(), 1);
    assert_eq!(ledger.facts()[1].sequence(), 2);
    assert_eq!(
        ledger.facts()[1].previous_hash(),
        Some(ledger.facts()[0].fact_hash())
    );
    ledger.verify().expect("append-only revocation chain");

    let mut read_model = DirectoryRevocationReadModel::default();
    for fact in ledger.facts() {
        read_model.apply(fact).expect("project revocation fact");
        read_model.apply(fact).expect("replay projected event");
    }
    let projected = read_model
        .state_for(FIXED_CONNECTOR_ID, &identity_link)
        .expect("projected identity state");
    assert!(!projected.active());
    assert_eq!(projected.gate(at), RevocationGate::Revoked);
    assert!(!projected.authorization_allowed(at));
    assert_eq!(read_model.applied_event_count(), 2);
}

fn role_identity() -> DirectoryRoleIdentity {
    DirectoryRoleIdentity::from_immutable_link(
        ImmutableIdentityLink::new("directory:immutable-id", "role-object-0001")
            .expect("immutable role link"),
    )
}

fn role_mapping(revision: u64, connector_id: Uuid) -> DirectoryManagedRoleMapping {
    DirectoryManagedRoleMapping::try_new(
        connector_id,
        DirectoryGroupRef::try_new("group:finance-approvers").expect("group"),
        DirectoryRoleRef::try_new("finance.approve").expect("role"),
        ConnectorScope::Platform,
        true,
        true,
        revision,
        fixed_at() + Duration::seconds(revision as i64),
    )
    .expect("directory role mapping")
}

#[test]
fn role_mapping_revisions_emit_append_only_role_facts_and_rebuildable_projection() {
    let mapping_v1 = role_mapping(1, FIXED_CONNECTOR_ID);
    let mapping_v2 = role_mapping(2, FIXED_CONNECTOR_ID);
    assert_ne!(mapping_v1.event_id(), mapping_v2.event_id());
    assert!(
        DirectoryRoleMappingSet::try_new(
            FIXED_CONNECTOR_ID,
            ConnectorScope::Platform,
            [mapping_v1.clone(), mapping_v2.clone()],
        )
        .is_err()
    );

    let identity = role_identity();
    let group = DirectoryGroupRef::try_new("group:finance-approvers").expect("group");
    let role = DirectoryRoleRef::try_new("finance.approve").expect("role");
    let mut ledger = DirectoryRoleFactLedger::default();
    let grant = DirectoryRoleFactInput {
        identity: identity.clone(),
        connector_id: FIXED_CONNECTOR_ID,
        group_ref: group.clone(),
        role_ref: role.clone(),
        scope: ConnectorScope::Platform,
        action: DirectoryRoleFactAction::Grant,
        reason: DirectoryRoleFactReason::GroupMatched,
        idempotency_key: "role-map-revision-1-grant".to_owned(),
        occurred_at: fixed_at(),
    };
    let (_, first) = ledger.append(grant).expect("grant fact");
    let first_hash = first.fact_hash().to_owned();
    let revoke = DirectoryRoleFactInput {
        identity: identity.clone(),
        connector_id: FIXED_CONNECTOR_ID,
        group_ref: group.clone(),
        role_ref: role.clone(),
        scope: ConnectorScope::Platform,
        action: DirectoryRoleFactAction::Revoke,
        reason: DirectoryRoleFactReason::MappingDeactivated,
        idempotency_key: "role-map-revision-1-revoke".to_owned(),
        occurred_at: fixed_at() + Duration::seconds(1),
    };
    let (_, second) = ledger.append(revoke).expect("append mapping revoke");
    let replay_input = DirectoryRoleFactInput {
        identity: identity.clone(),
        connector_id: FIXED_CONNECTOR_ID,
        group_ref: group.clone(),
        role_ref: role.clone(),
        scope: ConnectorScope::Platform,
        action: DirectoryRoleFactAction::Revoke,
        reason: DirectoryRoleFactReason::MappingDeactivated,
        idempotency_key: "role-map-revision-1-revoke".to_owned(),
        occurred_at: fixed_at() + Duration::seconds(1),
    };
    let (replayed, replayed_fact) = ledger.append(replay_input).expect("replay revoke");
    assert_eq!(replayed, DirectoryRoleFactAppendOutcome::AlreadyApplied);
    assert_eq!(replayed_fact.id(), second.id());
    assert_eq!(ledger.facts()[0].fact_hash(), first_hash);

    let new_grant = DirectoryRoleFactInput {
        identity: identity.clone(),
        connector_id: FIXED_CONNECTOR_ID,
        group_ref: group,
        role_ref: role.clone(),
        scope: ConnectorScope::Platform,
        action: DirectoryRoleFactAction::Grant,
        reason: DirectoryRoleFactReason::Reconciliation,
        idempotency_key: "role-map-revision-2-grant".to_owned(),
        occurred_at: fixed_at() + Duration::seconds(2),
    };
    let (_, third) = ledger
        .append(new_grant)
        .expect("new mapping revision grant");
    assert_eq!(third.sequence(), 3);
    assert_eq!(third.previous_hash(), Some(second.fact_hash()));
    ledger.verify().expect("role fact hash chain");

    let mut read_model = erpx_identity::directory::rbac::DirectoryRoleReadModel::default();
    for fact in ledger.facts() {
        read_model.apply(fact).expect("project role fact");
        read_model.apply(fact).expect("replay role event");
    }
    let roles = read_model.roles_for(&identity, FIXED_CONNECTOR_ID);
    assert_eq!(roles.len(), 1);
    assert_eq!(read_model.applied_event_count(), 3);
    let rollback = DirectoryRbacRollbackPlan::finalized(
        FIXED_CONNECTOR_ID,
        "mapping revision certification rollback",
    )
    .expect("rollback plan");
    assert!(rollback.preserves_history());
    assert!(rollback.rebuilds_read_model());
}

#[test]
fn break_glass_is_local_only_and_high_risk_requires_recent_mfa() {
    let now = fixed_at();
    let remote = erpx_identity::directory::sync::DirectoryAuthorizationContext::new(
        "break-glass-operator",
        ConnectorScope::Platform,
        Some(now),
        true,
        false,
    );
    assert!(matches!(remote, Err(SyncError::BreakGlassMustBeLocal)));

    let local_without_mfa = erpx_identity::directory::sync::DirectoryAuthorizationContext::new(
        "break-glass-operator",
        ConnectorScope::Platform,
        None,
        true,
        true,
    )
    .expect("localhost break-glass context");
    assert!(matches!(
        local_without_mfa.authorize(
            erpx_identity::directory::sync::DirectoryAction::StartFullImport,
            &ConnectorScope::Platform,
            now,
        ),
        Err(SyncError::MfaRequired)
    ));
    assert!(
        local_without_mfa
            .authorize(
                erpx_identity::directory::sync::DirectoryAction::StartFullImport,
                &ConnectorScope::Platform,
                now,
            )
            .is_err()
    );

    let local_with_mfa = erpx_identity::directory::sync::DirectoryAuthorizationContext::new(
        "break-glass-operator",
        ConnectorScope::Platform,
        Some(now - Duration::minutes(4)),
        true,
        true,
    )
    .expect("localhost break-glass context with MFA");
    assert!(
        local_with_mfa
            .authorize(
                erpx_identity::directory::sync::DirectoryAction::StartFullImport,
                &ConnectorScope::Platform,
                now,
            )
            .is_ok()
    );
    assert!(matches!(
        local_with_mfa.authorize(
            erpx_identity::directory::sync::DirectoryAction::StartFullImport,
            &ConnectorScope::Platform,
            now + Duration::minutes(1) + Duration::nanoseconds(1),
        ),
        Err(SyncError::MfaRequired)
    ));

    let policy = erpx_identity::directory::DirectoryAuthPolicy::default();
    assert!(policy.allow_local_break_glass(true, true).is_ok());
    assert!(policy.allow_local_break_glass(false, true).is_err());
    assert!(policy.allow_local_break_glass(true, false).is_err());

    // Exercise the dedicated authentication boundary as well: Disabled does
    // not turn into an implicit local fallback, while LocalOnly is explicit.
    let anchor = erpx_identity::directory::tls::TrustAnchor::try_new(
        "directory-ca-certification",
        erpx_identity::directory::tls::CertificateFingerprint::try_new("a".repeat(64))
            .expect("fingerprint"),
        1,
    )
    .expect("trust anchor");
    let tls = erpx_identity::directory::tls::TlsPolicy::ldaps("directory.example.test", anchor)
        .expect("TLS policy");
    let config = DirectoryConnectorConfig::try_new(
        FIXED_CONNECTOR_ID,
        erpx_identity::directory::auth::ConnectorScope::Platform,
        AuthProvider::GenericLdap,
        "ldaps://directory.example.test:636",
        tls,
        erpx_identity::directory::tls::TimeoutPolicy::conservative(),
        erpx_identity::directory::auth::NativeChangeFeed::Rfc4533,
        BreakGlassPolicy::Disabled,
    )
    .expect("auth connector");
    let authenticator = DirectoryAuthenticator::new(config).expect("authenticator");
    let secret = RedactedSecret::try_new("certification-break-glass-secret").expect("secret");
    let request = LocalBreakGlassRequest::try_new_local(
        LocalBreakGlassOrigin::try_from_peer_address(
            "127.0.0.1:43100".parse().expect("loopback peer"),
        )
        .expect("local origin"),
        Uuid::from_u128(0x4000),
        &secret,
        "CERT-DIRECTORY-VOLUME",
        erpx_identity::directory::auth::AccessRisk::Standard,
        None,
        now,
    )
    .expect("explicit request");
    let verifier = RejectingBreakGlass;
    assert_eq!(
        authenticator.authenticate_local_break_glass(&request, &verifier, None),
        Err(erpx_identity::directory::auth::DirectoryAuthError::BreakGlassDisabled)
    );
}

struct RejectingBreakGlass;

impl LocalBreakGlassVerifier for RejectingBreakGlass {
    fn verify(
        &self,
        _request: &LocalBreakGlassRequest<'_>,
    ) -> Result<LocalBreakGlassGrant, LocalBreakGlassError> {
        Err(LocalBreakGlassError::Rejected)
    }
}
