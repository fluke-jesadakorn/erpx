//! Durable directory feed application seam.
//!
//! External connector adapters own the wire protocol and TLS connection. The
//! worker owns bounded page validation and delegates the atomic SQLx write to
//! `SqlxDirectoryStore`; a failed page is never acknowledged as processed.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use erpx_identity::directory::feeds::{FeedCursor, FeedKind};
use erpx_identity::directory::sync::{
    ConnectorScope, DirectoryConnector as DurableDirectoryConnector, DirectoryPage,
    DirectorySyncStore, DirectoryUserRecord, DirectoryUserState, FeedVendor, ImmutableIdentityLink,
    ImportConfig, ImportPlan, ImportState, SqlxDirectorySyncStore, TransportSecurity,
};
use erpx_identity::{
    DirectoryChange, DirectoryConnector, DirectoryError, DirectoryFeedKind, DirectoryProvider,
    DirectoryRoleMapping, DirectorySyncCursor, DirectorySyncPage, SqlxDirectoryStore,
};
use sqlx::PgPool;
use std::fmt::Display;
use thiserror::Error;
use uuid::Uuid;

use crate::directory_live;
use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const DIRECTORY_SYNC_HANDLER_NAME: &str = "directory_sync";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectorySyncRunReport {
    pub changes_seen: usize,
    pub revocations_seen: usize,
    pub sequence: u64,
}

#[derive(Debug, Error)]
pub enum DirectorySyncWorkerError {
    #[error("directory connector validation failed: {0}")]
    Connector(#[from] DirectoryError),
    #[error("directory page was produced for the wrong connector")]
    ConnectorMismatch,
    #[error("directory page cursor is stale")]
    StaleCursor,
    #[error("directory page feed does not match the configured connector")]
    FeedMismatch,
    #[error("directory connector is disabled")]
    ConnectorDisabled,
}

#[derive(Debug, Clone)]
pub struct DirectorySyncWorker {
    store: SqlxDirectoryStore,
    durable_imports: SqlxDirectorySyncStore,
}

/// Explicit executable registration for directory synchronization. The
/// protocol adapter is selected only by deployment configuration; the worker
/// refuses to claim queued imports when no native LDAP/AD/UCS adapter is
/// available, preventing an in-memory or mock connector from being treated as
/// production evidence.
#[derive(Debug, Clone)]
pub struct DirectorySyncRuntimeHandler {
    worker: DirectorySyncWorker,
    connector_id: Uuid,
    adapter_name: String,
    full_import: bool,
}

impl DirectorySyncRuntimeHandler {
    pub fn from_env(pool: PgPool) -> Result<Self, WorkerRuntimeError> {
        let connector_id = std::env::var("ERPX_DIRECTORY_CONNECTOR_ID")
            .map_err(|_| {
                WorkerRuntimeError::Configuration("ERPX_DIRECTORY_CONNECTOR_ID is required".into())
            })?
            .parse::<Uuid>()
            .map_err(|_| {
                WorkerRuntimeError::Configuration("ERPX_DIRECTORY_CONNECTOR_ID is invalid".into())
            })?;
        let adapter_name = std::env::var("ERPX_DIRECTORY_FEED_ADAPTER")
            .map_err(|_| WorkerRuntimeError::Configuration("ERPX_DIRECTORY_FEED_ADAPTER is required (ldap_polling, ad_dirsync, rfc4533, or ucs_provisioning_service)".into()))?;
        if !matches!(
            adapter_name.as_str(),
            "ldap_polling" | "ad_dirsync" | "rfc4533" | "ucs_provisioning_service"
        ) {
            return Err(WorkerRuntimeError::Configuration(
                "ERPX_DIRECTORY_FEED_ADAPTER is unsupported".into(),
            ));
        }
        let full_import = std::env::var("ERPX_DIRECTORY_FULL_IMPORT")
            .map(|value| match value.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(WorkerRuntimeError::Configuration(
                    "ERPX_DIRECTORY_FULL_IMPORT must be true or false".into(),
                )),
            })
            .unwrap_or(Ok(false))?;
        Ok(Self {
            worker: DirectorySyncWorker::new(pool),
            connector_id,
            adapter_name,
            full_import,
        })
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for DirectorySyncRuntimeHandler {
    fn name(&self) -> &str {
        DIRECTORY_SYNC_HANDLER_NAME
    }

    async fn run_once(&self, _worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let connector = self
            .worker
            .store
            .connector(self.connector_id)
            .await
            .map_err(|_| {
                WorkerRuntimeError::Configuration("directory connector unavailable".into())
            })?;
        ensure_connector_enabled(connector.enabled)?;
        let previous = self
            .worker
            .store
            .latest_cursor(self.connector_id)
            .await
            .map_err(|_| {
                WorkerRuntimeError::Configuration("directory checkpoint unavailable".into())
            })?;
        let role_mappings = self
            .worker
            .store
            .list_role_mappings(self.connector_id)
            .await
            .map_err(|_| {
                WorkerRuntimeError::Configuration("directory role mappings unavailable".into())
            })?;
        let active_import = self
            .worker
            .durable_imports
            .active_import_id(connector.id)
            .await
            .map_err(|_| {
                WorkerRuntimeError::Configuration("directory import checkpoint unavailable".into())
            })?;
        if self.full_import || active_import.is_some() {
            return self
                .worker
                .run_full_import(&connector, &self.adapter_name, &role_mappings)
                .await;
        }
        let mut previous = previous;
        let mut pages = 0_u64;
        let mut changes_seen = 0_u64;
        let mut published = 0_u64;
        loop {
            let live = match directory_live::fetch(
                &connector,
                &self.adapter_name,
                previous.as_ref(),
            )
            .await
            {
                Ok(page) => page,
                Err(directory_live::LiveDirectoryError::NoMessage) => {
                    if pages == 0 {
                        return Ok(WorkerHandlerReport::default());
                    }
                    break;
                }
                Err(directory_live::LiveDirectoryError::AlreadyCommitted(sequence)) => {
                    directory_live::confirm_ucs(&connector, sequence)
                        .await
                        .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
                    return Ok(WorkerHandlerReport {
                        claimed: 1,
                        processed: 0,
                        published: 1,
                        retried: 0,
                        dead_lettered: 0,
                        has_work: true,
                    });
                }
                Err(error) => return Err(WorkerRuntimeError::Configuration(error.to_string())),
            };
            if live.page.changes.is_empty() {
                let mut native_checkpoint = live.page.cursor.clone();
                if native_checkpoint.opaque_cookie.is_none() {
                    native_checkpoint.opaque_cookie = previous
                        .as_ref()
                        .and_then(|cursor| cursor.opaque_cookie.clone());
                }
                self.worker
                    .store
                    .save_sync_checkpoint(&native_checkpoint)
                    .await
                    .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
                break;
            }
            let report = self
                .worker
                .apply_page(&connector, &live.page, previous.as_ref(), &role_mappings)
                .await
                .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
            if let Some(sequence) = live.ucs_confirmation {
                directory_live::confirm_ucs(&connector, sequence)
                    .await
                    .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
            }
            changes_seen = changes_seen.saturating_add(report.changes_seen as u64);
            published = published.saturating_add(report.changes_seen as u64);
            pages = pages.saturating_add(1);
            let continue_full_import = full_import_can_continue(
                self.full_import,
                connector.feed,
                live.page.cursor.opaque_cookie.is_some(),
                pages,
                changes_seen,
                connector.max_import_users,
            );
            previous = Some(live.page.cursor);
            if !continue_full_import {
                break;
            }
        }
        Ok(WorkerHandlerReport {
            claimed: pages,
            processed: changes_seen,
            published,
            retried: 0,
            dead_lettered: 0,
            has_work: true,
        })
    }
}

impl DirectorySyncWorker {
    pub fn new(pool: PgPool) -> Self {
        Self {
            durable_imports: SqlxDirectorySyncStore::new(pool.clone()),
            store: SqlxDirectoryStore::new(pool),
        }
    }

