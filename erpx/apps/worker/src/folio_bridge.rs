//! Coordinator-owned Folio source bridge.
//!
//! This handler performs one bounded, read-only source extraction, persists
//! its redacted evidence on the ERPX control-write side, and queues immutable
//! backfill scopes. It deliberately does not write Folio, mutate ERPX tenant
//! business aggregates, advance a cutover phase, or claim migration work
//! before a context-specific target executor is installed.

use std::{collections::BTreeSet, env, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{
    G13_EXECUTE_PERMISSION, G13AuthorizationProof, G13SourceSnapshotRecord, SqlxControlPlane,
    SqlxG13MigrationStore,
};
use erpx_migration::{FolioSourceExtractor, FolioSourceSnapshot, g13::content_hash};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const G13_FOLIO_BRIDGE_HANDLER_NAME: &str = "g13_folio_bridge";
const SOURCE_READ_ONLY_SESSION_SQL: &str = "SET default_transaction_read_only = on";

#[derive(Debug, Error)]
enum FolioBridgeError {
    #[error("{0} is required")]
    MissingConfiguration(&'static str),
    #[error(
        "Folio source database URL must be bounded and require PostgreSQL TLS certificate and hostname verification"
    )]
    InsecureSourceUrl,
    #[error("Folio source database connection failed")]
    SourceConnection,
    #[error("Folio source extraction failed")]
    Extraction,
    #[error("Folio source snapshot checksum mismatch")]
    SnapshotChecksum,
    #[error("G13 Folio bridge persistence failed")]
    Persistence,
}

#[derive(Clone)]
pub struct FolioBridgeRuntimeHandler {
    control: Arc<SqlxControlPlane>,
    source_database_url: String,
    run_id: Uuid,
    cohort: String,
    id_namespace: String,
    actor_ref: String,
    break_glass: bool,
}

