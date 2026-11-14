//! Executive external-connection orchestration.
//!
//! Token persistence, the final Control audit and the shared MCP request budget
//! belong to the store. This module only applies the retained access, scope,
//! expiry, status and one-time-secret rules over current Work facts.
use crate::domain::executive_connections::*;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The narrow port the Runtime PostgreSQL adapter implements. It never accepts
/// a secret hash that was not derived by the caller from a freshly minted secret.
pub trait ExecutiveConnectionsStore: Send + Sync {
    fn list_tokens<'a>(&'a self, actor: &'a ConnectionActor) -> StoreFuture<'a, Vec<TokenRow>>;
    fn scopes<'a>(&'a self, actor: &'a ConnectionActor) -> StoreFuture<'a, Vec<ConnectionScope>>;
    fn eligible<'a>(&'a self, actor: &'a ConnectionActor) -> StoreFuture<'a, bool>;
    fn selected_grant<'a>(
        &'a self,
        actor: &'a ConnectionActor,
    ) -> StoreFuture<'a, Option<GrantRow>>;
    /// Whether the token's persisted assignment, role revision, organization
    /// scope and executive eligibility still resolve at `now`.
    fn access_current<'a>(&'a self, token: &'a TokenRow, now: i64) -> StoreFuture<'a, bool>;
    fn create<'a>(&'a self, connection: &'a NewConnection) -> StoreFuture<'a, ()>;
    fn revoke<'a>(
        &'a self,
        actor: &'a ConnectionActor,
        id: &'a str,
        now: i64,
    ) -> StoreFuture<'a, bool>;
    fn mark_revoked<'a>(&'a self, id: &'a str, now: i64) -> StoreFuture<'a, ()>;
}

/// A validated, fully bounded create that is safe to persist once.
#[derive(Clone, Debug)]
pub struct PreparedConnection {
    pub token: NewConnection,
    pub response: CreatedConnection,
}

/// Validate the request, current scope and assignment, and bound expiry.
///
/// The caller has already admitted an active `executive.access` authority and
/// supplies the freshly minted secret plus its persisted hash.
pub async fn prepare_create(
    store: &dyn ExecutiveConnectionsStore,
    now: i64,
    actor: &ConnectionActor,
    raw: &Value,
    command_key: &str,
    secret: &str,
    secret_hash: &str,
) -> Result<PreparedConnection, BusinessError> {
    let id = validate_command_key(command_key)?;
    let input = parse_create(raw)?;
    if !store.eligible(actor).await? {
        return Err(connection_forbidden());
    }
    let scopes = store.scopes(actor).await?;
    let selected = selected_scope(&scopes, &input.scope_id)
        .ok_or_else(scope_forbidden)?
        .clone();
    let grant = store
        .selected_grant(actor)
        .await?
        .ok_or_else(access_changed)?;
    let expires_at = expires_at(now, input.expires_days, grant.valid_until);
    Ok(PreparedConnection {
        token: NewConnection {
            id: id.clone(),
            secret_hash: secret_hash.to_owned(),
            name: input.name,
            actor: actor.clone(),
            scope_label: selected.label.clone(),
            created_at: now,
            expires_at,
        },
        response: CreatedConnection {
            id,
            secret: secret.to_owned(),
            expires_at,
            scope: selected.label,
            role: grant.role,
        },
    })
}

/// Persist a prepared connection and its final Control audit atomically.
pub async fn commit_create(
    store: &dyn ExecutiveConnectionsStore,
    prepared: &PreparedConnection,
) -> Result<(), BusinessError> {
    store.create(&prepared.token).await
}

/// Best-effort rollback when authorization changed after the Control commit.
pub async fn revoke_failed(
    store: &dyn ExecutiveConnectionsStore,
    id: &str,
    now: i64,
) -> Result<(), BusinessError> {
    store.mark_revoked(id, now).await
}

/// The caller's own connections with the retained derived status. No secret or
/// hash is ever selected into this projection.
pub async fn connections(
    store: &dyn ExecutiveConnectionsStore,
    now: i64,
    actor: &ConnectionActor,
) -> Result<Vec<ExternalConnection>, BusinessError> {
    let rows = store.list_tokens(actor).await?;
    let mut connections = Vec::with_capacity(rows.len());
    for row in &rows {
        let live = row.revoked_at.is_none() && row.expires_at > now;
        let access_current = if live {
            store.access_current(row, now).await?
        } else {
            false
        };
        connections.push(external(row, now, access_current));
    }
    Ok(connections)
}