    pub fn store(&self) -> &SqlxDirectoryStore {
        &self.store
    }

    async fn rollback_import_failure<E: Display>(
        &self,
        import_id: Uuid,
        reason: E,
    ) -> WorkerRuntimeError {
        let reason = reason.to_string();
        let rollback = self
            .durable_imports
            .rollback_import(import_id, &reason, Utc::now())
            .await;
        import_failure_with_rollback(reason, rollback)
    }

    async fn finalize_import_with_evidence(
        &self,
        plan: &ImportPlan,
        checkpoint: &erpx_identity::directory::sync::ImportCheckpoint,
        role_mappings: &[DirectoryRoleMapping],
    ) -> Result<erpx_identity::directory::sync::ImportSummary, WorkerRuntimeError> {
        let summary = match self
            .durable_imports
            .finalize_import(plan, checkpoint, role_mappings, Utc::now())
            .await
        {
            Ok(summary) => summary,
            Err(error) => {
                return Err(self.rollback_import_failure(plan.import_id(), error).await);
            }
        };
        let reconciliation = self
            .durable_imports
            .reconcile_import(plan.import_id(), Utc::now())
            .await
            .map_err(|error| {
                WorkerRuntimeError::Configuration(format!(
                    "directory import reconciliation failed after finalization: {error}"
                ))
            })?;
        if !reconciliation.is_reconciled() {
            return Err(WorkerRuntimeError::Configuration(
                "directory import reconciliation is blocked after finalization".into(),
            ));
        }
        Ok(summary)
    }