impl FolioBridgeRuntimeHandler {
    pub fn from_env(control: Arc<SqlxControlPlane>) -> Result<Self, WorkerRuntimeError> {
        let source_database_url = env::var("ERPX_G13_SOURCE_DATABASE_URL").map_err(|_| {
            config(FolioBridgeError::MissingConfiguration(
                "ERPX_G13_SOURCE_DATABASE_URL",
            ))
        })?;
        validate_source_database_url(&source_database_url)?;
        let run_id = env::var("ERPX_G13_RUN_ID")
            .map_err(|_| config(FolioBridgeError::MissingConfiguration("ERPX_G13_RUN_ID")))?
            .parse::<Uuid>()
            .map_err(|_| config(FolioBridgeError::MissingConfiguration("ERPX_G13_RUN_ID")))?;
        let cohort = bounded_env("ERPX_G13_COHORT")?;
        let id_namespace = bounded_env("ERPX_G13_ID_NAMESPACE")?;
        let break_glass = env::var("ERPX_G13_BRIDGE_BREAK_GLASS")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let actor_ref = resolve_actor_ref(env::var("ERPX_G13_BRIDGE_ACTOR_REF").ok(), break_glass)
            .map_err(|_| {
                config(FolioBridgeError::MissingConfiguration(
                    "ERPX_G13_BRIDGE_ACTOR_REF",
                ))
            })?;
        if !valid_actor_ref(&actor_ref) {
            return Err(config(FolioBridgeError::MissingConfiguration(
                "ERPX_G13_BRIDGE_ACTOR_REF",
            )));
        }
        if !env::var("ERPX_G13_BRIDGE_MFA_VERIFIED")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return Err(config(FolioBridgeError::MissingConfiguration(
                "ERPX_G13_BRIDGE_MFA_VERIFIED=true",
            )));
        }
        Ok(Self {
            control,
            source_database_url,
            run_id,
            cohort,
            id_namespace,
            actor_ref,
            break_glass,
        })
    }

    fn proof(&self) -> G13AuthorizationProof {
        G13AuthorizationProof {
            actor_ref: self.actor_ref.clone(),
            permissions: BTreeSet::from([G13_EXECUTE_PERMISSION.to_owned()]),
            mfa_verified: true,
            break_glass: self.break_glass,
            expires_at: Utc::now() + Duration::minutes(5),
            cutover_approval_ref: None,
        }
    }

    async fn capture(&self) -> Result<BridgeCapture, FolioBridgeError> {
        let source_pool = PgPoolOptions::new()
            .max_connections(2)
            // Keep the bridge's source boundary read-only for the entire
            // lifetime of every pooled connection. The extractor also uses a
            // transaction-level fence, but this session guard protects future
            // bridge queries that might accidentally run outside that
            // transaction.
            .after_connect(|connection, _meta| {
                Box::pin(async move {
                    sqlx::query(SOURCE_READ_ONLY_SESSION_SQL)
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&self.source_database_url)
            .await
            .map_err(|_| FolioBridgeError::SourceConnection)?;
        let extractor = FolioSourceExtractor::new(self.id_namespace.clone())
            .map_err(|_| FolioBridgeError::Extraction)?;
        let snapshot = extractor
            .extract(&source_pool)
            .await
            .map_err(|_| FolioBridgeError::Extraction)?;
        source_pool.close().await;
        // The extractor computes this hash from the complete aggregate
        // snapshot. Recompute it at the bridge boundary before any control
        // write so a corrupted or tampered extraction can never create a run,
        // snapshot record, or work item with an untrusted source fence.
        validate_snapshot_checksum(&snapshot)?;

        let proof = self.proof();
        let store = SqlxG13MigrationStore::new(Arc::clone(&self.control));
        let correlation_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "g13-source-correlation:{}:{}",
                self.run_id, snapshot.source_contract_hash
            )
            .as_bytes(),
        );
        store
            .start_run(
                &erpx_migration::g13::G13StartRunRequest {
                    run_id: self.run_id,
                    cohort: self.cohort.clone(),
                    source_contract_hash: snapshot.source_contract_hash.clone(),
                    id_namespace: self.id_namespace.clone(),
                    actor_ref: self.actor_ref.clone(),
                    correlation_id,
                    idempotency_key: format!(
                        "g13-source-run:{}:{}",
                        self.run_id, snapshot.source_contract_hash
                    ),
                    now: Utc::now(),
                },
                &proof,
            )
            .await
            .map_err(|_| FolioBridgeError::Persistence)?;
        let snapshot_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "g13-source-snapshot:{}:{}",
                self.run_id, snapshot.source_contract_hash
            )
            .as_bytes(),
        );
        let record = store
            .record_source_snapshot(
                self.run_id,
                snapshot_id,
                &snapshot,
                &self.actor_ref,
                correlation_id,
                &format!("g13-source-snapshot:{snapshot_id}"),
                &proof,
            )
            .await
            .map_err(|_| FolioBridgeError::Persistence)?;

        let mut queued = 0_u64;
        // Only source scopes with immutable IDs can be imported by the target
        // executor. Counts for tables without an identity contract remain in
        // the reconciliation report as an explicit gap; they are never
        // acknowledged as migrated through a fabricated placeholder row.
        // Every identity-bearing source scope must become durable work. A
        // fixed prefix would silently omit scopes from the authoritative
        // source snapshot and make reconciliation impossible to interpret.
        for kind in snapshot.id_mappings.keys() {
            let count = snapshot.counts.get(kind).copied().unwrap_or_default();
            let scope_key = format!("source_snapshot:{}:{}", record.snapshot_id, kind);
            let work_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, scope_key.as_bytes());
            let work_hash = content_hash(&json!({
                "run_id": self.run_id,
                "snapshot_id": record.snapshot_id,
                "work_kind": "backfill",
                "scope_key": scope_key,
                "source_contract_hash": snapshot.source_contract_hash,
                "source_count": count,
            }))
            .map_err(|_| FolioBridgeError::Persistence)?;
            if store
                .enqueue_work(
                    self.run_id,
                    work_id,
                    "backfill",
                    &scope_key,
                    &snapshot.source_contract_hash,
                    &work_hash,
                    10,
                    &self.actor_ref,
                    &proof,
                )
                .await
                .map_err(|_| FolioBridgeError::Persistence)?
            {
                queued = queued.saturating_add(1);
            }
        }
        let reconcile_scope = format!("source_snapshot:{}:reconcile", record.snapshot_id);
        let reconcile_hash = content_hash(&json!({
            "run_id": self.run_id,
            "snapshot_id": record.snapshot_id,
            "scope_key": reconcile_scope,
            "source_contract_hash": snapshot.source_contract_hash,
        }))
        .map_err(|_| FolioBridgeError::Persistence)?;
        if store
            .enqueue_work(
                self.run_id,
                Uuid::new_v5(&Uuid::NAMESPACE_URL, reconcile_scope.as_bytes()),
                "reconcile",
                &reconcile_scope,
                &snapshot.source_contract_hash,
                &reconcile_hash,
                10,
                &self.actor_ref,
                &proof,
            )
            .await
            .map_err(|_| FolioBridgeError::Persistence)?
        {
            queued = queued.saturating_add(1);
        }
        Ok(BridgeCapture { record, queued })
    }
}

