use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingStoreRegistration {
    pub holding_id: String,
    pub data_profile_key: String,
    pub write_database_key: String,
    pub read_database_key: String,
    pub storage_namespace: String,
    pub credential_key_ref: String,
    pub backup_chain_ref: String,
    pub state: String,
    pub registry_version: u64,
}

/// Maximum number of holding registry rows exposed by one platform read.
pub const MAX_HOLDING_REGISTRY_PAGE_SIZE: u32 = 100;

/// A bounded, deterministic page of holding registry rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingStoreRegistrationPage {
    pub registrations: Vec<HoldingStoreRegistration>,
    pub has_more: bool,
    pub next_after_holding_id: Option<String>,
}

/// Safe detail metadata for Platform Operations. Credential/database/storage
/// values are intentionally represented only as presence flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingStoreRegistrationDetail {
    pub holding_id: String,
    pub data_profile_key: String,
    pub state: String,
    pub registry_version: u64,
    pub write_registered: bool,
    pub read_registered: bool,
    pub storage_registered: bool,
    pub credential_binding_registered: bool,
    pub backup_binding_registered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionAuthorizationRequest<'a> {
    pub holding_id: &'a str,
    pub data_profile_key: &'a str,
    pub actor_ref: &'a str,
    pub permission: &'a str,
    pub operation: &'a str,
    pub purpose: &'a str,
    pub correlation_id: Uuid,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SqlxHoldingStoreRegistry {
    control: Arc<SqlxControlPlane>,
}

