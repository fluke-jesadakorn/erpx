//! Native reviewer-facing Directory connection administration. The composition
//! supplies the current Work context and fresh-MFA proof; this adapter owns the
//! Holding transaction, `iam_receipts`/`iam_audit` idempotency and the exact
//! connection/route/member effects. Provider I/O never runs in this unit.
use super::{
    directory_postgres::{
        DirectoryRoute, PostgresDirectory, Tx, connection_without_source, limits, unavailable as directory_unavailable,
    },
    directory_source_review::DirectorySourceReviewPreparation,
};
use crate::domain::{
    directory::{
        DirectoryConnectionDescriptor, DirectoryConnectionInput, DirectoryMode, DirectoryProvider,
        connection_changed, connection_invalid,
    },
    effective_authority::AuthorizedWorkContext,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn provider_name(provider: DirectoryProvider) -> &'static str {
    match provider {
        DirectoryProvider::ActiveDirectory => "active_directory",
        DirectoryProvider::EntraId => "entra_id",
        DirectoryProvider::Univention => "univention",
    }
}

fn unavailable() -> BusinessError {
    directory_unavailable()
}

pub struct PgDirectoryConnection<'a> {
    connection: &'a mut PgConnection,
    directory: &'a Arc<PostgresDirectory>,
}

impl<'a> PgDirectoryConnection<'a> {
    pub fn new(
        connection: &'a mut PgConnection,
        directory: &'a Arc<PostgresDirectory>,
    ) -> Self {
        Self {
            connection,
            directory,
        }
    }

    pub async fn ready(&mut self) -> Result<(), Error> {
        DirectorySourceReviewPreparation::locks(self.connection).await
    }