/// List only the caller's own connections. Scopes are exposed only when this
/// admitted authority may create another connection right now.
pub async fn list(
    store: &dyn ExecutiveConnectionsStore,
    now: i64,
    actor: &ConnectionActor,
    admitted: bool,
    emergency: bool,
) -> Result<ExternalConnections, BusinessError> {
    let connections = connections(store, now, actor).await?;
    let can_create = admitted && !emergency && store.eligible(actor).await?;
    let scopes = if can_create {
        store.scopes(actor).await?
    } else {
        Vec::new()
    };
    Ok(ExternalConnections {
        can_create,
        scopes,
        connections,
    })
}

/// Revoke only a connection owned by the current user, holding and company.
pub async fn revoke(
    store: &dyn ExecutiveConnectionsStore,
    now: i64,
    actor: &ConnectionActor,
    id: &str,
) -> Result<(), BusinessError> {
    if !store.revoke(actor, id, now).await? {
        return Err(connection_not_found());
    }
    Ok(())
}

/// Parse the retained POST branch without accepting partial or unknown bodies.
pub fn is_revoke(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key("revokeId"))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        fail_create: Option<BusinessError>,
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
        fn selected_grant<'a>(
            &'a self,
            _: &'a ConnectionActor,
        ) -> StoreFuture<'a, Option<GrantRow>> {
            Box::pin(async move { Ok(self.grant.clone()) })
        }
        fn access_current<'a>(&'a self, _: &'a TokenRow, _: i64) -> StoreFuture<'a, bool> {
            Box::pin(async move { Ok(self.access) })
        }
        fn create<'a>(&'a self, connection: &'a NewConnection) -> StoreFuture<'a, ()> {
            Box::pin(async move {
                if let Some(ref error) = self.fail_create {
                    return Err(BusinessError::new(error.kind, error.code, error.message));
                }
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
                Ok(id == "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
            })
        }
        fn mark_revoked<'a>(&'a self, id: &'a str, now: i64) -> StoreFuture<'a, ()> {
            Box::pin(async move {
                self.revoked.lock().unwrap().push((id.to_owned(), now));
                Ok(())
            })
        }
    }

    fn actor() -> ConnectionActor {
        ConnectionActor {
            user_id: "user".into(),
            holding_id: "h001".into(),
            company_id: "company".into(),
            assignment_id: "assignment".into(),
            assignment_version: 3,
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
            label: "Company · Company-wide".into(),
            branch_id: None,
            department_id: None,
        }
    }

    fn token(id: &str, expires_at: i64, revoked_at: Option<i64>) -> TokenRow {
        TokenRow {
            id: id.into(),
            name: "Agent".into(),
            user_id: "user".into(),
            holding_id: "h001".into(),
            company_id: "company".into(),
            assignment_id: "assignment".into(),
            assignment_version: 3,
            role: "executive_author".into(),
            assignment_scope_kind: "holding".into(),
            assignment_scope_node_id: None,
            branch_id: None,
            department_id: None,
            scope_label: "Company · Company-wide".into(),
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

    fn key() -> &'static str {
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
    }

    #[tokio::test]
    async fn create_binds_current_scope_assignment_and_bounded_expiry() {
        let store = sample();
        let raw = serde_json::json!({"name":"Agent","scopeId":"company","expiresDays":2});
        let prepared = prepare_create(&store, 1_000, &actor(), &raw, key(), "erpx_exec_s", "hash")
            .await
            .unwrap();
        assert_eq!(prepared.token.id, key());
        assert_eq!(prepared.token.actor.assignment_version, 3);
        assert_eq!(prepared.token.expires_at, 1_000 + 2 * 86_400_000);
        assert_eq!(prepared.response.role, "executive_author");
        assert_eq!(prepared.response.scope, "Company · Company-wide");
    }

    #[tokio::test]
    async fn create_caps_expiry_at_the_assignment_valid_until() {
        let mut store = sample();
        store.grant = Some(GrantRow {
            role: "executive_author".into(),
            valid_until: 2_000,
        });
        let raw = serde_json::json!({"name":"Agent","scopeId":"company","expiresDays":30});
        let prepared = prepare_create(&store, 1_000, &actor(), &raw, key(), "erpx_exec_s", "hash")
            .await
            .unwrap();
        assert_eq!(prepared.token.expires_at, 2_000);
    }

    #[tokio::test]
    async fn create_rejects_ineligible_forbidden_scope_and_changed_assignment() {
        let raw = serde_json::json!({"name":"Agent","scopeId":"company"});
        let mut store = sample();
        store.eligible = false;
        assert_eq!(
            prepare_create(&store, 1_000, &actor(), &raw, key(), "s", "h")
                .await
                .unwrap_err()
                .code,
            "EXECUTIVE_CONNECTION_FORBIDDEN"
        );
        let mut store = sample();
        store.scopes.clear();
        assert_eq!(
            prepare_create(&store, 1_000, &actor(), &raw, key(), "s", "h")
                .await
                .unwrap_err()
                .code,
            "CONNECTION_SCOPE_FORBIDDEN"
        );
        let mut store = sample();
        store.grant = None;
        assert_eq!(
            prepare_create(&store, 1_000, &actor(), &raw, key(), "s", "h")
                .await
                .unwrap_err()
                .code,
            "EXECUTIVE_ACCESS_CHANGED"
        );
    }

    #[tokio::test]
    async fn create_requires_the_uuid_command_key_and_strict_body() {
        let store = sample();
        let raw = serde_json::json!({"name":"Agent","scopeId":"company"});
        assert_eq!(
            prepare_create(&store, 1_000, &actor(), &raw, "not-a-key", "s", "h")
                .await
                .unwrap_err()
                .code,
            "COMMAND_KEY_REQUIRED"
        );
        assert_eq!(
            prepare_create(
                &store,
                1_000,
                &actor(),
                &serde_json::json!({"name":"Agent"}),
                key(),
                "s",
                "h"
            )
            .await
            .unwrap_err()
            .code,
            "CONNECTION_INPUT_INVALID"
        );
    }

    #[tokio::test]
    async fn commit_persists_once_and_surfaces_limit_or_replay() {
        let store = sample();
        let raw = serde_json::json!({"name":"Agent","scopeId":"company"});
        let prepared = prepare_create(&store, 1_000, &actor(), &raw, key(), "s", "h")
            .await
            .unwrap();
        commit_create(&store, &prepared).await.unwrap();
        assert_eq!(store.created.lock().unwrap().len(), 1);
        assert_eq!(store.created.lock().unwrap()[0].id, key());
        let mut limited = sample();
        limited.fail_create = Some(connection_limit());
        assert_eq!(
            commit_create(&limited, &prepared).await.unwrap_err().code,
            "CONNECTION_LIMIT"
        );
    }

    #[tokio::test]
    async fn list_derives_each_status_and_hides_scopes_when_not_admitted() {
        let mut store = sample();
        store.tokens = vec![
            token("a", 5_000, None),
            token("b", 500, None),
            token("c", 5_000, Some(1)),
        ];
        let admitted = list(&store, 1_000, &actor(), true, false).await.unwrap();
        assert!(admitted.can_create);
        assert_eq!(admitted.scopes, vec![scope()]);
        assert_eq!(
            admitted
                .connections
                .iter()
                .map(|c| c.status)
                .collect::<Vec<_>>(),
            vec![
                ConnectionStatus::Active,
                ConnectionStatus::Expired,
                ConnectionStatus::Revoked
            ]
        );
        let denied = list(&store, 1_000, &actor(), false, false).await.unwrap();
        assert!(!denied.can_create);
        assert!(denied.scopes.is_empty());
        let emergency = list(&store, 1_000, &actor(), true, true).await.unwrap();
        assert!(!emergency.can_create);
        store.access = false;
        let changed = list(&store, 1_000, &actor(), true, false).await.unwrap();
        assert_eq!(
            changed.connections[0].status,
            ConnectionStatus::AccessChanged
        );
    }

    #[tokio::test]
    async fn revoke_requires_ownership_and_is_idempotent_by_store_contract() {
        let store = sample();
        assert!(
            revoke(
                &store,
                1_000,
                &actor(),
                "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
            )
            .await
            .is_ok()
        );
        assert_eq!(
            revoke(
                &store,
                1_000,
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
    fn revoke_branch_is_selected_by_key_presence_like_the_retained_route() {
        assert!(is_revoke(&serde_json::json!({"revokeId":"x"})));
        assert!(!is_revoke(
            &serde_json::json!({"name":"Agent","scopeId":"c"})
        ));
        assert!(!is_revoke(&serde_json::json!([])));
    }
}