struct BridgeCapture {
    record: G13SourceSnapshotRecord,
    queued: u64,
}

#[async_trait::async_trait]
impl NamedWorkerHandler for FolioBridgeRuntimeHandler {
    fn name(&self) -> &str {
        G13_FOLIO_BRIDGE_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "G13 Folio bridge worker identity is required".into(),
            ));
        }
        let capture = self
            .capture()
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        Ok(WorkerHandlerReport {
            claimed: 0,
            processed: 1,
            published: capture.queued,
            retried: 0,
            dead_lettered: 0,
            has_work: capture.queued > 0 || capture.record.total_rows > 0,
        })
    }
}

fn bounded_env(name: &'static str) -> Result<String, WorkerRuntimeError> {
    let value = env::var(name).map_err(|_| config(FolioBridgeError::MissingConfiguration(name)))?;
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(config(FolioBridgeError::MissingConfiguration(name)));
    }
    Ok(value)
}

fn resolve_actor_ref(actor_ref: Option<String>, break_glass: bool) -> Result<String, &'static str> {
    match actor_ref {
        Some(actor_ref) if actor_ref.trim().is_empty() => {
            Err("ERPX_G13_BRIDGE_ACTOR_REF must not be blank")
        }
        Some(actor_ref) if actor_ref.starts_with("local:") && !break_glass => {
            Err("local G13 bridge actors require break-glass mode")
        }
        Some(actor_ref) => Ok(actor_ref),
        None if break_glass => Ok("local:g13-folio-bridge".into()),
        None => {
            Err("ERPX_G13_BRIDGE_ACTOR_REF is required unless ERPX_G13_BRIDGE_BREAK_GLASS=true")
        }
    }
}

