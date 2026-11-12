//! Portable Executive external-connection contract: strict request parsing, the
//! one-time hashed secret, derived status, scope/assignment binding and the
//! retained list/revoke rules. No database is opened.
#![allow(dead_code)]

use erpx_server_runtime::public::executive_connections::{
    ConnectionActor, ConnectionScope, ConnectionStatus, CreateConnectionInput,
    EXECUTIVE_CONNECTION_LIMIT, EXECUTIVE_CONNECTION_SECRET_LENGTH,
    EXECUTIVE_CONNECTION_SECRET_PREFIX, ExecutiveConnectionsStore, GrantRow, NewConnection,
    StoreFuture, TokenRow, commit_create, hash_secret, list, mint_secret, parse_create,
    parse_revoke, prepare_create, revoke, status, token_secret, validate_command_key,
};
use serde_json::json;
use std::sync::Mutex;

#[derive(Default)]
struct Store {
    tokens: Vec<TokenRow>,
    scopes: Vec<ConnectionScope>,
    eligible: bool,
    grant: Option<GrantRow>,
    access: bool,
    created: Mutex<Vec<NewConnection>>,
    revoked: Mutex<Vec<(String, i64)>>,
}

impl ExecutiveConnectionsStore for Store {
    fn list_tokens<'a>(&'a self, _: &'a ConnectionActor) -> StoreFuture<'a, Vec<TokenRow>> {
        Box::pin(async move { Ok(self.tokens.clone()) })
    }
    fn scopes<'a>(&'a self, _: &'a ConnectionActor) -> StoreFuture<'a, Vec<ConnectionScope>> {
        Box::pin(async move { Ok(self.scopes.clone()) })
    }
    fn eligible<'a>(&'a self, _: &'a ConnectionActor) -> StoreFuture<'a, bool> {
        Box::pin(async move { Ok(self.eligible) })
    }
    fn selected_grant<'a>(&'a self, _: &'a ConnectionActor) -> StoreFuture<'a, Option<GrantRow>> {
        Box::pin(async move { Ok(self.grant.clone()) })
    }
    fn access_current<'a>(&'a self, _: &'a TokenRow, _: i64) -> StoreFuture<'a, bool> {
        Box::pin(async move { Ok(self.access) })
    }
    fn create<'a>(&'a self, connection: &'a NewConnection) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.created.lock().unwrap().push(connection.clone());
            Ok(())
        })
    }
    fn revoke<'a>(
        &'a self,
        _: &'a ConnectionActor,
        id: &'a str,
        now: i64,
    ) -> StoreFuture<'a, bool> {
        Box::pin(async move {
            self.revoked.lock().unwrap().push((id.to_owned(), now));
            Ok(id == ACTOR_ASSIGNMENT_ID)
        })
    }
    fn mark_revoked<'a>(&'a self, id: &'a str, now: i64) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.revoked.lock().unwrap().push((id.to_owned(), now));
            Ok(())
        })
    }
}

const ACTOR_ASSIGNMENT_ID: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