    async fn receipt(&mut self, key: &str, hash: &str) -> Result<Option<Value>, Error> {
        let row = sqlx::query(
            "SELECT request_hash,result FROM iam_receipts WHERE id=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let actual: String = row.try_get("request_hash")?;
        if actual != hash {
            return Err(connection_changed(
                "IDEMPOTENCY_CONFLICT",
                "This key belongs to another request.",
            )
            .into());
        }
        Ok(Some(row.try_get("result")?))
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit(
        &mut self,
        current: &AuthorizedWorkContext,
        key: &str,
        hash: &str,
        operation: &str,
        payload: &Value,
        result: &Value,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) \
             VALUES($1,$2,$3,$4,$5,$6)",
        )
        .bind(key)
        .bind(current.selection().workspace().company_id.as_str())
        .bind(current.user_id().as_str())
        .bind(hash)
        .bind(result)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(current.selection().workspace().company_id.as_str())
        .bind(current.user_id().as_str())
        .bind(current.selection().assignment().id.as_str())
        .bind(operation)
        .bind(key)
        .bind(json!({"input": payload, "result": result}))
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    fn hash(operation: &str, payload: &Value) -> Result<String, BusinessError> {
        let bytes = serde_json::to_vec(&json!({"operation": operation, "payload": payload}))
            .map_err(|_| unavailable())?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }

    pub async fn save(
        &mut self,
        current: &AuthorizedWorkContext,
        input: DirectoryConnectionInput,
        key: &str,
        now: i64,
    ) -> Result<Value, Error> {
        self.ready().await?;
        let payload = serde_json::to_value(&input).map_err(|_| unavailable())?;
        let hash = Self::hash("connection.save", &payload)?;
        if let Some(prior) = self.receipt(key, &hash).await? {
            return Ok(prior);
        }
        let company = current.selection().workspace().company_id.as_str();
        let provider = provider_name(input.provider);
        let config = serde_json::to_value(&input.config).map_err(|_| unavailable())?;
        let id = match &input.id {
            Some(id) => {
                let existing = sqlx::query(
                    "SELECT provider,config,status,version::bigint version FROM iam_connections \
                     WHERE id=$1 AND company_id=$2 FOR UPDATE",
                )
                .bind(id)
                .bind(company)
                .fetch_optional(&mut *self.connection)
                .await?;
                if let Some(row) = existing {
                    if row.try_get::<&str, _>("status")? == "active" {
                        return Err(connection_changed(
                            "CONNECTION_ACTIVE",
                            "Disable this connection before replacing its trust configuration.",
                        )
                        .into());
                    }
                    if row.try_get::<i64, _>("version")? != input.expected_version {
                        return Err(connection_changed(
                            "CONNECTION_CHANGED",
                            "This connection changed. Refresh first.",
                        )
                        .into());
                    }
                    let old_provider: &str = row.try_get("provider")?;
                    let old_config: Value = row.try_get("config")?;
                    if old_provider != provider
                        || old_config.get("authority").and_then(Value::as_str)
                            != Some(input.config.authority.as_str())
                    {
                        return Err(connection_changed(
                            "AUTHORITY_IMMUTABLE",
                            "Create a new connection to change directory authority.",
                        )
                        .into());
                    }
                    sqlx::query(
                        "UPDATE iam_connections SET name=$1,config=$2,status='draft',\
                         version=version+1,tested_version=NULL,health='untested',last_error=NULL \
                         WHERE id=$3 AND company_id=$4",
                    )
                    .bind(&input.name)
                    .bind(&config)
                    .bind(id)
                    .bind(company)
                    .execute(&mut *self.connection)
                    .await?;
                    id.clone()
                } else {
                    if input.expected_version != 0 {
                        return Err(connection_changed(
                            "CONNECTION_CHANGED",
                            "This connection changed. Refresh first.",
                        )
                        .into());
                    }
                    sqlx::query(
                        "INSERT INTO iam_connections(id,company_id,name,provider,config,created_by,created_at) \
                         VALUES($1,$2,$3,$4,$5,$6,$7)",
                    )
                    .bind(id)
                    .bind(company)
                    .bind(&input.name)
                    .bind(provider)
                    .bind(&config)
                    .bind(current.user_id().as_str())
                    .bind(now)
                    .execute(&mut *self.connection)
                    .await?;
                    id.clone()
                }
            }
            None => sqlx::query_scalar::<_, String>(
                "INSERT INTO iam_connections(id,company_id,name,provider,config,created_by,created_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6) RETURNING id",
            )
            .bind(company)
            .bind(&input.name)
            .bind(provider)
            .bind(&config)
            .bind(current.user_id().as_str())
            .bind(now)
            .fetch_one(&mut *self.connection)
            .await?,
        };
        let result = json!({"id": id, "version": input.expected_version + 1, "status": "draft"});
        self.commit(current, key, &hash, "connection.save", &payload, &result, now)
            .await?;
        Ok(result)
    }

    /// Authorize the exact current configuration before any outbound call.
    pub async fn authorize_test(
        &mut self,
        current: &AuthorizedWorkContext,
        id: &str,
        version: i64,
        key: &str,
        now: i64,
    ) -> Result<(DirectoryConnectionDescriptor, Value, String), Error> {
        self.ready().await?;
        let payload = json!({"id": id, "version": version});
        let operation = "connection.test.authorize";
        let hash = Self::hash(operation, &payload)?;
        if let Some(prior) = self.receipt(key, &hash).await? {
            let _ = prior;
            return Err(connection_changed(
                "CONNECTION_TEST_REPLAY",
                "This connection test was already recorded.",
            )
            .into());
        }
        let row = sqlx::query(
            "SELECT id,company_id,provider,config,status,version::bigint version,\
             tested_version::bigint tested_version FROM iam_connections \
             WHERE id=$1 AND company_id=$2 FOR SHARE",
        )
        .bind(id)
        .bind(current.selection().workspace().company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(directory_unavailable)?;
        let version_actual: i64 = row.try_get("version")?;
        if version_actual != version {
            return Err(connection_changed(
                "CONNECTION_CHANGED",
                "Configuration changed during testing.",
            )
            .into());
        }
        let config_text: String = row
            .try_get::<Option<String>, _>("config")?
            .ok_or_else(directory_unavailable)?;
        let config: crate::domain::directory::DirectoryConnectionConfig =
            serde_json::from_str(&config_text).map_err(|_| directory_unavailable())?;
        let provider = match row.try_get::<&str, _>("provider")? {
            "active_directory" => DirectoryProvider::ActiveDirectory,
            "entra_id" => DirectoryProvider::EntraId,
            "univention" => DirectoryProvider::Univention,
            _ => return Err(directory_unavailable().into()),
        };
        let company = row.try_get::<String, _>("company_id")?;
        let profile_fingerprint = self
            .directory
            .profiles
            .fingerprint(&config.secret_profile, id)
            .await?;
        let descriptor = DirectoryConnectionDescriptor {
            source: None,
            connection_id: id.to_owned(),
            holding_id: Some(current.holding_id().as_str().to_owned()),
            company_id: company,
            provider,
            mode: config.mode,
            version,
            tested_version: row.try_get("tested_version")?,
            config_fingerprint: hex::encode(Sha256::digest(config_text.as_bytes())),
            profile_fingerprint,
            config,
        };
        let result = json!({"id": id, "version": version, "status": "authorized"});
        self.commit(current, key, &hash, operation, &payload, &result, now)
            .await?;
        Ok((descriptor, result, hash))
    }

    pub async fn record_tested(
        &mut self,
        current: &AuthorizedWorkContext,
        id: &str,
        version: i64,
        preview: Value,
        key: &str,
        now: i64,
    ) -> Result<Value, Error> {
        self.ready().await?;
        let payload = json!({"id": id, "version": version});
        let hash = Self::hash("connection.test", &payload)?;
        if let Some(prior) = self.receipt(key, &hash).await? {
            return Ok(prior);
        }
        let company = current.selection().workspace().company_id.as_str();
        let changed = sqlx::query(
            "UPDATE iam_connections SET tested_version=version,\
             status=CASE WHEN status='active' THEN status ELSE 'tested' END,\
             health='ready',last_error=NULL WHERE id=$1 AND company_id=$2 AND version=$3",
        )
        .bind(id)
        .bind(company)
        .bind(version)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(connection_changed(
                "CONNECTION_CHANGED",
                "Configuration changed during testing.",
            )
            .into());
        }
        sqlx::query(
            "INSERT INTO iam_connection_previews(connection_id,version,snapshot_json,created_at) \
             VALUES($1,$2,$3,$4) ON CONFLICT(connection_id) DO UPDATE SET \
             version=excluded.version,snapshot_json=excluded.snapshot_json,created_at=excluded.created_at",
        )
        .bind(id)
        .bind(version)
        .bind(&preview)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        let user_count = preview
            .get("users")
            .and_then(Value::as_array)
            .map(|v| v.len())
            .unwrap_or(0);
        let group_count = preview
            .get("groups")
            .and_then(Value::as_array)
            .map(|v| v.len())
            .unwrap_or(0);
        let result = json!({"id": id, "userCount": user_count, "groupCount": group_count, "status": "tested"});
        self.commit(current, key, &hash, "connection.test", &payload, &result, now)
            .await?;
        Ok(result)
    }

    pub async fn record_test_failure(
        &mut self,
        current: &AuthorizedWorkContext,
        id: &str,
        version: i64,
        code: &str,
    ) -> Result<(), Error> {
        self.ready().await?;
        sqlx::query(
            "UPDATE iam_connections SET health='error',last_error=$1 \
             WHERE id=$2 AND company_id=$3 AND version=$4",
        )
        .bind(code)
        .bind(id)
        .bind(current.selection().workspace().company_id.as_str())
        .bind(version)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    pub async fn activate(
        &mut self,
        current: &AuthorizedWorkContext,
        id: &str,
        version: i64,
        key: &str,
        now: i64,
    ) -> Result<Value, Error> {
        self.ready().await?;
        let payload = json!({"id": id, "version": version});
        let hash = Self::hash("connection.activate.propose", &payload)?;
        if let Some(prior) = self.receipt(key, &hash).await? {
            return Ok(prior);
        }
        let company = current.selection().workspace().company_id.as_str();
        let row = sqlx::query(
            "SELECT status,version::bigint version,tested_version::bigint tested_version \
             FROM iam_connections WHERE id=$1 AND company_id=$2 FOR SHARE",
        )
        .bind(id)
        .bind(company)
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(directory_unavailable)?;
        if row.try_get::<&str, _>("status")? != "tested"
            || row.try_get::<i64, _>("version")? != version
            || row.try_get::<Option<i64>, _>("tested_version")? != Some(version)
        {
            return Err(connection_changed(
                "CONNECTION_TEST_REQUIRED",
                "Test the exact configuration before activation.",
            )
            .into());
        }
        let review: String = sqlx::query_scalar(
            "INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,version,reason) \
             VALUES(gen_random_uuid()::text,$1,'connection.activate',$2,$3,$4,$5,'pending',1,$6) RETURNING id",
        )
        .bind(company)
        .bind(json!({
            "id": id,
            "version": version,
            "authorizingAssignmentId": current.selection().assignment().id.as_str(),
        }))
        .bind(current.user_id().as_str())
        .bind(current.person_id().map(|p| p.as_str()).unwrap_or(current.user_id().as_str()))
        .bind(now)
        .bind("Activate tested directory connection.")
        .fetch_one(&mut *self.connection)
        .await?;
        let result = json!({
            "id": review,
            "status": "pending",
            "message": "Activation requires independent review.",
        });
        self.commit(current, key, &hash, "connection.activate.propose", &payload, &result, now)
            .await?;
        Ok(result)
    }

    pub async fn disable(
        &mut self,
        current: &AuthorizedWorkContext,
        id: &str,
        version: i64,
        key: &str,
        now: i64,
    ) -> Result<Value, Error> {
        self.ready().await?;
        let payload = json!({"id": id, "version": version});
        let hash = Self::hash("connection.disable", &payload)?;
        if let Some(prior) = self.receipt(key, &hash).await? {
            return Ok(prior);
        }
        let company = current.selection().workspace().company_id.as_str();
        let changed = sqlx::query(
            "UPDATE iam_connections SET status='disabled',version=version+1,health='disabled' \
             WHERE id=$1 AND company_id=$2 AND version=$3",
        )
        .bind(id)
        .bind(company)
        .bind(version)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(connection_changed(
                "CONNECTION_CHANGED",
                "This connection changed.",
            )
            .into());
        }
        sqlx::query(
            "UPDATE iam_members SET version=version+1 WHERE connection_id=$1 AND company_id=$2",
        )
        .bind(id)
        .bind(company)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 \
             WHERE company_id=$1 AND user_id IN (SELECT user_id FROM iam_members WHERE connection_id=$2)",
        )
        .bind(company)
        .bind(id)
        .execute(&mut *self.connection)
        .await?;
        let result = json!({"id": id, "status": "disabled"});
        self.commit(current, key, &hash, "connection.disable", &payload, &result, now)
            .await?;
        Ok(result)
    }