    async fn run_full_import(
        &self,
        connector: &DirectoryConnector,
        adapter_name: &str,
        role_mappings: &[DirectoryRoleMapping],
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let active_config = self
            .durable_imports
            .active_import_config(connector.id)
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        let (import_id, page_size, max_users, max_in_flight_pages) = match active_config {
            Some((import_id, page_size, max_users, max_in_flight_pages)) => {
                (Some(import_id), page_size, max_users, max_in_flight_pages)
            }
            None => {
                let page_size = std::env::var("ERPX_DIRECTORY_PAGE_SIZE")
                    .ok()
                    .map(|value| {
                        value.parse::<u32>().map_err(|_| {
                            WorkerRuntimeError::Configuration(
                                "ERPX_DIRECTORY_PAGE_SIZE is invalid".into(),
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or(500);
                if page_size == 0 || page_size > 5_000 {
                    return Err(WorkerRuntimeError::Configuration(
                        "ERPX_DIRECTORY_PAGE_SIZE is outside the permitted bound".into(),
                    ));
                }
                let page_size = page_size.min(connector.max_import_users as u32);
                (None, page_size, connector.max_import_users, 4)
            }
        };
        if max_users == 0
            || max_users > connector.max_import_users
            || u64::from(page_size) > max_users
        {
            return Err(WorkerRuntimeError::Configuration(
                "directory import page or user capacity exceeds the connector limit".into(),
            ));
        }
        let durable_connector = durable_connector(connector, page_size, max_in_flight_pages)?;
        let config = ImportConfig::new(page_size, max_in_flight_pages)
            .and_then(|config| config.with_max_users(max_users))
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        let started_at = Utc::now();
        let plan = match import_id {
            Some(import_id) => {
                ImportPlan::with_id(import_id, durable_connector, config, started_at)
            }
            None => ImportPlan::new(durable_connector, config, started_at),
        }
        .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        let mut checkpoint = self
            .durable_imports
            .begin_import(&plan)
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        if checkpoint.state() == &ImportState::Finalizing {
            let source_cursor = match self
                .durable_imports
                .latest_import_source_cursor(plan.import_id())
                .await
            {
                Ok(source_cursor) => source_cursor,
                Err(error) => {
                    return Err(self.rollback_import_failure(plan.import_id(), error).await);
                }
            };
            let native_checkpoint = match source_cursor {
                Some(source_cursor) => match native_cursor_from_feed_cursor(
                    connector,
                    source_cursor,
                    checkpoint.pages_applied(),
                ) {
                    Ok(Some(native_checkpoint)) => Some(native_checkpoint),
                    Ok(None) => {
                        return Err(self
                            .rollback_import_failure(
                                plan.import_id(),
                                "finalized directory import lost its native source cursor",
                            )
                            .await);
                    }
                    Err(error) => {
                        return Err(self.rollback_import_failure(plan.import_id(), error).await);
                    }
                },
                None => None,
            };
            let summary = self
                .finalize_import_with_evidence(&plan, &checkpoint, role_mappings)
                .await?;
            if let Some(native_checkpoint) = native_checkpoint {
                self.store
                    .save_sync_checkpoint(&native_checkpoint)
                    .await
                    .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
            }
            return Ok(WorkerHandlerReport {
                claimed: summary.pages_applied,
                processed: summary.users_seen,
                published: summary.users_seen,
                retried: 0,
                dead_lettered: 0,
                has_work: true,
            });
        }
        let mut pages = 0_u64;
        let mut users_seen = 0_u64;
        loop {
            let previous = match live_cursor_from_checkpoint(connector, &checkpoint) {
                Ok(previous) => previous,
                Err(error) => {
                    return Err(self.rollback_import_failure(plan.import_id(), error).await);
                }
            };
            let live = match directory_live::fetch_with_mode(
                connector,
                adapter_name,
                previous.as_ref(),
                true,
            )
            .await
            {
                Ok(page) => page,
                Err(error) => {
                    return Err(self.rollback_import_failure(plan.import_id(), error).await);
                }
            };
            let mut native_checkpoint = live.page.cursor.clone();
            if native_checkpoint.opaque_cookie.is_none() {
                native_checkpoint.opaque_cookie = previous
                    .as_ref()
                    .and_then(|cursor| cursor.opaque_cookie.clone());
            }
            let page = match durable_page_from_live(
                connector,
                &checkpoint,
                &live.page,
                live.end_of_snapshot,
            ) {
                Ok(page) => page,
                Err(error) => {
                    return Err(self.rollback_import_failure(plan.import_id(), error).await);
                }
            };
            let next = match checkpoint.accept_page(&page, plan.config(), Utc::now()) {
                Ok(next) => next,
                Err(error) => {
                    return Err(self.rollback_import_failure(plan.import_id(), error).await);
                }
            };
            if let Err(error) =
                validate_full_import_progress(next.pages_applied(), next.users_seen(), max_users)
            {
                return Err(self.rollback_import_failure(plan.import_id(), error).await);
            }
            let receipt = match self
                .durable_imports
                .stage_page(&plan, &checkpoint, &page, &next)
                .await
            {
                Ok(receipt) => receipt,
                Err(error) => {
                    return Err(self.rollback_import_failure(plan.import_id(), error).await);
                }
            };
            checkpoint = receipt.checkpoint;
            pages = pages.saturating_add(1);
            users_seen = users_seen.saturating_add(receipt.records_applied);
            if checkpoint.state() == &ImportState::Finalizing {
                let summary = self
                    .finalize_import_with_evidence(&plan, &checkpoint, role_mappings)
                    .await?;
                if let Some(sequence) = live.ucs_confirmation {
                    directory_live::confirm_ucs(connector, sequence)
                        .await
                        .map_err(|error| {
                            WorkerRuntimeError::Configuration(format!(
                                "UCS terminal message confirmation failed after durable finalization: {error}"
                            ))
                        })?;
                }
                self.store
                    .save_sync_checkpoint(&native_checkpoint)
                    .await
                    .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
                return Ok(WorkerHandlerReport {
                    claimed: pages,
                    processed: summary.users_seen,
                    published: summary.users_seen,
                    retried: 0,
                    dead_lettered: 0,
                    has_work: true,
                });
            }
            if let Some(sequence) = live.ucs_confirmation
                && let Err(error) = directory_live::confirm_ucs(connector, sequence).await
            {
                return Err(self.rollback_import_failure(plan.import_id(), error).await);
            }
            if users_seen >= max_users {
                let error = format!(
                    "full import reached its configured {max_users}-user bound without an end marker"
                );
                return Err(self.rollback_import_failure(plan.import_id(), error).await);
            }
        }
    }

    pub async fn apply_page(
        &self,
        connector: &DirectoryConnector,
        page: &DirectorySyncPage,
        previous: Option<&DirectorySyncCursor>,
        role_mappings: &[DirectoryRoleMapping],
    ) -> Result<DirectorySyncRunReport, DirectorySyncWorkerError> {
        connector.validate()?;
        if !connector.enabled {
            return Err(DirectorySyncWorkerError::ConnectorDisabled);
        }
        if page.connector_id != connector.id || page.cursor.connector_id != connector.id {
            return Err(DirectorySyncWorkerError::ConnectorMismatch);
        }
        if page.cursor.feed != connector.feed {
            return Err(DirectorySyncWorkerError::FeedMismatch);
        }
        if page.cursor.is_stale(Utc::now()) {
            return Err(DirectorySyncWorkerError::StaleCursor);
        }
        self.store
            .apply_sync_page(page, connector.provider, previous, role_mappings)
            .await?;
        let revocations_seen = page
            .changes
            .iter()
            .filter(|change| {
                matches!(
                    change,
                    DirectoryChange::Disable { .. } | DirectoryChange::Delete { .. }
                )
            })
            .count();
        Ok(DirectorySyncRunReport {
            changes_seen: page.changes.len(),
            revocations_seen,
            sequence: page.cursor.sequence,
        })
    }

    pub const fn handler_name() -> &'static str {
        DIRECTORY_SYNC_HANDLER_NAME
    }

    pub fn provider_for_feed(connector: &DirectoryConnector) -> DirectoryProvider {
        connector.provider
    }
}

fn import_failure_with_rollback<E: Display>(
    reason: String,
    rollback: Result<(), E>,
) -> WorkerRuntimeError {
    match rollback {
        Ok(()) => WorkerRuntimeError::Configuration(reason),
        Err(rollback_error) => WorkerRuntimeError::Configuration(format!(
            "directory import failed and rollback failed: {reason}; rollback error: {rollback_error}"
        )),
    }
}

fn ensure_connector_enabled(enabled: bool) -> Result<(), WorkerRuntimeError> {
    if !enabled {
        return Err(WorkerRuntimeError::Configuration(
            "directory connector is disabled".into(),
        ));
    }
    Ok(())
}

fn validate_full_import_progress(
    pages_applied: u64,
    users_seen: u64,
    max_users: u64,
) -> Result<(), WorkerRuntimeError> {
    if max_users == 0 {
        return Err(WorkerRuntimeError::Configuration(
            "directory import user capacity must be positive".into(),
        ));
    }
    if users_seen > max_users {
        return Err(WorkerRuntimeError::Configuration(
            "directory import exceeded its configured user bound".into(),
        ));
    }
    if pages_applied > max_users {
        return Err(WorkerRuntimeError::Configuration(
            "directory import exceeded its bounded page progress".into(),
        ));
    }
    Ok(())
}

fn full_import_can_continue(
    full_import: bool,
    feed: erpx_identity::DirectoryFeedKind,
    has_cookie: bool,
    pages: u64,
    users_seen: u64,
    max_users: u64,
) -> bool {
    full_import
        && matches!(
            feed,
            erpx_identity::DirectoryFeedKind::Polling
                | erpx_identity::DirectoryFeedKind::AdDirSync
                | erpx_identity::DirectoryFeedKind::Rfc4533
                | erpx_identity::DirectoryFeedKind::UcsProvisioningService
        )
        && has_cookie
        && pages < max_users
        && users_seen < max_users
}

fn native_cursor_from_feed_cursor(
    connector: &DirectoryConnector,
    cursor: FeedCursor,
    fallback_sequence: u64,
) -> Result<Option<DirectorySyncCursor>, WorkerRuntimeError> {
    let (feed, cookie, sequence) = match cursor {
        FeedCursor::Start | FeedCursor::EndOfSnapshot => return Ok(None),
        FeedCursor::LdapPaged(cookie) => (DirectoryFeedKind::Polling, cookie, fallback_sequence),
        FeedCursor::AdDirSync(cookie) => (DirectoryFeedKind::AdDirSync, cookie, fallback_sequence),
        FeedCursor::Rfc4533(cookie) => (DirectoryFeedKind::Rfc4533, cookie, fallback_sequence),
        FeedCursor::Ucs { token, sequence } => {
            let cookie = if token.is_empty() {
                sequence.to_be_bytes().to_vec()
            } else {
                URL_SAFE_NO_PAD.decode(token).map_err(|_| {
                    WorkerRuntimeError::Configuration(
                        "UCS durable cursor token is not compatible with the native worker".into(),
                    )
                })?
            };
            (DirectoryFeedKind::UcsProvisioningService, cookie, sequence)
        }
    };
    if feed != connector.feed || cookie.is_empty() || sequence == 0 {
        return Err(WorkerRuntimeError::Configuration(
            "directory import source cursor does not match the configured native feed".into(),
        ));
    }
    Ok(Some(DirectorySyncCursor {
        connector_id: connector.id,
        feed,
        sequence,
        opaque_cookie: Some(URL_SAFE_NO_PAD.encode(cookie)),
        observed_at: Utc::now(),
    }))
}

fn durable_connector(
    connector: &DirectoryConnector,
    page_size: u32,
    max_in_flight_pages: u16,
) -> Result<DurableDirectoryConnector, WorkerRuntimeError> {
    let vendor = match connector.provider {
        DirectoryProvider::GenericLdap => FeedVendor::GenericLdap,
        DirectoryProvider::ActiveDirectoryDomainServices => {
            FeedVendor::ActiveDirectoryDomainServices
        }
        DirectoryProvider::UniventionCorporateServer => FeedVendor::UniventionCorporateServer,
    };
    let feed = match connector.feed {
        erpx_identity::DirectoryFeedKind::Polling => FeedKind::GenericLdapPolling,
        erpx_identity::DirectoryFeedKind::AdDirSync => FeedKind::ActiveDirectoryDirSync,
        erpx_identity::DirectoryFeedKind::Rfc4533 => FeedKind::Rfc4533,
        erpx_identity::DirectoryFeedKind::UcsProvisioningService => {
            FeedKind::UcsProvisioningService
        }
    };
    let scope = match &connector.scope {
        erpx_identity::DirectoryScope::Platform => ConnectorScope::Platform,
        erpx_identity::DirectoryScope::Holding { holding_ref } => ConnectorScope::Holding(
            holding_ref
                .parse()
                .unwrap_or_else(|_| Uuid::new_v5(&Uuid::NAMESPACE_URL, holding_ref.as_bytes())),
        ),
    };
    let security = match connector.tls.mode {
        erpx_identity::DirectoryTlsMode::Ldaps => TransportSecurity::Ldaps,
        erpx_identity::DirectoryTlsMode::StartTls => TransportSecurity::StartTls,
        erpx_identity::DirectoryTlsMode::Https => TransportSecurity::Https,
    };
    DurableDirectoryConnector::new(
        connector.id,
        vendor,
        feed,
        scope,
        connector.endpoint.clone(),
        security,
        page_size,
        max_in_flight_pages,
    )
    .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))
}

fn live_cursor_from_checkpoint(
    connector: &DirectoryConnector,
    checkpoint: &erpx_identity::directory::sync::ImportCheckpoint,
) -> Result<Option<DirectorySyncCursor>, WorkerRuntimeError> {
    let cursor_matches_feed = matches!(
        (connector.feed, checkpoint.cursor()),
        (_, FeedCursor::Start)
            | (DirectoryFeedKind::Polling, FeedCursor::LdapPaged(_))
            | (DirectoryFeedKind::AdDirSync, FeedCursor::AdDirSync(_))
            | (DirectoryFeedKind::Rfc4533, FeedCursor::Rfc4533(_))
            | (
                DirectoryFeedKind::UcsProvisioningService,
                FeedCursor::Ucs { .. }
            )
    );
    if !cursor_matches_feed {
        return Err(WorkerRuntimeError::Configuration(
            "directory import checkpoint cursor does not match the configured native feed".into(),
        ));
    }
    let (cookie, sequence) = match checkpoint.cursor() {
        FeedCursor::Start => return Ok(None),
        FeedCursor::LdapPaged(cookie)
        | FeedCursor::AdDirSync(cookie)
        | FeedCursor::Rfc4533(cookie) => (cookie.clone(), checkpoint.pages_applied().max(1)),
        FeedCursor::Ucs { token, sequence } => {
            let cookie = if token.is_empty() {
                sequence.to_be_bytes().to_vec()
            } else {
                URL_SAFE_NO_PAD.decode(token).map_err(|_| {
                    WorkerRuntimeError::Configuration(
                        "UCS durable cursor token is not compatible with the native worker".into(),
                    )
                })?
            };
            (cookie, *sequence)
        }
        FeedCursor::EndOfSnapshot => {
            return Err(WorkerRuntimeError::Configuration(
                "completed directory import has no live continuation".into(),
            ));
        }
    };
    if cookie.is_empty() || sequence == 0 {
        return Err(WorkerRuntimeError::Configuration(
            "directory import checkpoint has an empty native cursor".into(),
        ));
    }
    Ok(Some(DirectorySyncCursor {
        connector_id: connector.id,
        feed: connector.feed,
        sequence,
        opaque_cookie: Some(URL_SAFE_NO_PAD.encode(cookie)),
        observed_at: Utc::now(),
    }))
}

fn durable_page_from_live(
    connector: &DirectoryConnector,
    checkpoint: &erpx_identity::directory::sync::ImportCheckpoint,
    page: &DirectorySyncPage,
    end_of_snapshot: bool,
) -> Result<DirectoryPage, WorkerRuntimeError> {
    if page.connector_id != connector.id || page.cursor.connector_id != connector.id {
        return Err(WorkerRuntimeError::Configuration(
            "full-import page is bound to another connector".into(),
        ));
    }
    if page.cursor.feed != connector.feed || page.cursor.sequence == 0 {
        return Err(WorkerRuntimeError::Configuration(
            "full-import page has an invalid provider cursor".into(),
        ));
    }
    let previous_sequence = match checkpoint.cursor() {
        FeedCursor::Start => 0,
        FeedCursor::Ucs { sequence, .. } => *sequence,
        FeedCursor::LdapPaged(_) | FeedCursor::AdDirSync(_) | FeedCursor::Rfc4533(_) => {
            checkpoint.pages_applied()
        }
        FeedCursor::EndOfSnapshot => {
            return Err(WorkerRuntimeError::Configuration(
                "full-import checkpoint is already terminal".into(),
            ));
        }
    };
    if page.cursor.sequence <= previous_sequence {
        return Err(WorkerRuntimeError::Configuration(
            "full-import page cursor did not advance".into(),
        ));
    }
    let cursor = checkpoint.cursor().clone();
    let next_cursor = (!end_of_snapshot)
        .then_some(page.cursor.opaque_cookie.as_deref())
        .flatten()
        .map(|cookie| {
            URL_SAFE_NO_PAD.decode(cookie).map_err(|_| {
                WorkerRuntimeError::Configuration("directory cursor is invalid".into())
            })
        })
        .transpose()?
        .map(|cookie| match connector.feed {
            erpx_identity::DirectoryFeedKind::Polling => FeedCursor::LdapPaged(cookie),
            erpx_identity::DirectoryFeedKind::AdDirSync => FeedCursor::AdDirSync(cookie),
            erpx_identity::DirectoryFeedKind::Rfc4533 => FeedCursor::Rfc4533(cookie),
            erpx_identity::DirectoryFeedKind::UcsProvisioningService => FeedCursor::Ucs {
                token: URL_SAFE_NO_PAD.encode(cookie),
                sequence: page.cursor.sequence,
            },
        });
    let mut records = page
        .changes
        .iter()
        .map(|change| durable_record_from_change(connector, change))
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_by(|left, right| left.external_id().cmp(right.external_id()));
    DirectoryPage::new(
        cursor,
        records,
        next_cursor.clone(),
        end_of_snapshot || next_cursor.is_none(),
        Some(page.cursor.sequence.to_string()),
    )
    .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))
}

fn durable_record_from_change(
    connector: &DirectoryConnector,
    change: &DirectoryChange,
) -> Result<DirectoryUserRecord, WorkerRuntimeError> {
    let authority = match connector.provider {
        DirectoryProvider::GenericLdap => "generic_ldap",
        DirectoryProvider::ActiveDirectoryDomainServices => "active_directory_domain_services",
        DirectoryProvider::UniventionCorporateServer => "univention_corporate_server",
    };
    let (external_id, state, login_name, email, display_name, groups, source_revision) =
        match change {
            DirectoryChange::Upsert(user) => (
                user.external_object_id.clone(),
                if user.active {
                    DirectoryUserState::Active
                } else {
                    DirectoryUserState::Disabled
                },
                Some(user.principal_ref.clone()),
                user.email.clone(),
                user.display_name.clone(),
                user.groups.iter().cloned().collect::<Vec<_>>(),
                user.source_revision,
            ),
            DirectoryChange::Disable {
                external_object_id,
                source_revision,
            } => (
                external_object_id.clone(),
                DirectoryUserState::Disabled,
                Some(external_object_id.clone()),
                None,
                None,
                Vec::new(),
                *source_revision,
            ),
            DirectoryChange::Delete {
                external_object_id,
                source_revision,
            } => (
                external_object_id.clone(),
                DirectoryUserState::Deleted,
                None,
                None,
                None,
                Vec::new(),
                *source_revision,
            ),
        };
    let mut record = DirectoryUserRecord::builder(
        external_id.clone(),
        ImmutableIdentityLink::new(
            authority,
            canonical_immutable_value(connector.provider, &external_id),
        )
        .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?,
    )
    .state(state)
    .groups(groups)
    .source_version(source_revision.to_string());
    if let Some(login_name) = login_name {
        record = record.login_name(login_name);
    }
    if let Some(email) = email {
        record = record.email(email);
    }
    if let Some(display_name) = display_name {
        record = record.display_name(display_name);
    }
    record
        .build()
        .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))
}