fn validate_source_database_url(value: &str) -> Result<(), WorkerRuntimeError> {
    if value.len() > 2048 {
        return Err(config(FolioBridgeError::InsecureSourceUrl));
    }
    let parsed =
        reqwest::Url::parse(value).map_err(|_| config(FolioBridgeError::InsecureSourceUrl))?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") || parsed.host_str().is_none() {
        return Err(config(FolioBridgeError::InsecureSourceUrl));
    }
    if parsed.fragment().is_some() {
        return Err(config(FolioBridgeError::InsecureSourceUrl));
    }
    // Require one explicit, unambiguous certificate-and-hostname verification
    // mode. `require` encrypts without verifying the server certificate and
    // `verify-ca` verifies the CA without checking the server hostname.
    let ssl_modes = parsed
        .query_pairs()
        .filter_map(|(key, value)| {
            if matches!(&*key, "sslmode" | "ssl-mode") {
                Some(value.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if ssl_modes.len() != 1 || ssl_modes[0] != "verify-full" {
        return Err(config(FolioBridgeError::InsecureSourceUrl));
    }
    Ok(())
}

fn validate_snapshot_checksum(snapshot: &FolioSourceSnapshot) -> Result<(), FolioBridgeError> {
    let mut canonical = snapshot.clone();
    canonical.generated_at = DateTime::<Utc>::UNIX_EPOCH;
    canonical.source_contract_hash.clear();
    let bytes = serde_json::to_vec(&canonical).map_err(|_| FolioBridgeError::SnapshotChecksum)?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    let expected = hex::encode(digest.finalize());
    if expected != snapshot.source_contract_hash {
        return Err(FolioBridgeError::SnapshotChecksum);
    }
    Ok(())
}

fn config(error: FolioBridgeError) -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration(error.to_string())
}

fn valid_actor_ref(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use erpx_migration::FolioSourceSnapshot;
    use sha2::Digest;

    use super::{
        SOURCE_READ_ONLY_SESSION_SQL, resolve_actor_ref, valid_actor_ref,
        validate_snapshot_checksum, validate_source_database_url,
    };

    #[test]
    fn source_pool_session_fence_is_read_only() {
        assert_eq!(
            SOURCE_READ_ONLY_SESSION_SQL,
            "SET default_transaction_read_only = on"
        );
    }

    #[test]
    fn source_database_requires_unambiguous_full_tls_verification() {
        assert!(
            validate_source_database_url(
                "postgresql://bridge:secret@folio.example/folio?sslmode=verify-full"
            )
            .is_ok()
        );
        assert!(
            validate_source_database_url(
                "postgresql://bridge:secret@folio.example/folio?sslmode=require"
            )
            .is_err()
        );
        assert!(
            validate_source_database_url(
                "postgresql://bridge:secret@folio.example/folio?sslmode=verify-ca"
            )
            .is_err()
        );
        assert!(validate_source_database_url(
            "postgresql://bridge:secret@folio.example/folio?sslmode=verify-full&sslmode=verify-full"
        )
        .is_err());
        assert!(validate_source_database_url(
            "postgresql://bridge:secret@folio.example/folio?sslmode=verify-full&ssl-mode=verify-full"
        )
        .is_err());
        assert!(
            validate_source_database_url(
                "postgresql://bridge:secret@folio.example/folio?sslmode=verify-full#fragment"
            )
            .is_err()
        );
        assert!(
            validate_source_database_url(
                "postgresql://bridge:sslmode=verify-full@folio.example/folio?sslmode=disable"
            )
            .is_err()
        );
    }

    #[test]
    fn actor_reference_rejects_control_characters_before_audit_persistence() {
        assert!(valid_actor_ref("local:g13-folio-bridge"));
        assert!(!valid_actor_ref("local:g13-folio-bridge\nforged"));
        assert!(!valid_actor_ref("local:g13-folio-bridge\u{001b}[31m"));
        assert!(!valid_actor_ref("   "));
        assert!(!valid_actor_ref(&"a".repeat(201)));
    }

    #[test]
    fn local_bridge_owner_requires_break_glass_and_missing_owner_fails_closed() {
        assert!(resolve_actor_ref(None, false).is_err());
        assert_eq!(
            resolve_actor_ref(None, true).expect("break-glass fallback"),
            "local:g13-folio-bridge"
        );
        assert!(resolve_actor_ref(Some("local:bridge".into()), false).is_err());
        assert_eq!(
            resolve_actor_ref(Some("service:g13-bridge".into()), false)
                .expect("explicit service owner"),
            "service:g13-bridge"
        );
    }

    #[test]
    fn source_snapshot_checksum_rejects_tampering_before_persistence() {
        let mut snapshot = FolioSourceSnapshot {
            extractor_version: "g13-mig-002-v1".into(),
            generated_at: Utc::now(),
            source_contract_hash: String::new(),
            missing_tables: vec!["folio.users".into()],
            counts: BTreeMap::from([(String::from("users"), 3)]),
            statuses: BTreeMap::new(),
            totals_minor: BTreeMap::from([(String::from("users_minor"), 0)]),
            object_bytes: BTreeMap::new(),
            id_mappings: BTreeMap::new(),
        };

        let mut canonical = snapshot.clone();
        canonical.generated_at = DateTime::<Utc>::UNIX_EPOCH;
        let expected = {
            let bytes = serde_json::to_vec(&canonical).expect("serialize checksum fixture");
            let mut digest = sha2::Sha256::new();
            digest.update(bytes);
            hex::encode(digest.finalize())
        };
        snapshot.source_contract_hash = expected;
        assert!(validate_snapshot_checksum(&snapshot).is_ok());

        snapshot.counts.insert("users".into(), 4);
        assert!(validate_snapshot_checksum(&snapshot).is_err());
    }
}
