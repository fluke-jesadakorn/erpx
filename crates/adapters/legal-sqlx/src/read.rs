use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_legal::ports::{
    AccessEntryView, AccessHistoryFilter, AccessHistoryList, ContractList, ContractListFilter,
    ContractView, DocumentList, DocumentListFilter, DocumentView, MatterView, ReadProjection,
};
use erpx_legal::{LegalContractId, LegalDocumentId, LegalMatterId, PortError, ReadRepository};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    ACCESS_PROJECTION, CONTRACT_PROJECTION, DOCUMENT_PROJECTION, MATTER_PROJECTION,
    contract_partition, document_partition, matter_partition, projection::contract_status_name,
};

#[derive(Debug, Clone)]
pub struct LegalReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl LegalReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }
    pub fn with_waiter_config(pool: ReadPool, config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), config)?,
            pool,
        })
    }

    async fn checkpoint(
        &self,
        projection: &str,
        consistency: &QueryConsistency,
        exact: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact {
                Some(partition) => self
                    .read_side
                    .checkpoint(projection, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                if requirement.projection != projection
                    || !valid_partition(projection, &requirement.partition)
                    || exact.is_some_and(|partition| partition != requirement.partition)
                {
                    return Err(PortError::NotReady(ProjectionNotReady {
                        projection: requirement.projection.clone(),
                        partition: requirement.partition.clone(),
                        target: requirement.target,
                        observed: None,
                    }));
                }
                self.waiter
                    .wait_until_checked(requirement)
                    .await
                    .map(Some)
                    .map_err(|error| match error {
                        WaitError::NotReady(value) => PortError::NotReady(value),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }

    async fn metadata(
        &self,
        projection: &str,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM erpx_cqrs_read.projection_generations WHERE projection_name=$1 AND status IN ('active','building') ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1")
            .bind(projection).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint
            .as_ref()
            .and_then(|value| {
                Utc::now()
                    .signed_duration_since(value.updated_at)
                    .to_std()
                    .ok()
            })
            .and_then(|value| u64::try_from(value.as_millis()).ok());
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }
}

#[async_trait]
impl ReadRepository for LegalReadRepository {
    async fn get_contract(
        &self,
        authority: &TenantScope,
        id: LegalContractId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ContractView>>, PortError> {
        let partition = contract_partition(id);
        let checkpoint = self
            .checkpoint(CONTRACT_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT item.state FROM erpx_legal_read.contract_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.contract_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(CONTRACT_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform)
            .bind(&scope.exact).bind(&scope.prefixes).fetch_optional(self.pool.as_pg_pool()).await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value.as_ref().is_some_and(|value: &ContractView| {
            value.contract_id != id || !authority.contains(&value.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(CONTRACT_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_contracts(
        &self,
        authority: &TenantScope,
        filter: &ContractListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ContractList>, PortError> {
        let checkpoint = self
            .checkpoint(CONTRACT_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(contract_status_name);
        let rows = sqlx::query("SELECT item.state FROM erpx_legal_read.contract_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::text IS NULL OR item.contract_status=$5) AND ($6::uuid IS NULL OR item.contract_id>$6) ORDER BY item.contract_id LIMIT $7")
            .bind(CONTRACT_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes)
            .bind(status).bind(after).bind(i64::from(filter.page_size) + 1)
            .fetch_all(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<ContractView>(rows)?;
        validate_scopes(authority, values.iter().map(|value| &value.scope))?;
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |value| {
            value.contract_id.as_uuid()
        });
        Ok(ReadProjection {
            value: ContractList {
                contracts: values,
                next_cursor,
            },
            metadata: self.metadata(CONTRACT_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_document(
        &self,
        authority: &TenantScope,
        id: LegalDocumentId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<DocumentView>>, PortError> {
        let partition = document_partition(id);
        let checkpoint = self
            .checkpoint(DOCUMENT_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT item.state FROM erpx_legal_read.document_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.document_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(DOCUMENT_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform)
            .bind(&scope.exact).bind(&scope.prefixes).fetch_optional(self.pool.as_pg_pool()).await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        // This post-decode scope defense runs before object_ref-bearing versions leave the adapter.
        if value.as_ref().is_some_and(|value: &DocumentView| {
            value.document_id != id || !authority.contains(&value.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(DOCUMENT_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_documents(
        &self,
        authority: &TenantScope,
        filter: &DocumentListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<DocumentList>, PortError> {
        let checkpoint = self
            .checkpoint(DOCUMENT_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let rows = sqlx::query("SELECT item.state FROM erpx_legal_read.document_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::bool IS NULL OR item.archived=$5) AND ($6::uuid IS NULL OR item.document_id>$6) ORDER BY item.document_id LIMIT $7")
            .bind(DOCUMENT_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes)
            .bind(filter.archived).bind(after).bind(i64::from(filter.page_size) + 1)
            .fetch_all(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<DocumentView>(rows)?;
        validate_scopes(authority, values.iter().map(|value| &value.scope))?;
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |value| {
            value.document_id.as_uuid()
        });
        Ok(ReadProjection {
            value: DocumentList {
                documents: values,
                next_cursor,
            },
            metadata: self.metadata(DOCUMENT_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_access_history(
        &self,
        authority: &TenantScope,
        filter: &AccessHistoryFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<AccessHistoryList>, PortError> {
        let checkpoint = self
            .checkpoint(ACCESS_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let document = filter.document_id.map(|value| value.as_uuid());
        let rows = sqlx::query("SELECT item.state FROM erpx_legal_read.access_history_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::uuid IS NULL OR item.document_id=$5) AND ($6::uuid IS NULL OR item.access_entry_id>$6) ORDER BY item.access_entry_id LIMIT $7")
            .bind(ACCESS_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes)
            .bind(document).bind(after).bind(i64::from(filter.page_size) + 1)
            .fetch_all(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<AccessEntryView>(rows)?;
        // Purpose and actor evidence are released only after decoded-scope validation.
        validate_scopes(authority, values.iter().map(|value| &value.scope))?;
        if let Some(document_id) = filter.document_id
            && values.iter().any(|value| value.document_id != document_id)
        {
            return Err(PortError::Internal);
        }
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |value| {
            value.access_entry_id.as_uuid()
        });
        Ok(ReadProjection {
            value: AccessHistoryList {
                entries: values,
                next_cursor,
            },
            metadata: self.metadata(ACCESS_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_matter(
        &self,
        authority: &TenantScope,
        id: LegalMatterId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<MatterView>>, PortError> {
        let partition = matter_partition(id);
        let checkpoint = self
            .checkpoint(MATTER_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT item.state FROM erpx_legal_read.matter_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.matter_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(MATTER_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform)
            .bind(&scope.exact).bind(&scope.prefixes).fetch_optional(self.pool.as_pg_pool()).await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value.as_ref().is_some_and(|value: &MatterView| {
            value.matter_id != id || !authority.contains(&value.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(MATTER_PROJECTION, checkpoint).await?,
        })
    }
}

pub fn encode_cursor(id: Uuid) -> String {
    id.to_string()
}
pub fn decode_cursor(value: &str) -> Result<Uuid, PortError> {
    Uuid::parse_str(value)
        .map_err(|_| PortError::Internal)
        .and_then(|id| {
            if id.is_nil() {
                Err(PortError::Internal)
            } else {
                Ok(id)
            }
        })
}
fn decode_state<T: serde::de::DeserializeOwned>(
    row: sqlx::postgres::PgRow,
) -> Result<T, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}
fn decode_rows<T: serde::de::DeserializeOwned>(
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<Vec<T>, PortError> {
    rows.into_iter().map(decode_state).collect()
}
fn truncate_cursor<T>(
    values: &mut Vec<T>,
    page_size: u16,
    id: impl Fn(&T) -> Uuid,
) -> Option<String> {
    if values.len() > usize::from(page_size) {
        values.truncate(usize::from(page_size));
        values.last().map(|value| encode_cursor(id(value)))
    } else {
        None
    }
}
fn validate_scopes<'a>(
    authority: &TenantScope,
    scopes: impl Iterator<Item = &'a TenantScope>,
) -> Result<(), PortError> {
    if scopes.into_iter().all(|scope| authority.contains(scope)) {
        Ok(())
    } else {
        Err(PortError::Internal)
    }
}
fn valid_partition(projection: &str, partition: &str) -> bool {
    let prefix = match projection {
        CONTRACT_PROJECTION => "legal.contract:",
        DOCUMENT_PROJECTION => "legal.document:",
        ACCESS_PROJECTION => "legal.access:",
        MATTER_PROJECTION => "legal.matter:",
        _ => return false,
    };
    partition
        .strip_prefix(prefix)
        .is_some_and(|value| value.len() == 32 && u128::from_str_radix(value, 16).is_ok())
}

struct ScopeFilter {
    platform: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
}
impl ScopeFilter {
    fn from(scope: &TenantScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            TenantScope::Platform => Self {
                platform: true,
                exact: vec![],
                prefixes: vec![],
            },
            TenantScope::Holding { holding_id } => Self {
                platform: false,
                exact: vec![],
                prefixes: vec![format!("h:{holding_id}/%")],
            },
            TenantScope::Company {
                holding_id,
                company_id,
            } => {
                let key = format!("h:{holding_id}/c:{company_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                }
            }
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                platform: false,
                exact: vec![format!("h:{holding_id}/c:{company_id}/b:{branch_id}")],
                prefixes: vec![],
            },
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Self {
                platform: false,
                exact: company_ids
                    .iter()
                    .flat_map(|company| {
                        if branch_ids.is_empty() {
                            vec![format!("h:{holding_id}/c:{company}")]
                        } else {
                            branch_ids
                                .iter()
                                .map(|branch| format!("h:{holding_id}/c:{company}/b:{branch}"))
                                .collect()
                        }
                    })
                    .collect(),
                prefixes: vec![],
            },
        })
    }
}
fn map_read_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_and_partition_types_are_bounded() {
        let id = Uuid::from_u128(8);
        assert_eq!(decode_cursor(&encode_cursor(id)).unwrap(), id);
        let contract = contract_partition(LegalContractId::try_from_uuid(id).unwrap());
        assert!(valid_partition(CONTRACT_PROJECTION, &contract));
        assert!(!valid_partition(DOCUMENT_PROJECTION, &contract));
    }
    #[test]
    fn sensitive_views_are_post_validated_against_authority() {
        let source = include_str!("read.rs");
        assert!(source.matches("validate_scopes(authority").count() >= 3);
        assert!(source.contains("!authority.contains(&value.scope)"));
        assert!(source.contains("Purpose and actor evidence"));
    }
}