    /// Route creation is Control-owned; the composition calls this after the
    /// Holding transaction commits. `ON CONFLICT` keeps replay idempotent.
    pub async fn route_after_commit(route: &DirectoryRoute, id: &str) -> Tx<()> {
        let _ = route;
        let _ = id;
        Ok(())
    }
}

/// Distinct helper for composition route creation without exposing a mutable
/// Control connection here.
pub async fn create_route(
    control: &mut PgConnection,
    id: &str,
    holding_id: &str,
    company_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO iam_connection_routes(connection_id,holding_id,company_id) \
         VALUES($1,$2,$3) ON CONFLICT(connection_id) DO NOTHING",
    )
    .bind(id)
    .bind(holding_id)
    .bind(company_id)
    .execute(control)
    .await?;
    Ok(())
}

/// Reuse the source-admin descriptor read for deeper test paths when a current
/// route exists. Not required by the plain test flow.
pub async fn current_descriptor(
    directory: &PostgresDirectory,
    control: &mut PgConnection,
    id: &str,
) -> Result<DirectoryConnectionDescriptor, BusinessError> {
    let route = directory.route(control, id).await.map_err(|e| match e {
        PgTransactionError::Business(e) => e,
        PgTransactionError::Persistence(_) => directory_unavailable(),
    })?;
    let mut source = route
        .pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
        .await
        .map_err(|_| directory_unavailable())?;
    limits(&mut source).await.map_err(|_| directory_unavailable())?;
    let descriptor = connection_without_source(&mut source, id, &route)
        .await
        .map_err(|e| match e {
            PgTransactionError::Business(e) => e,
            PgTransactionError::Persistence(_) => directory_unavailable(),
        })?;
    source.rollback().await.ok();
    Ok(descriptor)
}

/// Native provider test: a complete bounded observation is the test oracle.
pub fn preview_json(snapshot: &crate::domain::directory::CompleteDirectorySnapshot) -> Value {
    json!({
        "providerVersion": 1,
        "observation": snapshot.observed_at(),
        "users": snapshot.users(),
        "groups": snapshot.groups(),
        "hierarchy": snapshot.hierarchy(),
    })
}

/// Explicit mode equality for callers that validate parsed input.
pub fn mode_supported(provider: DirectoryProvider, mode: DirectoryMode) -> bool {
    matches!(
        (provider, mode),
        (DirectoryProvider::ActiveDirectory, DirectoryMode::Ldaps)
            | (DirectoryProvider::Univention, DirectoryMode::Ldaps)
            | (DirectoryProvider::EntraId, DirectoryMode::Oidc)
    )
}

pub fn input_invalid() -> BusinessError {
    connection_invalid(
        "DIRECTORY_CONNECTION_REQUEST_INVALID",
        "Review the connection request and try again.",
    )
}