fn actor() -> ConnectionActor {
    ConnectionActor {
        user_id: "reviewer".into(),
        holding_id: "h001".into(),
        company_id: "company".into(),
        assignment_id: "assignment".into(),
        assignment_version: 4,
        role: "executive_author".into(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        branch_id: None,
        department_id: None,
    }
}

fn scope() -> ConnectionScope {
    ConnectionScope {
        id: "company".into(),
        label: "Acme Group · Company-wide".into(),
        branch_id: None,
        department_id: None,
    }
}

fn token(id: &str, expires_at: i64, revoked_at: Option<i64>) -> TokenRow {
    TokenRow {
        id: id.into(),
        name: "MCP agent".into(),
        user_id: "reviewer".into(),
        holding_id: "h001".into(),
        company_id: "company".into(),
        assignment_id: "assignment".into(),
        assignment_version: 4,
        role: "executive_author".into(),
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
        branch_id: None,
        department_id: None,
        scope_label: "Acme Group · Company-wide".into(),
        created_at: 1,
        expires_at,
        revoked_at,
        last_used_at: None,
    }
}

fn sample() -> Store {
    Store {
        eligible: true,
        grant: Some(GrantRow {
            role: "executive_author".into(),
            valid_until: i64::MAX,
        }),
        scopes: vec![scope()],
        access: true,
        ..Store::default()
    }
}

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

#[test]
fn create_input_is_strict_bounded_and_defaults_thirty_days() {
    let input = parse_create(&json!({"name":"  MCP agent  ","scopeId":"company"}))
        .expect("a strict create body is accepted");
    assert_eq!(
        input,
        CreateConnectionInput {
            name: "MCP agent".into(),
            scope_id: "company".into(),
            expires_days: 30
        }
    );
    for bad in [
        json!({}),
        json!({"name":"","scopeId":"company"}),
        json!({"name":"MCP","scopeId":""}),
        json!({"name":"MCP","scopeId":"company","expiresDays":0}),
        json!({"name":"MCP","scopeId":"company","expiresDays":91}),
        json!({"name":"MCP","scopeId":"company","expiresDays":1.5}),
        json!({"name":"MCP","scopeId":"company","extra":true}),
    ] {
        assert_eq!(
            parse_create(&bad).unwrap_err().code,
            "CONNECTION_INPUT_INVALID",
            "accepted {bad}"
        );
    }
    let long = "x".repeat(81);
    assert!(
        parse_create(&json!({"name":long,"scopeId":"company"})).is_err(),
        "an over-long name stays rejected"
    );
}

#[test]
fn command_key_and_bearer_secret_keep_the_retained_wire_format() {
    assert_eq!(
        validate_command_key("AAAAAAAA-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
    );
    assert_eq!(
        validate_command_key("not-a-key").unwrap_err().code,
        "COMMAND_KEY_REQUIRED"
    );
    assert_eq!(parse_revoke(&json!({"revokeId":KEY})).unwrap(), KEY);
    assert!(parse_revoke(&json!({"revokeId":KEY,"name":"extra"})).is_err());
    let secret = format!("erpx_exec_{}", "a".repeat(43));
    assert_eq!(
        token_secret(&format!("Bearer {secret}")),
        Some(secret.as_str())
    );
    assert!(token_secret("Bearer session-cookie").is_none());
    let minted = mint_secret().unwrap();
    assert_eq!(
        minted.len(),
        EXECUTIVE_CONNECTION_SECRET_PREFIX.len() + EXECUTIVE_CONNECTION_SECRET_LENGTH
    );
    assert_eq!(hash_secret(&minted).len(), 64);
    assert!(
        hash_secret(&minted)
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
}

#[tokio::test]
async fn create_binds_current_scope_and_never_replays_the_secret() {
    let store = sample();
    let raw = json!({"name":"MCP agent","scopeId":"company","expiresDays":2});
    let prepared = prepare_create(&store, NOW, &actor(), &raw, KEY, "erpx_exec_secret", "hash")
        .await
        .expect("an eligible current scope is accepted");
    assert_eq!(prepared.token.id, KEY);
    assert_eq!(prepared.token.actor.assignment_version, 4);
    assert_eq!(prepared.token.actor.role, "executive_author");
    assert_eq!(prepared.token.scope_label, "Acme Group · Company-wide");
    assert_eq!(prepared.token.expires_at, NOW + 2 * 86_400_000);
    assert_eq!(prepared.response.secret, "erpx_exec_secret");
    assert_eq!(prepared.response.role, "executive_author");
    commit_create(&store, &prepared).await.unwrap();
    assert_eq!(store.created.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn create_rejects_ineligible_foreign_scope_missing_grant_and_bad_key() {
    let raw = json!({"name":"MCP agent","scopeId":"company"});
    let mut store = sample();
    store.eligible = false;
    assert_eq!(
        prepare_create(&store, NOW, &actor(), &raw, KEY, "s", "h")
            .await
            .unwrap_err()
            .code,
        "EXECUTIVE_CONNECTION_FORBIDDEN"
    );
    let mut store = sample();
    store.scopes.clear();
    assert_eq!(
        prepare_create(&store, NOW, &actor(), &raw, KEY, "s", "h")
            .await
            .unwrap_err()
            .code,
        "CONNECTION_SCOPE_FORBIDDEN"
    );
    let mut store = sample();
    store.grant = None;
    assert_eq!(
        prepare_create(&store, NOW, &actor(), &raw, KEY, "s", "h")
            .await
            .unwrap_err()
            .code,
        "EXECUTIVE_ACCESS_CHANGED"
    );
    assert_eq!(
        prepare_create(&sample(), NOW, &actor(), &raw, "bad", "s", "h")
            .await
            .unwrap_err()
            .code,
        "COMMAND_KEY_REQUIRED"
    );
}

#[tokio::test]
async fn expiry_is_capped_by_the_current_assignment_valid_until() {
    let mut store = sample();
    store.grant = Some(GrantRow {
        role: "executive_author".into(),
        valid_until: NOW + 1_000,
    });
    let prepared = prepare_create(
        &store,
        NOW,
        &actor(),
        &json!({"name":"MCP agent","scopeId":"company","expiresDays":30}),
        KEY,
        "s",
        "h",
    )
    .await
    .unwrap();
    assert_eq!(prepared.token.expires_at, NOW + 1_000);
}

#[tokio::test]
async fn list_derives_every_status_and_only_exposes_scopes_when_admitted() {
    let mut store = sample();
    store.tokens = vec![
        token("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", NOW + 1_000, None),
        token("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", NOW - 1, None),
        token("cccccccc-cccc-cccc-cccc-cccccccccccc", NOW + 1_000, Some(1)),
    ];
    let admitted = list(&store, NOW, &actor(), true, false).await.unwrap();
    assert!(admitted.can_create);
    assert_eq!(admitted.scopes, vec![scope()]);
    assert_eq!(
        admitted
            .connections
            .iter()
            .map(|connection| connection.status)
            .collect::<Vec<_>>(),
        vec![
            ConnectionStatus::Active,
            ConnectionStatus::Expired,
            ConnectionStatus::Revoked
        ]
    );
    let denied = list(&store, NOW, &actor(), false, false).await.unwrap();
    assert!(!denied.can_create);
    assert!(denied.scopes.is_empty());
    assert_eq!(denied.connections.len(), 3);
    let emergency = list(&store, NOW, &actor(), true, true).await.unwrap();
    assert!(!emergency.can_create);
    store.access = false;
    let changed = list(&store, NOW, &actor(), true, false).await.unwrap();
    assert_eq!(
        changed.connections[0].status,
        ConnectionStatus::AccessChanged
    );
}

#[tokio::test]
async fn revoke_requires_the_exact_owner_scope() {
    let store = sample();
    revoke(&store, NOW, &actor(), ACTOR_ASSIGNMENT_ID)
        .await
        .expect("the seeded owner connection is revoked");
    assert_eq!(
        revoke(
            &store,
            NOW,
            &actor(),
            "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"
        )
        .await
        .unwrap_err()
        .code,
        "CONNECTION_NOT_FOUND"
    );
    assert_eq!(store.revoked.lock().unwrap().len(), 2);
}

#[test]
fn status_prefers_revocation_then_expiry_then_current_access() {
    let value = token(KEY, NOW, None);
    assert_eq!(status(&value, NOW - 1, true), ConnectionStatus::Active);
    assert_eq!(
        status(&value, NOW - 1, false),
        ConnectionStatus::AccessChanged
    );
    assert_eq!(status(&value, NOW, true), ConnectionStatus::Expired);
    let mut revoked = value;
    revoked.revoked_at = Some(1);
    assert_eq!(status(&revoked, NOW - 1, false), ConnectionStatus::Revoked);
    assert_eq!(EXECUTIVE_CONNECTION_LIMIT, 20);
}