fn canonical_immutable_value(provider: DirectoryProvider, value: &str) -> String {
    if let Ok(uuid) = value.parse::<Uuid>() {
        return uuid.hyphenated().to_string();
    }
    let Ok(bytes) = hex::decode(value) else {
        return value.to_owned();
    };
    if bytes.len() != 16 {
        return value.to_owned();
    }
    let Ok(bytes) = <[u8; 16]>::try_from(bytes) else {
        return value.to_owned();
    };
    let uuid = if provider == DirectoryProvider::ActiveDirectoryDomainServices {
        Uuid::from_bytes_le(bytes)
    } else {
        Uuid::from_bytes(bytes)
    };
    uuid.hyphenated().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_identity::directory::sync::ImportCheckpoint;

    #[test]
    fn handler_name_is_stable_and_provider_is_not_inferred() {
        assert_eq!(DirectorySyncWorker::handler_name(), "directory_sync");
        let _ = DirectorySyncWorker::provider_for_feed;
    }

    #[test]
    fn full_import_continuation_covers_every_native_feed() {
        for feed in [
            erpx_identity::DirectoryFeedKind::Polling,
            erpx_identity::DirectoryFeedKind::AdDirSync,
            erpx_identity::DirectoryFeedKind::Rfc4533,
            erpx_identity::DirectoryFeedKind::UcsProvisioningService,
        ] {
            assert!(full_import_can_continue(
                true, feed, true, 1, 500, 1_000_000
            ));
        }
        assert!(!full_import_can_continue(
            true,
            erpx_identity::DirectoryFeedKind::Rfc4533,
            false,
            1,
            500,
            1_000_000
        ));
        assert!(!full_import_can_continue(
            true,
            erpx_identity::DirectoryFeedKind::Rfc4533,
            true,
            1_000_000,
            1_000_000,
            1_000_000
        ));
    }

    #[test]
    fn finalized_import_source_cursor_is_rebuilt_without_cross_feed_fallback() {
        let connector = DirectoryConnector {
            id: Uuid::from_u128(10),
            provider: DirectoryProvider::GenericLdap,
            scope: erpx_identity::DirectoryScope::Platform,
            endpoint: "ldaps://ldap.example.test:636".into(),
            base_dn: "dc=example,dc=test".into(),
            tls: erpx_identity::DirectoryTlsPolicy {
                mode: erpx_identity::DirectoryTlsMode::Ldaps,
                ca_certificate_sha256: "a".repeat(64),
                server_name: "ldap.example.test".into(),
                minimum_tls_version: "1.2".into(),
            },
            feed: DirectoryFeedKind::Rfc4533,
            credential_ref: "directory-secret".into(),
            enabled: true,
            max_import_users: 1_000_000,
            revision: 1,
        };
        let checkpoint =
            native_cursor_from_feed_cursor(&connector, FeedCursor::Rfc4533(vec![1, 2, 3]), 7)
                .expect("native source cursor must be accepted")
                .expect("non-terminal source cursor must be present");
        assert_eq!(checkpoint.connector_id, connector.id);
        assert_eq!(checkpoint.feed, DirectoryFeedKind::Rfc4533);
        assert_eq!(checkpoint.sequence, 7);
        assert_eq!(
            checkpoint.opaque_cookie,
            Some(URL_SAFE_NO_PAD.encode([1_u8, 2, 3]))
        );

        assert!(
            native_cursor_from_feed_cursor(&connector, FeedCursor::Start, 7)
                .expect("empty import has no native cursor")
                .is_none()
        );
        assert!(
            native_cursor_from_feed_cursor(&connector, FeedCursor::AdDirSync(vec![1, 2, 3]), 7)
                .is_err()
        );
    }

    #[test]
    fn full_import_resume_preserves_the_provider_ucs_sequence() {
        let connector_id = Uuid::from_u128(11);
        let connector = DirectoryConnector {
            id: connector_id,
            provider: DirectoryProvider::UniventionCorporateServer,
            scope: erpx_identity::DirectoryScope::Platform,
            endpoint: "https://ucs.example.test:443".into(),
            base_dn: "dc=example,dc=test".into(),
            tls: erpx_identity::DirectoryTlsPolicy {
                mode: erpx_identity::DirectoryTlsMode::Https,
                ca_certificate_sha256: "a".repeat(64),
                server_name: "ucs.example.test".into(),
                minimum_tls_version: "1.2".into(),
            },
            feed: erpx_identity::DirectoryFeedKind::UcsProvisioningService,
            credential_ref: "ucs-secret".into(),
            enabled: true,
            max_import_users: 1_000_000,
            revision: 1,
        };
        let durable = durable_connector(&connector, 10, 1).expect("durable connector");
        let config = ImportConfig::new(10, 1).expect("import config");
        let plan = ImportPlan::new(durable, config, Utc::now()).expect("import plan");
        let checkpoint = ImportCheckpoint::initial(&plan);
        let record = DirectoryUserRecord::builder(
            "ucs-user-1",
            ImmutableIdentityLink::new("univention_corporate_server", "ucs-user-1")
                .expect("immutable link"),
        )
        .login_name("alice")
        .build()
        .expect("record");
        let page = DirectoryPage::new(
            FeedCursor::Start,
            vec![record],
            Some(FeedCursor::Ucs {
                token: URL_SAFE_NO_PAD.encode("ucs-token-41"),
                sequence: 41,
            }),
            false,
            Some("41".into()),
        )
        .expect("page");
        let checkpoint = checkpoint
            .accept_page(&page, plan.config(), Utc::now())
            .expect("checkpoint");
        let live = live_cursor_from_checkpoint(&connector, &checkpoint)
            .expect("live cursor")
            .expect("continuation");
        assert_eq!(live.sequence, 41);
        assert_eq!(live.connector_id, connector_id);
        assert_eq!(
            live.feed,
            erpx_identity::DirectoryFeedKind::UcsProvisioningService
        );
    }

    #[test]
    fn empty_ucs_full_import_page_advances_to_terminal_checkpoint() {
        let connector_id = Uuid::from_u128(12);
        let connector = DirectoryConnector {
            id: connector_id,
            provider: DirectoryProvider::UniventionCorporateServer,
            scope: erpx_identity::DirectoryScope::Platform,
            endpoint: "https://ucs.example.test:443".into(),
            base_dn: "dc=example,dc=test".into(),
            tls: erpx_identity::DirectoryTlsPolicy {
                mode: erpx_identity::DirectoryTlsMode::Https,
                ca_certificate_sha256: "a".repeat(64),
                server_name: "ucs.example.test".into(),
                minimum_tls_version: "1.2".into(),
            },
            feed: erpx_identity::DirectoryFeedKind::UcsProvisioningService,
            credential_ref: "ucs-secret".into(),
            enabled: true,
            max_import_users: 1_000_000,
            revision: 1,
        };
        let durable = durable_connector(&connector, 10, 1).expect("durable connector");
        let config = ImportConfig::new(10, 1).expect("import config");
        let plan = ImportPlan::new(durable, config, Utc::now()).expect("import plan");
        let checkpoint = ImportCheckpoint::initial(&plan);
        let live_page = DirectorySyncPage {
            connector_id,
            cursor: DirectorySyncCursor {
                connector_id,
                feed: DirectoryFeedKind::UcsProvisioningService,
                sequence: 1,
                opaque_cookie: None,
                observed_at: Utc::now(),
            },
            changes: Vec::new(),
        };

        let durable_page = durable_page_from_live(&connector, &checkpoint, &live_page, true)
            .expect("empty terminal page");
        assert!(durable_page.end_of_snapshot());
        assert!(durable_page.records().is_empty());
        let next = checkpoint
            .accept_page(&durable_page, plan.config(), Utc::now())
            .expect("terminal checkpoint");
        assert_eq!(next.state(), &ImportState::Finalizing);
        assert_eq!(next.cursor(), &FeedCursor::EndOfSnapshot);
    }

    #[test]
    fn first_non_ucs_full_import_page_accepts_native_sequence_one() {
        let connector_id = Uuid::from_u128(13);
        let connector = DirectoryConnector {
            id: connector_id,
            provider: DirectoryProvider::GenericLdap,
            scope: erpx_identity::DirectoryScope::Platform,
            endpoint: "ldaps://ldap.example.test:636".into(),
            base_dn: "dc=example,dc=test".into(),
            tls: erpx_identity::DirectoryTlsPolicy {
                mode: erpx_identity::DirectoryTlsMode::Ldaps,
                ca_certificate_sha256: "a".repeat(64),
                server_name: "ldap.example.test".into(),
                minimum_tls_version: "1.2".into(),
            },
            feed: DirectoryFeedKind::Polling,
            credential_ref: "directory-secret".into(),
            enabled: true,
            max_import_users: 1_000_000,
            revision: 1,
        };
        let durable = durable_connector(&connector, 10, 1).expect("durable connector");
        let config = ImportConfig::new(10, 1).expect("import config");
        let plan = ImportPlan::new(durable, config, Utc::now()).expect("import plan");
        let checkpoint = ImportCheckpoint::initial(&plan);
        let live_page = DirectorySyncPage {
            connector_id,
            cursor: DirectorySyncCursor {
                connector_id,
                feed: DirectoryFeedKind::Polling,
                sequence: 1,
                opaque_cookie: Some(URL_SAFE_NO_PAD.encode([1_u8, 2, 3])),
                observed_at: Utc::now(),
            },
            changes: Vec::new(),
        };

        let durable_page = durable_page_from_live(&connector, &checkpoint, &live_page, true)
            .expect("first native page must be accepted");
        let next = checkpoint
            .accept_page(&durable_page, plan.config(), Utc::now())
            .expect("terminal checkpoint");
        assert_eq!(next.state(), &ImportState::Finalizing);
    }

    #[test]
    fn import_failure_surfaces_rollback_failure_without_claiming_success() {
        let original = "native directory response was rejected".to_owned();
        let rollback = Err("database unavailable");

        let error = import_failure_with_rollback(original, rollback);

        assert!(
            error
                .to_string()
                .contains("native directory response was rejected")
        );
        assert!(error.to_string().contains("rollback failed"));
        assert!(error.to_string().contains("database unavailable"));
    }

    #[test]
    fn import_failure_keeps_original_reason_after_successful_rollback() {
        let error =
            import_failure_with_rollback("page cursor regressed".to_owned(), Ok::<(), &str>(()));

        assert_eq!(
            error.to_string(),
            "worker configuration error: page cursor regressed"
        );
    }

    #[test]
    fn disabled_connectors_are_rejected_before_full_import_resume() {
        assert!(ensure_connector_enabled(true).is_ok());
        let error = ensure_connector_enabled(false).expect_err("disabled connector must fail");
        assert_eq!(
            error.to_string(),
            "worker configuration error: directory connector is disabled"
        );
    }

    #[test]
    fn full_import_progress_rejects_unbounded_empty_page_streams() {
        assert!(validate_full_import_progress(1_000_000, 0, 1_000_000).is_ok());
        assert!(validate_full_import_progress(1_000_001, 0, 1_000_000).is_err());
        assert!(validate_full_import_progress(1, 1_000_001, 1_000_000).is_err());
        assert!(validate_full_import_progress(1, 0, 0).is_err());
    }

    #[test]
    fn resume_rejects_a_checkpoint_cursor_from_another_native_feed() {
        let connector = DirectoryConnector {
            id: Uuid::from_u128(21),
            provider: DirectoryProvider::GenericLdap,
            scope: erpx_identity::DirectoryScope::Platform,
            endpoint: "ldaps://ldap.example.test:636".into(),
            base_dn: "dc=example,dc=test".into(),
            tls: erpx_identity::DirectoryTlsPolicy {
                mode: erpx_identity::DirectoryTlsMode::Ldaps,
                ca_certificate_sha256: "a".repeat(64),
                server_name: "ldap.example.test".into(),
                minimum_tls_version: "1.2".into(),
            },
            feed: DirectoryFeedKind::Rfc4533,
            credential_ref: "directory-secret".into(),
            enabled: true,
            max_import_users: 1_000_000,
            revision: 1,
        };
        let durable = durable_connector(&connector, 10, 1).expect("durable connector");
        let config = ImportConfig::new(10, 1).expect("import config");
        let plan = ImportPlan::new(durable, config, Utc::now()).expect("import plan");
        let checkpoint = ImportCheckpoint::initial(&plan)
            .accept_page(
                &DirectoryPage::new(
                    FeedCursor::Start,
                    vec![
                        DirectoryUserRecord::builder(
                            "checkpoint-user",
                            ImmutableIdentityLink::new("generic_ldap", "checkpoint-user")
                                .expect("immutable link"),
                        )
                        .login_name("checkpoint-user")
                        .build()
                        .expect("checkpoint record"),
                    ],
                    Some(FeedCursor::Rfc4533(vec![1, 2, 3])),
                    false,
                    Some("1".into()),
                )
                .expect("checkpoint page"),
                plan.config(),
                Utc::now(),
            )
            .expect("checkpoint");
        let mut wrong_feed = connector;
        wrong_feed.feed = DirectoryFeedKind::AdDirSync;
        let error = live_cursor_from_checkpoint(&wrong_feed, &checkpoint)
            .expect_err("cross-feed checkpoint must fail closed");
        assert!(error.to_string().contains("does not match"));
    }
}