impl SqlxHoldingStoreRegistry {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    pub async fn register(
        &self,
        registration: HoldingStoreRegistration,
    ) -> Result<(), PersistenceError> {
        validate_data_profile_reference(&registration.holding_id, "holding_id")?;
        validate_data_profile_reference(&registration.data_profile_key, "data_profile_key")?;
        validate_data_profile_reference(&registration.write_database_key, "write_database_key")?;
        validate_data_profile_reference(&registration.read_database_key, "read_database_key")?;
        validate_data_profile_reference(&registration.storage_namespace, "storage_namespace")?;
        validate_data_profile_reference(&registration.credential_key_ref, "credential_key_ref")?;
        validate_data_profile_reference(&registration.backup_chain_ref, "backup_chain_ref")?;
        if registration.write_database_key == registration.read_database_key {
            return Err(PersistenceError::Configuration(
                "holding write/read database keys must be distinct",
            ));
        }
        if !matches!(
            registration.state.as_str(),
            "provisioning" | "active" | "suspended" | "recovery"
        ) {
            return Err(PersistenceError::InvalidData(
                "invalid holding registry state".into(),
            ));
        }
        let result = sqlx::query(
            "INSERT INTO erpx_control.holding_registry
                (holding_id, data_profile_key, write_database_key, read_database_key,
                 storage_namespace, credential_key_ref, backup_chain_ref, state,
                 registry_version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&registration.holding_id)
        .bind(&registration.data_profile_key)
        .bind(&registration.write_database_key)
        .bind(&registration.read_database_key)
        .bind(&registration.storage_namespace)
        .bind(&registration.credential_key_ref)
        .bind(&registration.backup_chain_ref)
        .bind(&registration.state)
        .bind(i64::try_from(registration.registry_version).map_err(|_| {
            PersistenceError::InvalidData("holding registry version exceeds database range".into())
        })?)
        .execute(&self.control.pools.write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "holding registration was not inserted".into(),
            ));
        }
        Ok(())
    }

    pub async fn get(
        &self,
        holding_id: &str,
    ) -> Result<Option<HoldingStoreRegistration>, PersistenceError> {
        let row = sqlx::query(
            "SELECT holding_id, data_profile_key, write_database_key, read_database_key,
                    storage_namespace, credential_key_ref, backup_chain_ref, state,
                    registry_version
             FROM erpx_control.holding_registry WHERE holding_id = $1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools.write)
        .await?;
        row.map(|row| {
            Ok(HoldingStoreRegistration {
                holding_id: row.try_get("holding_id")?,
                data_profile_key: row.try_get("data_profile_key")?,
                write_database_key: row.try_get("write_database_key")?,
                read_database_key: row.try_get("read_database_key")?,
                storage_namespace: row.try_get("storage_namespace")?,
                credential_key_ref: row.try_get("credential_key_ref")?,
                backup_chain_ref: row.try_get("backup_chain_ref")?,
                state: row.try_get("state")?,
                registry_version: positive_u64(
                    row.try_get("registry_version")?,
                    "holding registry version",
                )?,
            })
        })
        .transpose()
    }

    /// Read one safe registry detail without exposing any opaque database,
    /// storage, credential, or backup reference to the caller.
    pub async fn get_detail(
        &self,
        holding_id: &str,
    ) -> Result<Option<HoldingStoreRegistrationDetail>, PersistenceError> {
        validate_data_profile_reference(holding_id, "holding_id")?;
        let row = sqlx::query(
            "SELECT holding_id, data_profile_key, state, registry_version,
                    (write_database_key <> '') AS write_registered,
                    (read_database_key <> '') AS read_registered,
                    (storage_namespace <> '') AS storage_registered,
                    (credential_key_ref <> '') AS credential_binding_registered,
                    (backup_chain_ref <> '') AS backup_binding_registered
             FROM erpx_control.holding_registry
             WHERE holding_id = $1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools.write)
        .await?;
        row.map(|row| {
            Ok(HoldingStoreRegistrationDetail {
                holding_id: row.try_get("holding_id")?,
                data_profile_key: row.try_get("data_profile_key")?,
                state: row.try_get("state")?,
                registry_version: positive_u64(
                    row.try_get("registry_version")?,
                    "holding registry version",
                )?,
                write_registered: row.try_get("write_registered")?,
                read_registered: row.try_get("read_registered")?,
                storage_registered: row.try_get("storage_registered")?,
                credential_binding_registered: row.try_get("credential_binding_registered")?,
                backup_binding_registered: row.try_get("backup_binding_registered")?,
            })
        })
        .transpose()
    }

    pub async fn list_active(&self) -> Result<Vec<HoldingStoreRegistration>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT holding_id, data_profile_key, write_database_key, read_database_key,
                    storage_namespace, credential_key_ref, backup_chain_ref, state,
                    registry_version
             FROM erpx_control.holding_registry
             WHERE state = 'active'
             ORDER BY holding_id",
        )
        .fetch_all(&self.control.pools.write)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(HoldingStoreRegistration {
                    holding_id: row.try_get("holding_id")?,
                    data_profile_key: row.try_get("data_profile_key")?,
                    write_database_key: row.try_get("write_database_key")?,
                    read_database_key: row.try_get("read_database_key")?,
                    storage_namespace: row.try_get("storage_namespace")?,
                    credential_key_ref: row.try_get("credential_key_ref")?,
                    backup_chain_ref: row.try_get("backup_chain_ref")?,
                    state: row.try_get("state")?,
                    registry_version: positive_u64(
                        row.try_get("registry_version")?,
                        "holding registry version",
                    )?,
                })
            })
            .collect()
    }

    /// List active holding registrations with a caller-supplied hard bound.
    /// The limit is enforced in SQL with one sentinel row so a worker never
    /// has to fetch an unbounded active registry into memory before rejecting
    /// it.
    pub async fn list_active_bounded(
        &self,
        limit: usize,
    ) -> Result<Vec<HoldingStoreRegistration>, PersistenceError> {
        if limit == 0 {
            return Err(PersistenceError::InvalidData(
                "active holding registry limit must be positive".into(),
            ));
        }
        let query_limit = i64::try_from(limit)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "active holding registry limit exceeds database range".into(),
                )
            })?;
        let rows = sqlx::query(
            "SELECT holding_id, data_profile_key, write_database_key, read_database_key,
                    storage_namespace, credential_key_ref, backup_chain_ref, state,
                    registry_version
             FROM erpx_control.holding_registry
             WHERE state = 'active'
             ORDER BY holding_id
             LIMIT $1",
        )
        .bind(query_limit)
        .fetch_all(&self.control.pools.write)
        .await?;
        if rows.len() > limit {
            return Err(PersistenceError::InvalidData(
                "active holding registry set exceeds its worker bound".into(),
            ));
        }
        rows.into_iter()
            .map(|row| {
                Ok(HoldingStoreRegistration {
                    holding_id: row.try_get("holding_id")?,
                    data_profile_key: row.try_get("data_profile_key")?,
                    write_database_key: row.try_get("write_database_key")?,
                    read_database_key: row.try_get("read_database_key")?,
                    storage_namespace: row.try_get("storage_namespace")?,
                    credential_key_ref: row.try_get("credential_key_ref")?,
                    backup_chain_ref: row.try_get("backup_chain_ref")?,
                    state: row.try_get("state")?,
                    registry_version: positive_u64(
                        row.try_get("registry_version")?,
                        "holding registry version",
                    )?,
                })
            })
            .collect()
    }

    /// List all holding registry states for the platform operations read path.
    /// The cursor is an opaque registry identifier and the query never
    /// returns database, storage, credential, or backup references to callers.
    pub async fn list_page(
        &self,
        after_holding_id: Option<&str>,
        page_size: u32,
    ) -> Result<HoldingStoreRegistrationPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_HOLDING_REGISTRY_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "holding registry page size is outside the allowed bound".into(),
            ));
        }
        if let Some(after) = after_holding_id {
            validate_data_profile_reference(after, "after_holding_id")?;
        }
        let rows = sqlx::query(
            "SELECT holding_id, data_profile_key, write_database_key, read_database_key,
                    storage_namespace, credential_key_ref, backup_chain_ref, state,
                    registry_version
             FROM erpx_control.holding_registry
             WHERE ($1::text IS NULL OR holding_id > $1)
             ORDER BY holding_id
             LIMIT $2",
        )
        .bind(after_holding_id)
        .bind(i64::from(page_size) + 1)
        .fetch_all(&self.control.pools.write)
        .await?;
        let has_more = rows.len() > page_size as usize;
        let mut registrations = rows
            .into_iter()
            .take(page_size as usize)
            .map(|row| {
                Ok(HoldingStoreRegistration {
                    holding_id: row.try_get("holding_id")?,
                    data_profile_key: row.try_get("data_profile_key")?,
                    write_database_key: row.try_get("write_database_key")?,
                    read_database_key: row.try_get("read_database_key")?,
                    storage_namespace: row.try_get("storage_namespace")?,
                    credential_key_ref: row.try_get("credential_key_ref")?,
                    backup_chain_ref: row.try_get("backup_chain_ref")?,
                    state: row.try_get("state")?,
                    registry_version: positive_u64(
                        row.try_get("registry_version")?,
                        "holding registry version",
                    )?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_after_holding_id = if has_more {
            registrations
                .last()
                .map(|registration| registration.holding_id.clone())
        } else {
            None
        };
        Ok(HoldingStoreRegistrationPage {
            registrations: std::mem::take(&mut registrations),
            has_more,
            next_after_holding_id,
        })
    }

    /// Authorize an internal projection operation against the control-plane
    /// role assignment aggregate, then record the decision in the durable
    /// audit stream. Holding registry membership alone is not authorization.
    /// The holding reference is intentionally required to be a UUID here so
    /// the scheduler cannot widen a role decision from an opaque string.
    pub async fn authorize_and_audit_projection(
        &self,
        request: ProjectionAuthorizationRequest<'_>,
    ) -> Result<(), PersistenceError> {
        validate_data_profile_reference(request.holding_id, "holding_id")?;
        validate_data_profile_reference(request.data_profile_key, "data_profile_key")?;
        validate_projection_actor_reference(request.actor_ref)?;
        if request.permission.trim().is_empty()
            || request.permission.len() > 128
            || request.permission.chars().any(char::is_control)
            || request.operation.trim().is_empty()
            || request.operation.len() > 64
            || request.operation.chars().any(char::is_control)
            || request.purpose.trim().is_empty()
            || request.purpose.len() > 128
            || request.purpose.chars().any(char::is_control)
            || request.correlation_id == Uuid::nil()
        {
            return Err(PersistenceError::InvalidData(
                "projection authorization metadata is invalid".into(),
            ));
        }
        let holding_id = match Uuid::parse_str(request.data_profile_key) {
            Ok(holding_id) => holding_id,
            Err(_) => sqlx::query_scalar::<_, Uuid>(
                "SELECT canonical_holding_id
                 FROM erpx_control.holding_summary_projection
                 WHERE holding_id = $1
                   AND configuration_state = 'configured'",
            )
            .bind(request.holding_id)
            .fetch_optional(&self.control.pools.write)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "holding projection holding authority is unavailable".into(),
                )
            })?,
        };
        let scope = Scope::Holding { holding_id };
        if !self
            .control
            .principal_has_role(request.actor_ref, &scope, request.permission, request.at)
            .await?
        {
            return Err(PersistenceError::InvalidData(
                "projection authorization denied".into(),
            ));
        }
        let action = format!("projection.scheduler.{}", request.operation);
        let payload = serde_json::json!({
            "holding_id": request.holding_id,
            "data_profile_key": request.data_profile_key,
            "permission": request.permission,
            "operation": request.operation,
        });
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, action, purpose, correlation_id, recorded_at, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(request.actor_ref)
        .bind(action)
        .bind(request.purpose)
        .bind(request.correlation_id)
        .bind(request.at)
        .bind(payload)
        .execute(&self.control.pools.write)
        .await?;
        Ok(())
    }
}

fn validate_projection_actor_reference(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value.chars().any(|character| {
            character.is_control() || character.is_whitespace() || matches!(character, '/' | '\\')
        })
    {
        return Err(PersistenceError::InvalidData(
            "projection actor reference must be namespaced, path-safe, and non-empty".into(),
        ));
    }
    Ok(())
}
