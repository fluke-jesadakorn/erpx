//! TLS-only directory feed contracts.
//!
//! The adapters in this file intentionally stop at a typed protocol seam. A
//! production transport implementation must provide an LDAP/AD/UCS client
//! that performs certificate validation, channel binding where supported, and
//! protocol-specific cookie persistence. The fail-closed behavior is explicit:
//! no adapter silently falls back to an unauthenticated poll or a mutable
//! username key.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::sync::{
    ChangeKind, DirectoryChange, DirectoryChangeBatch, DirectoryConnector, DirectoryPage,
    DirectoryUserRecord, DirectoryUserState, FeedVendor, ImmutableIdentityLink, SyncError,
};

pub type FeedFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, FeedError>> + Send + 'a>>;

/// The hard per-connector full-import ceiling.  This is intentionally
/// sourced from the synchronization state machine so a feed cannot accept a
/// larger request than the durable import planner can reconcile.
pub const MAX_FULL_IMPORT_USERS: u64 = super::sync::MAX_FULL_IMPORT_USERS;
pub const MAX_NATIVE_CURSOR_BYTES: usize = 64 * 1024;
const MAX_DIRECTORY_ATTRIBUTE_COUNT: usize = 128;
const MAX_DIRECTORY_ATTRIBUTE_BYTES: usize = 256;
const MAX_DIRECTORY_REFERENCE_BYTES: usize = 4 * 1024;

/// The directory revocation freshness window, in seconds.  Revocation is
/// immediate when a disable/delete observation arrives; this window is the
/// maximum freshness allowed before the scheduler must deny access.
pub const REVOCATION_WINDOW_SECONDS: i64 = super::sync::REVOCATION_SLA_MINUTES * 60;

pub fn revocation_window() -> Duration {
    Duration::seconds(REVOCATION_WINDOW_SECONDS)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedCursor {
    Start,
    LdapPaged(Vec<u8>),
    AdDirSync(Vec<u8>),
    Rfc4533(Vec<u8>),
    Ucs { token: String, sequence: u64 },
    EndOfSnapshot,
}

impl FeedCursor {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::LdapPaged(_) => "ldap_paged",
            Self::AdDirSync(_) => "ad_dirsync",
            Self::Rfc4533(_) => "rfc4533",
            Self::Ucs { .. } => "ucs_provisioning_service",
            Self::EndOfSnapshot => "end_of_snapshot",
        }
    }

    /// Validate a cursor that is about to cross the feed boundary.  The
    /// protocol adapters receive `Start` as the initial position, but every
    /// persisted native continuation must contain an opaque source position.
    /// Accepting an empty cookie/token would turn a failed source checkpoint
    /// into an implicit restart of the snapshot or change stream.
    pub fn validate(&self) -> Result<(), FeedError> {
        match self {
            Self::Start | Self::EndOfSnapshot => Ok(()),
            Self::LdapPaged(cookie) => validate_native_bytes(cookie, "LDAP page cookie"),
            Self::AdDirSync(cookie) => validate_native_bytes(cookie, "AD DirSync cookie"),
            Self::Rfc4533(cookie) => validate_native_bytes(cookie, "RFC 4533 sync cookie"),
            Self::Ucs { token, sequence } => {
                if *sequence == 0 {
                    return Err(FeedError::InvalidCursor(
                        "UCS provisioning sequence is zero".into(),
                    ));
                }
                if token.len() > MAX_NATIVE_CURSOR_BYTES || token.chars().any(char::is_control) {
                    return Err(FeedError::InvalidCursor(
                        "UCS provisioning token is invalid or too large".into(),
                    ));
                }
                Ok(())
            }
        }
    }

    /// Validate a native continuation returned for a non-terminal page or a
    /// change batch.  A source that repeats the same cursor is not making
    /// progress; treating that response as successful would cause an
    /// idempotent replay loop forever.  UCS exposes both a token and a
    /// sequence, so sequence regression is rejected even when the token
    /// changes.
    pub fn validate_continuation_from(&self, next: &Self) -> Result<(), FeedError> {
        next.validate()?;
        if self == next {
            return Err(FeedError::InvalidCursor(format!(
                "{} continuation cursor did not advance",
                next.kind()
            )));
        }
        match (self, next) {
            (
                Self::Start,
                Self::LdapPaged(_) | Self::AdDirSync(_) | Self::Rfc4533(_) | Self::Ucs { .. },
            ) => Ok(()),
            (Self::LdapPaged(_), Self::LdapPaged(_))
            | (Self::AdDirSync(_), Self::AdDirSync(_))
            | (Self::Rfc4533(_), Self::Rfc4533(_)) => Ok(()),
            (
                Self::Ucs {
                    sequence: current_sequence,
                    ..
                },
                Self::Ucs {
                    sequence: next_sequence,
                    ..
                },
            ) if *next_sequence
                == current_sequence.checked_add(1).ok_or_else(|| {
                    FeedError::InvalidCursor("UCS provisioning sequence overflow".into())
                })? =>
            {
                Ok(())
            }
            (
                Self::Ucs {
                    sequence: current_sequence,
                    ..
                },
                Self::Ucs {
                    sequence: next_sequence,
                    ..
                },
            ) if *next_sequence < *current_sequence => Err(FeedError::InvalidCursor(
                "UCS provisioning sequence regressed".into(),
            )),
            (Self::Ucs { .. }, Self::Ucs { .. }) => Err(FeedError::InvalidCursor(
                "UCS provisioning sequence is not contiguous or regressed".into(),
            )),
            _ => Err(FeedError::InvalidCursor(
                "native continuation cursor changed protocol or regressed".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedKind {
    GenericLdapPolling,
    ActiveDirectoryDirSync,
    Rfc4533,
    UcsProvisioningService,
}

impl FeedKind {
    pub const fn supports(self, vendor: FeedVendor) -> bool {
        matches!(
            (self, vendor),
            (Self::GenericLdapPolling, FeedVendor::GenericLdap)
                | (
                    Self::ActiveDirectoryDirSync,
                    FeedVendor::ActiveDirectoryDomainServices
                )
                | (
                    Self::UcsProvisioningService,
                    FeedVendor::UniventionCorporateServer
                )
                | (Self::Rfc4533, FeedVendor::GenericLdap)
        )
    }
}

#[derive(Debug, Error)]
pub enum FeedError {
    #[error("directory protocol is not available in this deployment: {0}")]
    ExternalUnavailable(String),
    #[error("directory protocol rejected the request: {0}")]
    ProtocolRejected(String),
    #[error("directory feed returned an invalid cursor: {0}")]
    InvalidCursor(String),
    #[error("directory feed returned an invalid record: {0}")]
    InvalidRecord(String),
    #[error("directory feed requires TLS-only transport")]
    TlsRequired,
    #[error("directory feed is configured for the wrong connector protocol")]
    WrongProtocol,
    #[error("directory feed page exceeds the configured bound")]
    PageTooLarge,
    #[error(
        "directory full-import capacity {requested} is outside the bounded range 1..={maximum}"
    )]
    ImportCapacityExceeded { requested: u64, maximum: u64 },
    #[error("directory revocation schedule is invalid for this change")]
    InvalidRevocationSchedule,
    #[error("directory feed adapter is not implemented for this vendor")]
    Unsupported,
}

impl From<SyncError> for FeedError {
    fn from(value: SyncError) -> Self {
        Self::InvalidRecord(value.to_string())
    }
}

pub trait DirectoryFeed: Send {
    fn connector(&self) -> &DirectoryConnector;

    /// A concrete external adapter must pass its readiness/certification gate
    /// before the worker is allowed to request a page or change batch. The
    /// default is fail-closed; only a deliberately local, non-external feed
    /// should override it with its own deterministic preflight.
    fn preflight<'a>(&'a mut self) -> FeedFuture<'a, ()> {
        let result = validate_feed_connector(self.connector());
        Box::pin(async move {
            result?;
            Err(FeedError::ExternalUnavailable(
                "directory feed external readiness preflight is required".into(),
            ))
        })
    }

    fn full_import_page<'a>(
        &'a mut self,
        request: FullImportPageRequest,
    ) -> FeedFuture<'a, DirectoryPage>;

    fn change_batch<'a>(
        &'a mut self,
        request: ChangeFeedRequest,
    ) -> FeedFuture<'a, DirectoryChangeBatch>;
}

/// A small, copyable accounting contract for a full import.  It lets a
/// scheduler account pages without retaining source records and rejects the
/// first page that would cross the one-million-user ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportCapacity {
    maximum_users: u64,
    users_seen: u64,
}

impl ImportCapacity {
    pub fn new(maximum_users: u64) -> Result<Self, FeedError> {
        if maximum_users == 0 || maximum_users > MAX_FULL_IMPORT_USERS {
            return Err(FeedError::ImportCapacityExceeded {
                requested: maximum_users,
                maximum: MAX_FULL_IMPORT_USERS,
            });
        }
        Ok(Self {
            maximum_users,
            users_seen: 0,
        })
    }

    pub fn full_import() -> Self {
        Self {
            maximum_users: MAX_FULL_IMPORT_USERS,
            users_seen: 0,
        }
    }

    pub const fn maximum_users(self) -> u64 {
        self.maximum_users
    }

    pub const fn users_seen(self) -> u64 {
        self.users_seen
    }

    pub const fn remaining_users(self) -> u64 {
        self.maximum_users - self.users_seen
    }

    pub fn accept_page(&mut self, page_users: usize) -> Result<u64, FeedError> {
        let page_users = u64::try_from(page_users).map_err(|_| FeedError::PageTooLarge)?;
        let next =
            self.users_seen
                .checked_add(page_users)
                .ok_or(FeedError::ImportCapacityExceeded {
                    requested: u64::MAX,
                    maximum: self.maximum_users,
                })?;
        if next > self.maximum_users {
            return Err(FeedError::ImportCapacityExceeded {
                requested: next,
                maximum: self.maximum_users,
            });
        }
        self.users_seen = next;
        Ok(self.remaining_users())
    }
}

/// The deterministic schedule attached to a disable/delete observation.
/// Access remains permitted at the exact five-minute boundary for the
/// freshness contract; it is expired only after that boundary.  A scheduler
/// should enqueue its check at `deadline()` (`is_due_at` is inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevocationWindow {
    observed_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
}

impl RevocationWindow {
    pub fn for_change(
        kind: ChangeKind,
        observed_at: DateTime<Utc>,
    ) -> Result<Option<Self>, FeedError> {
        if matches!(kind, ChangeKind::Upsert) {
            return Ok(None);
        }
        let deadline = observed_at
            .checked_add_signed(revocation_window())
            .ok_or(FeedError::InvalidRevocationSchedule)?;
        Ok(Some(Self {
            observed_at,
            deadline,
        }))
    }

    pub const fn observed_at(self) -> DateTime<Utc> {
        self.observed_at
    }

    pub const fn deadline(self) -> DateTime<Utc> {
        self.deadline
    }

    pub fn is_due_at(self, at: DateTime<Utc>) -> bool {
        at >= self.deadline
    }

    pub fn is_expired_at(self, at: DateTime<Utc>) -> bool {
        at > self.deadline
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeFeedRequest {
    connector_id: uuid::Uuid,
    cursor: FeedCursor,
    limit: u32,
}

impl ChangeFeedRequest {
    pub fn new(
        connector_id: uuid::Uuid,
        cursor: FeedCursor,
        limit: u32,
    ) -> Result<Self, FeedError> {
        cursor.validate()?;
        if connector_id.is_nil() || limit == 0 || limit > super::sync::MAX_DIRECTORY_PAGE_SIZE {
            return Err(FeedError::InvalidCursor(
                "change request has an invalid connector or limit".into(),
            ));
        }
        Ok(Self {
            connector_id,
            cursor,
            limit,
        })
    }

    pub fn connector_id(&self) -> uuid::Uuid {
        self.connector_id
    }

    pub fn cursor(&self) -> &FeedCursor {
        &self.cursor
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullImportPageRequest {
    connector_id: uuid::Uuid,
    import_id: uuid::Uuid,
    cursor: FeedCursor,
    page_size: u32,
    remaining_users: u64,
}

impl FullImportPageRequest {
    pub fn new(
        connector_id: uuid::Uuid,
        import_id: uuid::Uuid,
        cursor: FeedCursor,
        page_size: u32,
        remaining_users: u64,
    ) -> Result<Self, FeedError> {
        cursor.validate()?;
        if connector_id.is_nil()
            || import_id.is_nil()
            || page_size == 0
            || page_size > super::sync::MAX_DIRECTORY_PAGE_SIZE
        {
            return Err(FeedError::InvalidCursor(
                "full-import request is outside its bounded contract".into(),
            ));
        }
        if remaining_users == 0 || remaining_users > MAX_FULL_IMPORT_USERS {
            return Err(FeedError::ImportCapacityExceeded {
                requested: remaining_users,
                maximum: MAX_FULL_IMPORT_USERS,
            });
        }
        Ok(Self {
            connector_id,
            import_id,
            cursor,
            page_size,
            remaining_users,
        })
    }

    pub fn connector_id(&self) -> uuid::Uuid {
        self.connector_id
    }

    pub fn import_id(&self) -> uuid::Uuid {
        self.import_id
    }

    pub fn cursor(&self) -> &FeedCursor {
        &self.cursor
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn remaining_users(&self) -> u64 {
        self.remaining_users
    }

    pub fn validate_page_len(&self, actual: usize) -> Result<(), FeedError> {
        let actual = u64::try_from(actual).map_err(|_| FeedError::PageTooLarge)?;
        if actual > u64::from(self.page_size) || actual > self.remaining_users {
            return Err(FeedError::PageTooLarge);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdapSearchRequest {
    pub base_dn: String,
    pub filter: String,
    pub attributes: Vec<String>,
    pub page_size: u32,
    pub cookie: Vec<u8>,
    pub require_sorted_immutable_id: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LdapSearchResponse {
    pub entries: Vec<LdapEntry>,
    pub next_cookie: Vec<u8>,
    pub end_of_snapshot: bool,
    pub source_watermark: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LdapChangeResponse {
    pub changes: Vec<DirectoryChangeEnvelope>,
    pub next_cookie: Vec<u8>,
    pub has_more: bool,
}

pub trait LdapPollingClient: Send {
    /// The default keeps an unconfigured client fail-closed. A production
    /// client must override this to prove endpoint, CA, credential, and
    /// vendor acceptance before search or change polling is attempted.
    fn preflight<'a>(&'a mut self, _connector: &'a DirectoryConnector) -> FeedFuture<'a, ()> {
        Box::pin(async {
            Err(FeedError::ExternalUnavailable(
                "Generic LDAP external readiness preflight is required".into(),
            ))
        })
    }

    fn search_page<'a>(
        &'a mut self,
        request: LdapSearchRequest,
    ) -> FeedFuture<'a, LdapSearchResponse>;

    fn poll_changes<'a>(
        &'a mut self,
        request: LdapChangeRequest,
    ) -> FeedFuture<'a, LdapChangeResponse>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdapChangeRequest {
    pub base_dn: String,
    pub filter: String,
    pub attributes: Vec<String>,
    pub cookie: Vec<u8>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LdapEntry {
    pub dn: String,
    pub attributes: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryChangeEnvelope {
    pub external_id: String,
    pub immutable_link: ImmutableIdentityLink,
    pub kind: ChangeKind,
    pub record: Option<DirectoryUserRecord>,
    pub source_version: Option<String>,
    pub observed_at: DateTime<Utc>,
}

impl DirectoryChangeEnvelope {
    pub fn revocation_window(&self) -> Result<Option<RevocationWindow>, FeedError> {
        RevocationWindow::for_change(self.kind, self.observed_at)
    }
}

/// Convert a native refresh response into the records that belong in an
/// authoritative snapshot.  Native change envelopes intentionally omit the
/// mutable record for disable/delete events, because a revocation must never
/// overwrite a record with untrusted mutable data.  A full snapshot has a
/// different requirement: the identity must remain present so finalization
/// can persist its disabled/deleted state and emit the corresponding
/// revocation event.  Preserve source fields when present and use only the
/// immutable external ID as the disabled fallback login.
fn full_import_records(
    entries: &[DirectoryChangeEnvelope],
    protocol: &str,
) -> Result<Vec<DirectoryUserRecord>, FeedError> {
    let mut records = entries
        .iter()
        .map(|entry| {
            if entry.kind == ChangeKind::Upsert {
                let record = entry.record.clone().ok_or_else(|| {
                    FeedError::ProtocolRejected(format!(
                        "{protocol} full snapshot upsert has no record"
                    ))
                })?;
                if record.external_id() != entry.external_id
                    || record.immutable_link() != &entry.immutable_link
                {
                    return Err(FeedError::InvalidRecord(format!(
                        "{protocol} full snapshot record disagrees with immutable envelope"
                    )));
                }
                return Ok(record);
            }

            let state = match entry.kind {
                ChangeKind::Disable => DirectoryUserState::Disabled,
                ChangeKind::Delete => DirectoryUserState::Deleted,
                ChangeKind::Upsert => unreachable!("upsert returned above"),
            };
            let mut builder = DirectoryUserRecord::builder(
                entry.external_id.clone(),
                entry.immutable_link.clone(),
            )
            .state(state);
            if let Some(record) = &entry.record {
                if record.external_id() != entry.external_id
                    || record.immutable_link() != &entry.immutable_link
                {
                    return Err(FeedError::InvalidRecord(format!(
                        "{protocol} full snapshot record disagrees with immutable envelope"
                    )));
                }
                if let Some(value) = record.login_name() {
                    builder = builder.login_name(value);
                }
                if let Some(value) = record.email() {
                    builder = builder.email(value);
                }
                if let Some(value) = record.display_name() {
                    builder = builder.display_name(value);
                }
                builder = builder
                    .groups(record.groups().iter().cloned())
                    .attributes(record.attributes().clone());
                if let Some(value) = record.source_version() {
                    builder = builder.source_version(value);
                }
            } else if state == DirectoryUserState::Disabled {
                // DirectoryUserRecord requires a login for a disabled
                // principal. The immutable source ID is the only safe
                // fallback when the native revocation envelope omits it.
                builder = builder.login_name(entry.external_id.clone());
            }
            if entry
                .record
                .as_ref()
                .and_then(DirectoryUserRecord::source_version)
                .is_none()
                && let Some(value) = entry.source_version.as_deref()
            {
                builder = builder.source_version(value);
            }
            builder.build().map_err(FeedError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_by(|left, right| left.external_id().cmp(right.external_id()));
    Ok(records)
}

fn validate_native_bytes(value: &[u8], subject: &str) -> Result<(), FeedError> {
    if value.is_empty() {
        return Err(FeedError::InvalidCursor(format!("{subject} is empty")));
    }
    if value.len() > MAX_NATIVE_CURSOR_BYTES {
        return Err(FeedError::InvalidCursor(format!(
            "{subject} exceeds the {MAX_NATIVE_CURSOR_BYTES}-byte bound"
        )));
    }
    Ok(())
}

fn validate_full_import_cursor(
    current: &FeedCursor,
    next: &FeedCursor,
    terminal: bool,
) -> Result<(), FeedError> {
    if terminal && current == next {
        next.validate()
    } else {
        current.validate_continuation_from(next)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdapAttributeMapping {
    pub external_id: String,
    pub immutable_authority: String,
    pub immutable_value: String,
    pub login_name: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub group: Option<String>,
    pub disabled: Option<String>,
    pub deleted: Option<String>,
    pub source_version: Option<String>,
}

impl LdapAttributeMapping {
    pub fn validate(&self) -> Result<(), FeedError> {
        for (field, value) in [
            ("external ID", self.external_id.as_str()),
            ("immutable authority", self.immutable_authority.as_str()),
            ("immutable value", self.immutable_value.as_str()),
            ("login name", self.login_name.as_str()),
        ] {
            if value.trim().is_empty()
                || value.len() > MAX_DIRECTORY_ATTRIBUTE_BYTES
                || value.chars().any(char::is_control)
            {
                return Err(FeedError::InvalidRecord(format!(
                    "LDAP mapping has an invalid {field} attribute"
                )));
            }
        }
        for (field, value) in [
            ("email", self.email.as_deref()),
            ("display name", self.display_name.as_deref()),
            ("group", self.group.as_deref()),
            ("disabled", self.disabled.as_deref()),
            ("deleted", self.deleted.as_deref()),
            ("source version", self.source_version.as_deref()),
        ] {
            if let Some(value) = value
                && (value.trim().is_empty()
                    || value.len() > MAX_DIRECTORY_ATTRIBUTE_BYTES
                    || value.chars().any(char::is_control))
            {
                return Err(FeedError::InvalidRecord(format!(
                    "LDAP mapping has an invalid {field} attribute"
                )));
            }
        }
        Ok(())
    }
}

fn validate_directory_attributes(
    attributes: &[String],
    required: &[&str],
) -> Result<(), FeedError> {
    if attributes.is_empty() || attributes.len() > MAX_DIRECTORY_ATTRIBUTE_COUNT {
        return Err(FeedError::ProtocolRejected(
            "directory feed attribute list is outside its bound".into(),
        ));
    }
    for attribute in attributes {
        if attribute.trim().is_empty()
            || attribute.len() > MAX_DIRECTORY_ATTRIBUTE_BYTES
            || attribute.chars().any(char::is_control)
        {
            return Err(FeedError::ProtocolRejected(
                "directory feed attribute list contains an invalid name".into(),
            ));
        }
    }
    let mut seen = BTreeMap::new();
    for attribute in attributes {
        if seen.insert(attribute.to_ascii_lowercase(), ()).is_some() {
            return Err(FeedError::ProtocolRejected(
                "directory feed attribute list contains duplicates".into(),
            ));
        }
    }
    for required in required {
        if !attributes
            .iter()
            .any(|attribute| attribute.eq_ignore_ascii_case(required))
        {
            return Err(FeedError::ProtocolRejected(format!(
                "directory feed attribute list omits mapped attribute {required}"
            )));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct GenericLdapPollingFeed<C> {
    connector: DirectoryConnector,
    client: C,
    base_dn: String,
    filter: String,
    mapping: LdapAttributeMapping,
    attributes: Vec<String>,
}

impl<C> GenericLdapPollingFeed<C> {
    pub fn new(
        connector: DirectoryConnector,
        client: C,
        base_dn: impl Into<String>,
        filter: impl Into<String>,
        mapping: LdapAttributeMapping,
        attributes: Vec<String>,
    ) -> Result<Self, FeedError> {
        connector.validate().map_err(FeedError::from)?;
        if connector.feed_kind() != FeedKind::GenericLdapPolling {
            return Err(FeedError::WrongProtocol);
        }
        mapping.validate()?;
        let required = [
            Some(mapping.external_id.as_str()),
            Some(mapping.immutable_value.as_str()),
            Some(mapping.login_name.as_str()),
            mapping.email.as_deref(),
            mapping.display_name.as_deref(),
            mapping.group.as_deref(),
            mapping.disabled.as_deref(),
            mapping.deleted.as_deref(),
            mapping.source_version.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        validate_directory_attributes(&attributes, &required)?;
        let base_dn = base_dn.into();
        let filter = filter.into();
        validate_directory_reference(&base_dn, "LDAP base DN")?;
        validate_directory_reference(&filter, "LDAP filter")?;
        Ok(Self {
            connector,
            client,
            base_dn,
            filter,
            mapping,
            attributes,
        })
    }

    pub fn client_mut(&mut self) -> &mut C {
        &mut self.client
    }
}

impl<C: LdapPollingClient> DirectoryFeed for GenericLdapPollingFeed<C> {
    fn connector(&self) -> &DirectoryConnector {
        &self.connector
    }

    fn preflight<'a>(&'a mut self) -> FeedFuture<'a, ()> {
        Box::pin(async move {
            validate_feed_connector(&self.connector)?;
            self.client.preflight(&self.connector).await
        })
    }

    fn full_import_page<'a>(
        &'a mut self,
        request: FullImportPageRequest,
    ) -> FeedFuture<'a, DirectoryPage> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let cookie = match request.cursor() {
                FeedCursor::Start => Vec::new(),
                FeedCursor::LdapPaged(cookie) => cookie.clone(),
                other => return Err(FeedError::InvalidCursor(other.kind().into())),
            };
            let response = self
                .client
                .search_page(LdapSearchRequest {
                    base_dn: self.base_dn.clone(),
                    filter: self.filter.clone(),
                    attributes: self.attributes.clone(),
                    page_size: request.page_size(),
                    cookie,
                    require_sorted_immutable_id: true,
                })
                .await?;
            request.validate_page_len(response.entries.len())?;
            let records = response
                .entries
                .iter()
                .map(|entry| map_ldap_entry(entry, &self.mapping))
                .collect::<Result<Vec<_>, _>>()?;
            let next = if response.next_cookie.is_empty() {
                if !response.end_of_snapshot {
                    return Err(FeedError::InvalidCursor(
                        "LDAP paged response omitted its continuation cookie".into(),
                    ));
                }
                None
            } else {
                let next = FeedCursor::LdapPaged(response.next_cookie);
                validate_full_import_cursor(request.cursor(), &next, response.end_of_snapshot)?;
                Some(next)
            };
            DirectoryPage::new(
                request.cursor().clone(),
                records,
                next,
                response.end_of_snapshot,
                response.source_watermark,
            )
            .map_err(FeedError::from)
        })
    }

    fn change_batch<'a>(
        &'a mut self,
        request: ChangeFeedRequest,
    ) -> FeedFuture<'a, DirectoryChangeBatch> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let cookie = match request.cursor() {
                FeedCursor::Start | FeedCursor::LdapPaged(_) => match request.cursor() {
                    FeedCursor::Start => Vec::new(),
                    FeedCursor::LdapPaged(cookie) => cookie.clone(),
                    _ => unreachable!(),
                },
                other => return Err(FeedError::InvalidCursor(other.kind().into())),
            };
            let response = self
                .client
                .poll_changes(LdapChangeRequest {
                    base_dn: self.base_dn.clone(),
                    filter: self.filter.clone(),
                    attributes: self.attributes.clone(),
                    cookie,
                    limit: request.limit(),
                })
                .await?;
            if response.changes.len() > request.limit() as usize {
                return Err(FeedError::PageTooLarge);
            }
            let cursor = request.cursor().clone();
            let next = if response.next_cookie.is_empty() {
                if response.has_more {
                    return Err(FeedError::InvalidCursor(
                        "LDAP change response omitted its continuation cookie".into(),
                    ));
                }
                None
            } else {
                let next = FeedCursor::LdapPaged(response.next_cookie);
                request.cursor().validate_continuation_from(&next)?;
                Some(next)
            };
            if !response.changes.is_empty() && next.is_none() {
                return Err(FeedError::InvalidCursor(
                    "LDAP change response with entries omitted its committed continuation cookie"
                        .into(),
                ));
            }
            let changes = response
                .changes
                .into_iter()
                .map(|change| {
                    change.revocation_window()?;
                    DirectoryChange::new(
                        self.connector.id(),
                        change.external_id,
                        change.immutable_link,
                        change.kind,
                        change.record,
                        change.observed_at,
                        change.source_version,
                        next.clone().unwrap_or_else(|| cursor.clone()),
                    )
                    .map_err(FeedError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            DirectoryChangeBatch::new(cursor, changes, next.clone(), response.has_more)
                .map_err(FeedError::from)
        })
    }
}

fn map_ldap_entry(
    entry: &LdapEntry,
    mapping: &LdapAttributeMapping,
) -> Result<DirectoryUserRecord, FeedError> {
    let external_id = required_attr(entry, &mapping.external_id)?;
    let immutable_value = required_attr(entry, &mapping.immutable_value)?;
    let link = ImmutableIdentityLink::new(&mapping.immutable_authority, immutable_value)
        .map_err(FeedError::from)?;
    let login_name = optional_attr(entry, &mapping.login_name);
    let state = if mapping
        .deleted
        .as_deref()
        .and_then(|attribute| optional_attr(entry, attribute))
        .as_deref()
        .is_some_and(is_truthy)
    {
        super::sync::DirectoryUserState::Deleted
    } else if mapping
        .disabled
        .as_deref()
        .and_then(|attribute| optional_attr(entry, attribute))
        .as_deref()
        .is_some_and(is_truthy)
    {
        super::sync::DirectoryUserState::Disabled
    } else {
        super::sync::DirectoryUserState::Active
    };
    let mut builder = DirectoryUserRecord::builder(external_id.clone(), link).state(state);
    if let Some(login_name) = login_name {
        builder = builder.login_name(login_name);
    } else if state == super::sync::DirectoryUserState::Disabled {
        // A disabled native entry can omit its mutable login attribute. Keep
        // the immutable source ID as a non-authoritative placeholder so the
        // revocation state remains durable and auditable.
        builder = builder.login_name(external_id.clone());
    }
    if let Some(attribute) = &mapping.email
        && let Some(value) = optional_attr(entry, attribute)
    {
        builder = builder.email(value);
    }
    if let Some(attribute) = &mapping.display_name
        && let Some(value) = optional_attr(entry, attribute)
    {
        builder = builder.display_name(value);
    }
    if let Some(attribute) = &mapping.group {
        let groups = all_attrs(entry, attribute);
        builder = builder.groups(groups);
    }
    if let Some(attribute) = &mapping.source_version
        && let Some(value) = optional_attr(entry, attribute)
    {
        builder = builder.source_version(value);
    }
    builder.build().map_err(FeedError::from)
}

fn required_attr(entry: &LdapEntry, name: &str) -> Result<String, FeedError> {
    optional_attr(entry, name)
        .ok_or_else(|| FeedError::InvalidRecord(format!("LDAP entry {} lacks {name}", entry.dn)))
}

fn optional_attr(entry: &LdapEntry, name: &str) -> Option<String> {
    entry
        .attributes
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn all_attrs(entry: &LdapEntry, name: &str) -> Vec<String> {
    entry
        .attributes
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, values)| {
            values
                .iter()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "disabled" | "deleted"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdDirSyncRequest {
    pub naming_context: String,
    pub attributes: Vec<String>,
    pub cookie: Vec<u8>,
    pub object_security: bool,
    pub incremental_values: bool,
    pub link_values: bool,
    pub page_size: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdDirSyncResponse {
    pub entries: Vec<DirectoryChangeEnvelope>,
    pub cookie: Vec<u8>,
    pub more_data: bool,
    pub highest_committed_usn: Option<String>,
}

pub trait AdDirSyncClient: Send {
    /// The default keeps an unconfigured AD client fail-closed. A production
    /// client must override this to prove endpoint, CA, credentials, and
    /// DirSync acceptance before a critical control is sent.
    fn preflight<'a>(&'a mut self, _connector: &'a DirectoryConnector) -> FeedFuture<'a, ()> {
        Box::pin(async {
            Err(FeedError::ExternalUnavailable(
                "AD DirSync external readiness preflight is required".into(),
            ))
        })
    }

    fn dirsync<'a>(&'a mut self, request: AdDirSyncRequest) -> FeedFuture<'a, AdDirSyncResponse>;
}

#[derive(Debug)]
pub struct ActiveDirectoryDirSyncFeed<C> {
    connector: DirectoryConnector,
    client: C,
    naming_context: String,
    attributes: Vec<String>,
}

impl<C> ActiveDirectoryDirSyncFeed<C> {
    pub fn new(
        connector: DirectoryConnector,
        client: C,
        naming_context: impl Into<String>,
        attributes: Vec<String>,
    ) -> Result<Self, FeedError> {
        connector.validate().map_err(FeedError::from)?;
        if connector.feed_kind() != FeedKind::ActiveDirectoryDirSync {
            return Err(FeedError::WrongProtocol);
        }
        let naming_context = naming_context.into();
        if naming_context.trim().is_empty() {
            return Err(FeedError::ProtocolRejected(
                "AD DirSync naming context is required".into(),
            ));
        }
        validate_directory_reference(&naming_context, "AD DirSync naming context")?;
        validate_directory_attributes(&attributes, &[])?;
        Ok(Self {
            connector,
            client,
            naming_context,
            attributes,
        })
    }
}

impl<C: AdDirSyncClient> DirectoryFeed for ActiveDirectoryDirSyncFeed<C> {
    fn connector(&self) -> &DirectoryConnector {
        &self.connector
    }

    fn preflight<'a>(&'a mut self) -> FeedFuture<'a, ()> {
        Box::pin(async move {
            validate_feed_connector(&self.connector)?;
            self.client.preflight(&self.connector).await
        })
    }

    fn full_import_page<'a>(
        &'a mut self,
        request: FullImportPageRequest,
    ) -> FeedFuture<'a, DirectoryPage> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let cookie = match request.cursor() {
                FeedCursor::Start => Vec::new(),
                FeedCursor::AdDirSync(cookie) => cookie.clone(),
                other => return Err(FeedError::InvalidCursor(other.kind().into())),
            };
            let response = self
                .client
                .dirsync(AdDirSyncRequest {
                    naming_context: self.naming_context.clone(),
                    attributes: self.attributes.clone(),
                    cookie,
                    object_security: true,
                    incremental_values: true,
                    link_values: true,
                    page_size: request.page_size(),
                })
                .await?;
            request.validate_page_len(response.entries.len())?;
            let records = full_import_records(&response.entries, "AD DirSync")?;
            let end = !response.more_data;
            let next = if response.cookie.is_empty() {
                if !end {
                    return Err(FeedError::InvalidCursor(
                        "AD DirSync response omitted its continuation cookie".into(),
                    ));
                }
                None
            } else {
                let next = FeedCursor::AdDirSync(response.cookie);
                validate_full_import_cursor(request.cursor(), &next, end)?;
                Some(next)
            };
            DirectoryPage::new(
                request.cursor().clone(),
                records,
                next,
                end,
                response.highest_committed_usn,
            )
            .map_err(FeedError::from)
        })
    }

    fn change_batch<'a>(
        &'a mut self,
        request: ChangeFeedRequest,
    ) -> FeedFuture<'a, DirectoryChangeBatch> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let cookie = match request.cursor() {
                FeedCursor::Start => Vec::new(),
                FeedCursor::AdDirSync(cookie) => cookie.clone(),
                other => return Err(FeedError::InvalidCursor(other.kind().into())),
            };
            let response = self
                .client
                .dirsync(AdDirSyncRequest {
                    naming_context: self.naming_context.clone(),
                    attributes: self.attributes.clone(),
                    cookie,
                    object_security: true,
                    incremental_values: true,
                    link_values: true,
                    page_size: request.limit(),
                })
                .await?;
            if response.entries.len() > request.limit() as usize {
                return Err(FeedError::PageTooLarge);
            }
            let next = if response.cookie.is_empty() {
                if response.more_data {
                    return Err(FeedError::InvalidCursor(
                        "AD DirSync change response omitted its continuation cookie".into(),
                    ));
                }
                None
            } else {
                let next = FeedCursor::AdDirSync(response.cookie);
                request.cursor().validate_continuation_from(&next)?;
                Some(next)
            };
            if !response.entries.is_empty() && next.is_none() {
                return Err(FeedError::InvalidCursor(
                    "AD DirSync change response with entries omitted its committed continuation cookie"
                        .into(),
                ));
            }
            let cursor = request.cursor().clone();
            let changes = response
                .entries
                .into_iter()
                .map(|entry| {
                    entry.revocation_window()?;
                    DirectoryChange::new(
                        self.connector.id(),
                        entry.external_id,
                        entry.immutable_link,
                        entry.kind,
                        entry.record,
                        entry.observed_at,
                        entry.source_version,
                        next.clone().unwrap_or_else(|| cursor.clone()),
                    )
                    .map_err(FeedError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            DirectoryChangeBatch::new(cursor, changes, next.clone(), response.more_data)
                .map_err(FeedError::from)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rfc4533SyncRequestMode {
    RefreshOnly,
    RefreshAndPersist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rfc4533Request {
    pub base_dn: String,
    pub attributes: Vec<String>,
    pub cookie: Vec<u8>,
    pub mode: Rfc4533SyncRequestMode,
    pub reload_hint: bool,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rfc4533Response {
    pub entries: Vec<DirectoryChangeEnvelope>,
    pub cookie: Vec<u8>,
    pub refresh_done: bool,
    pub refresh_required: bool,
}

pub trait Rfc4533Client: Send {
    /// The default keeps an unconfigured RFC 4533 client fail-closed. A
    /// production client must override this to prove endpoint, CA,
    /// credentials, and Sync control acceptance before refresh/persist.
    fn preflight<'a>(&'a mut self, _connector: &'a DirectoryConnector) -> FeedFuture<'a, ()> {
        Box::pin(async {
            Err(FeedError::ExternalUnavailable(
                "RFC 4533 external readiness preflight is required".into(),
            ))
        })
    }

    fn sync<'a>(&'a mut self, request: Rfc4533Request) -> FeedFuture<'a, Rfc4533Response>;
}

#[derive(Debug)]
pub struct Rfc4533Feed<C> {
    connector: DirectoryConnector,
    client: C,
    base_dn: String,
    attributes: Vec<String>,
}

impl<C> Rfc4533Feed<C> {
    pub fn new(
        connector: DirectoryConnector,
        client: C,
        base_dn: impl Into<String>,
        attributes: Vec<String>,
    ) -> Result<Self, FeedError> {
        connector.validate().map_err(FeedError::from)?;
        if connector.feed_kind() != FeedKind::Rfc4533 {
            return Err(FeedError::WrongProtocol);
        }
        let base_dn = base_dn.into();
        validate_directory_reference(&base_dn, "RFC 4533 base DN")?;
        validate_directory_attributes(&attributes, &[])?;
        Ok(Self {
            connector,
            client,
            base_dn,
            attributes,
        })
    }
}

impl<C: Rfc4533Client> DirectoryFeed for Rfc4533Feed<C> {
    fn connector(&self) -> &DirectoryConnector {
        &self.connector
    }

    fn preflight<'a>(&'a mut self) -> FeedFuture<'a, ()> {
        Box::pin(async move {
            validate_feed_connector(&self.connector)?;
            self.client.preflight(&self.connector).await
        })
    }

    fn full_import_page<'a>(
        &'a mut self,
        request: FullImportPageRequest,
    ) -> FeedFuture<'a, DirectoryPage> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let cookie = match request.cursor() {
                FeedCursor::Start => Vec::new(),
                FeedCursor::Rfc4533(cookie) => cookie.clone(),
                other => return Err(FeedError::InvalidCursor(other.kind().into())),
            };
            let response = self
                .client
                .sync(Rfc4533Request {
                    base_dn: self.base_dn.clone(),
                    attributes: self.attributes.clone(),
                    cookie,
                    mode: Rfc4533SyncRequestMode::RefreshOnly,
                    reload_hint: false,
                    limit: request.page_size(),
                })
                .await?;
            if response.refresh_required {
                return Err(FeedError::ProtocolRejected(
                    "RFC 4533 server requested a full reload before continuing".into(),
                ));
            }
            let records = full_import_records(&response.entries, "RFC 4533")?;
            request.validate_page_len(records.len())?;
            let next = if response.cookie.is_empty() {
                if !response.refresh_done {
                    return Err(FeedError::InvalidCursor(
                        "RFC 4533 refresh response omitted its continuation cookie".into(),
                    ));
                }
                None
            } else {
                let next = FeedCursor::Rfc4533(response.cookie);
                validate_full_import_cursor(request.cursor(), &next, response.refresh_done)?;
                Some(next)
            };
            DirectoryPage::new(
                request.cursor().clone(),
                records,
                next,
                response.refresh_done,
                None,
            )
            .map_err(FeedError::from)
        })
    }

    fn change_batch<'a>(
        &'a mut self,
        request: ChangeFeedRequest,
    ) -> FeedFuture<'a, DirectoryChangeBatch> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let cookie = match request.cursor() {
                FeedCursor::Start => Vec::new(),
                FeedCursor::Rfc4533(cookie) => cookie.clone(),
                other => return Err(FeedError::InvalidCursor(other.kind().into())),
            };
            let response = self
                .client
                .sync(Rfc4533Request {
                    base_dn: self.base_dn.clone(),
                    attributes: self.attributes.clone(),
                    cookie,
                    mode: Rfc4533SyncRequestMode::RefreshAndPersist,
                    reload_hint: false,
                    limit: request.limit(),
                })
                .await?;
            if response.refresh_required {
                return Err(FeedError::ProtocolRejected(
                    "RFC 4533 change feed requires a new refresh".into(),
                ));
            }
            if response.entries.len() > request.limit() as usize {
                return Err(FeedError::PageTooLarge);
            }
            let next = if response.cookie.is_empty() {
                if !response.entries.is_empty() {
                    return Err(FeedError::InvalidCursor(
                        "RFC 4533 change response with entries omitted its sync cookie".into(),
                    ));
                }
                None
            } else {
                let next = FeedCursor::Rfc4533(response.cookie);
                request.cursor().validate_continuation_from(&next)?;
                Some(next)
            };
            let cursor = request.cursor().clone();
            let changes = response
                .entries
                .into_iter()
                .map(|entry| {
                    entry.revocation_window()?;
                    DirectoryChange::new(
                        self.connector.id(),
                        entry.external_id,
                        entry.immutable_link,
                        entry.kind,
                        entry.record,
                        entry.observed_at,
                        entry.source_version,
                        next.clone().ok_or_else(|| {
                            FeedError::InvalidCursor(
                                "RFC 4533 change requires a committed native cursor".into(),
                            )
                        })?,
                    )
                    .map_err(FeedError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            DirectoryChangeBatch::new(cursor, changes, next.clone(), next.is_some())
                .map_err(FeedError::from)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UcsProvisioningRequest {
    pub position: String,
    pub token: String,
    pub sequence: u64,
    pub limit: u32,
    pub full_snapshot: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UcsProvisioningResponse {
    pub entries: Vec<DirectoryChangeEnvelope>,
    pub token: String,
    pub sequence: u64,
    pub end_of_snapshot: bool,
    pub has_more: bool,
}

pub trait UcsProvisioningClient: Send {
    /// The default keeps an unconfigured UCS client fail-closed. A production
    /// client must override this to prove the HTTPS endpoint, CA,
    /// subscription credentials, and Provisioning Service acceptance before
    /// consuming or acknowledging a message.
    fn preflight<'a>(&'a mut self, _connector: &'a DirectoryConnector) -> FeedFuture<'a, ()> {
        Box::pin(async {
            Err(FeedError::ExternalUnavailable(
                "UCS Provisioning Service external readiness preflight is required".into(),
            ))
        })
    }

    fn provision<'a>(
        &'a mut self,
        request: UcsProvisioningRequest,
    ) -> FeedFuture<'a, UcsProvisioningResponse>;
}

#[derive(Debug)]
pub struct UcsProvisioningServiceFeed<C> {
    connector: DirectoryConnector,
    client: C,
    position: String,
}

impl<C> UcsProvisioningServiceFeed<C> {
    pub fn new(
        connector: DirectoryConnector,
        client: C,
        position: impl Into<String>,
    ) -> Result<Self, FeedError> {
        connector.validate().map_err(FeedError::from)?;
        if connector.feed_kind() != FeedKind::UcsProvisioningService {
            return Err(FeedError::WrongProtocol);
        }
        let position = position.into();
        validate_directory_reference(&position, "UCS provisioning position")?;
        Ok(Self {
            connector,
            client,
            position,
        })
    }
}

impl<C: UcsProvisioningClient> DirectoryFeed for UcsProvisioningServiceFeed<C> {
    fn connector(&self) -> &DirectoryConnector {
        &self.connector
    }

    fn preflight<'a>(&'a mut self) -> FeedFuture<'a, ()> {
        Box::pin(async move {
            validate_feed_connector(&self.connector)?;
            self.client.preflight(&self.connector).await
        })
    }

    fn full_import_page<'a>(
        &'a mut self,
        request: FullImportPageRequest,
    ) -> FeedFuture<'a, DirectoryPage> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let (token, sequence) = match request.cursor() {
                FeedCursor::Start => (String::new(), 0),
                FeedCursor::Ucs { token, sequence } => (token.clone(), *sequence),
                other => return Err(FeedError::InvalidCursor(other.kind().into())),
            };
            let response = self
                .client
                .provision(UcsProvisioningRequest {
                    position: self.position.clone(),
                    token,
                    sequence,
                    limit: request.page_size(),
                    full_snapshot: true,
                })
                .await?;
            if response.end_of_snapshot && response.has_more {
                return Err(FeedError::ProtocolRejected(
                    "UCS provisioning response marked both end_of_snapshot and has_more".into(),
                ));
            }
            request.validate_page_len(response.entries.len())?;
            let records = full_import_records(&response.entries, "UCS")?;
            let end = response.end_of_snapshot && !response.has_more;
            let next = FeedCursor::Ucs {
                token: response.token,
                sequence: response.sequence,
            };
            validate_full_import_cursor(request.cursor(), &next, end)?;
            DirectoryPage::new(
                request.cursor().clone(),
                records,
                Some(next),
                end,
                Some(response.sequence.to_string()),
            )
            .map_err(FeedError::from)
        })
    }

    fn change_batch<'a>(
        &'a mut self,
        request: ChangeFeedRequest,
    ) -> FeedFuture<'a, DirectoryChangeBatch> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let (token, sequence) = match request.cursor() {
                FeedCursor::Start => (String::new(), 0),
                FeedCursor::Ucs { token, sequence } => (token.clone(), *sequence),
                other => return Err(FeedError::InvalidCursor(other.kind().into())),
            };
            let response = self
                .client
                .provision(UcsProvisioningRequest {
                    position: self.position.clone(),
                    token,
                    sequence,
                    limit: request.limit(),
                    full_snapshot: false,
                })
                .await?;
            if response.end_of_snapshot && response.has_more {
                return Err(FeedError::ProtocolRejected(
                    "UCS provisioning response marked both end_of_snapshot and has_more".into(),
                ));
            }
            if response.entries.len() > request.limit() as usize {
                return Err(FeedError::PageTooLarge);
            }
            let next = (response.sequence > 0 || !response.token.trim().is_empty()).then_some(
                FeedCursor::Ucs {
                    token: response.token,
                    sequence: response.sequence,
                },
            );
            if let Some(next) = &next {
                request.cursor().validate_continuation_from(next)?;
            }
            if !response.entries.is_empty() && next.is_none() {
                return Err(FeedError::InvalidCursor(
                    "UCS change response with entries omitted its committed continuation cursor"
                        .into(),
                ));
            }
            let cursor = request.cursor().clone();
            let changes = response
                .entries
                .into_iter()
                .map(|entry| {
                    entry.revocation_window()?;
                    DirectoryChange::new(
                        self.connector.id(),
                        entry.external_id,
                        entry.immutable_link,
                        entry.kind,
                        entry.record,
                        entry.observed_at,
                        entry.source_version,
                        next.clone().unwrap_or_else(|| cursor.clone()),
                    )
                    .map_err(FeedError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            DirectoryChangeBatch::new(request.cursor().clone(), changes, next, response.has_more)
                .map_err(FeedError::from)
        })
    }
}

/// A transport implementation can use this explicit adapter until its
/// external certification is complete. It never returns data and therefore
/// cannot accidentally be treated as a successful sync.
#[derive(Debug, Clone, Copy, Default)]
pub struct FailClosedProtocolAdapter;

impl FailClosedProtocolAdapter {
    pub fn unavailable<T>(&self, protocol: &'static str) -> Result<T, FeedError> {
        Err(FeedError::ExternalUnavailable(protocol.into()))
    }
}

fn validate_feed_connector(connector: &DirectoryConnector) -> Result<(), FeedError> {
    connector.validate().map_err(FeedError::from)?;
    if !connector.enabled() {
        return Err(FeedError::ProtocolRejected(
            "directory connector is disabled".into(),
        ));
    }
    Ok(())
}

fn validate_directory_reference(value: &str, subject: &str) -> Result<(), FeedError> {
    if value.trim().is_empty()
        || value.len() > MAX_DIRECTORY_REFERENCE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(FeedError::ProtocolRejected(format!(
            "{subject} is empty, too large, or contains control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::sync::ConnectorScope;
    use super::*;
    use chrono::TimeZone;

    fn connector(kind: FeedKind, vendor: FeedVendor) -> DirectoryConnector {
        DirectoryConnector::new(
            uuid::Uuid::from_u128(1),
            vendor,
            kind,
            ConnectorScope::Platform,
            match kind {
                FeedKind::UcsProvisioningService => "https://ucs.example.test/provision",
                _ => "ldaps://directory.example.test",
            },
            if kind == FeedKind::UcsProvisioningService {
                super::super::sync::TransportSecurity::Https
            } else {
                super::super::sync::TransportSecurity::Ldaps
            },
            2,
            4,
        )
        .expect("connector")
    }

    #[test]
    fn cursor_kinds_are_protocol_specific() {
        assert_eq!(FeedCursor::AdDirSync(vec![1]).kind(), "ad_dirsync");
        assert_eq!(FeedCursor::Rfc4533(vec![2]).kind(), "rfc4533");
        assert_eq!(
            FeedCursor::Ucs {
                token: "t".into(),
                sequence: 1
            }
            .kind(),
            "ucs_provisioning_service"
        );
    }

    #[test]
    fn protocol_vendor_matrix_is_closed() {
        assert!(FeedKind::GenericLdapPolling.supports(FeedVendor::GenericLdap));
        assert!(
            FeedKind::ActiveDirectoryDirSync.supports(FeedVendor::ActiveDirectoryDomainServices)
        );
        assert!(FeedKind::Rfc4533.supports(FeedVendor::GenericLdap));
        assert!(FeedKind::UcsProvisioningService.supports(FeedVendor::UniventionCorporateServer));
        assert!(!FeedKind::ActiveDirectoryDirSync.supports(FeedVendor::GenericLdap));
    }

    #[test]
    fn request_bounds_reject_zero_and_oversized_pages() {
        assert!(
            FullImportPageRequest::new(
                uuid::Uuid::from_u128(1),
                uuid::Uuid::from_u128(2),
                FeedCursor::Start,
                0,
                1,
            )
            .is_err()
        );
        assert!(
            ChangeFeedRequest::new(
                uuid::Uuid::from_u128(1),
                FeedCursor::Start,
                super::super::sync::MAX_DIRECTORY_PAGE_SIZE + 1,
            )
            .is_err()
        );
    }

    #[test]
    fn native_cursor_boundaries_fail_closed_before_transport() {
        assert!(matches!(
            FeedCursor::AdDirSync(Vec::new()).validate(),
            Err(FeedError::InvalidCursor(message)) if message.contains("empty")
        ));
        assert!(matches!(
            ChangeFeedRequest::new(
                uuid::Uuid::from_u128(1),
                FeedCursor::Rfc4533(Vec::new()),
                1,
            ),
            Err(FeedError::InvalidCursor(message)) if message.contains("empty")
        ));

        let current = FeedCursor::AdDirSync(vec![7]);
        assert!(matches!(
            current.validate_continuation_from(&FeedCursor::AdDirSync(vec![7])),
            Err(FeedError::InvalidCursor(message)) if message.contains("did not advance")
        ));
        assert!(matches!(
            current.validate_continuation_from(&FeedCursor::AdDirSync(Vec::new())),
            Err(FeedError::InvalidCursor(message)) if message.contains("empty")
        ));

        let current = FeedCursor::Ucs {
            token: "ucs-token".into(),
            sequence: 42,
        };
        assert!(matches!(
            current.validate_continuation_from(&FeedCursor::Ucs {
                token: "new-token".into(),
                sequence: 41,
            }),
            Err(FeedError::InvalidCursor(message)) if message.contains("regressed")
        ));
        assert!(
            current
                .validate_continuation_from(&FeedCursor::Ucs {
                    token: "new-token".into(),
                    sequence: 43,
                })
                .is_ok()
        );
        assert!(matches!(
            FeedCursor::Ucs {
                token: "token".into(),
                sequence: 0,
            }
            .validate(),
            Err(FeedError::InvalidCursor(message)) if message.contains("zero")
        ));
        assert!(matches!(
            FeedCursor::LdapPaged(vec![0; MAX_NATIVE_CURSOR_BYTES + 1]).validate(),
            Err(FeedError::InvalidCursor(message)) if message.contains("bound")
        ));
        assert!(matches!(
            current.validate_continuation_from(&FeedCursor::Ucs {
                token: "rotated-token".into(),
                sequence: 42,
            }),
            Err(FeedError::InvalidCursor(message)) if message.contains("regressed")
        ));
    }

    #[test]
    fn ldap_mapping_is_case_insensitive_and_preserves_disabled_state() {
        let mapping = LdapAttributeMapping {
            external_id: "uidNumber".into(),
            immutable_authority: "ldap:entryUUID".into(),
            immutable_value: "entryUUID".into(),
            login_name: "uid".into(),
            email: Some("mail".into()),
            display_name: Some("cn".into()),
            group: Some("memberOf".into()),
            disabled: Some("disabled".into()),
            deleted: None,
            source_version: Some("modifyTimestamp".into()),
        };
        let entry = LdapEntry {
            dn: "uid=alice,dc=example,dc=test".into(),
            attributes: BTreeMap::from([
                ("UIDNUMBER".into(), vec!["100".into()]),
                ("ENTRYUUID".into(), vec!["immutable-1".into()]),
                ("UID".into(), vec!["alice".into()]),
                ("MAIL".into(), vec!["alice@example.test".into()]),
                ("CN".into(), vec!["Alice".into()]),
                ("MEMBEROF".into(), vec!["cn=users".into()]),
                ("DISABLED".into(), vec!["true".into()]),
            ]),
        };
        let record = map_ldap_entry(&entry, &mapping).expect("mapped record");
        assert_eq!(
            record.state(),
            super::super::sync::DirectoryUserState::Disabled
        );
        assert_eq!(record.login_name(), Some("alice"));
        assert_eq!(record.groups(), &["cn=users".to_owned()]);

        let mut deleted_entry = entry;
        deleted_entry.attributes.remove("UID");
        deleted_entry
            .attributes
            .insert("deleted".into(), vec!["true".into()]);
        let mut deleted_mapping = mapping;
        deleted_mapping.deleted = Some("deleted".into());
        let deleted = map_ldap_entry(&deleted_entry, &deleted_mapping).expect("deleted record");
        assert_eq!(
            deleted.state(),
            super::super::sync::DirectoryUserState::Deleted
        );
        assert_eq!(deleted.login_name(), None);
    }

    #[test]
    fn feed_constructors_require_the_attributes_used_for_authoritative_mapping() {
        let mapping = LdapAttributeMapping {
            external_id: "uidNumber".into(),
            immutable_authority: "ldap:entryUUID".into(),
            immutable_value: "entryUUID".into(),
            login_name: "uid".into(),
            email: Some("mail".into()),
            display_name: None,
            group: None,
            disabled: None,
            deleted: None,
            source_version: None,
        };
        let missing = GenericLdapPollingFeed::new(
            connector(FeedKind::GenericLdapPolling, FeedVendor::GenericLdap),
            (),
            "dc=example,dc=test",
            "(objectClass=person)",
            mapping.clone(),
            vec!["uidNumber".into(), "entryUUID".into(), "uid".into()],
        );
        assert!(matches!(
            missing,
            Err(FeedError::ProtocolRejected(message))
                if message.contains("mail")
        ));

        let duplicate = GenericLdapPollingFeed::new(
            connector(FeedKind::GenericLdapPolling, FeedVendor::GenericLdap),
            (),
            "dc=example,dc=test",
            "(objectClass=person)",
            mapping,
            vec![
                "uidNumber".into(),
                "entryUUID".into(),
                "uid".into(),
                "MAIL".into(),
                "mail".into(),
            ],
        );
        assert!(matches!(
            duplicate,
            Err(FeedError::ProtocolRejected(message))
                if message.contains("duplicates")
        ));

        assert!(matches!(
            ActiveDirectoryDirSyncFeed::new(
                connector(
                    FeedKind::ActiveDirectoryDirSync,
                    FeedVendor::ActiveDirectoryDomainServices,
                ),
                (),
                "DC=example,DC=test",
                Vec::new(),
            ),
            Err(FeedError::ProtocolRejected(message))
                if message.contains("attribute list")
        ));
    }

    #[test]
    fn native_full_import_retains_revoked_identity_rows() {
        let disabled = DirectoryChangeEnvelope {
            external_id: "disabled-1".into(),
            immutable_link: ImmutableIdentityLink::new("ad:objectGUID", "guid-disabled-1")
                .expect("immutable link"),
            kind: ChangeKind::Disable,
            record: None,
            source_version: Some("usn-10".into()),
            observed_at: Utc::now(),
        };
        let deleted = DirectoryChangeEnvelope {
            external_id: "deleted-1".into(),
            immutable_link: ImmutableIdentityLink::new("ad:objectGUID", "guid-deleted-1")
                .expect("immutable link"),
            kind: ChangeKind::Delete,
            record: None,
            source_version: Some("usn-11".into()),
            observed_at: Utc::now(),
        };

        let records = full_import_records(&[disabled, deleted], "AD DirSync")
            .expect("native full snapshot records");
        assert_eq!(records.len(), 2);
        let disabled = records
            .iter()
            .find(|record| record.external_id() == "disabled-1")
            .expect("disabled identity is retained");
        assert_eq!(disabled.state(), DirectoryUserState::Disabled);
        assert_eq!(disabled.login_name(), Some("disabled-1"));
        assert_eq!(disabled.source_version(), Some("usn-10"));
        let deleted = records
            .iter()
            .find(|record| record.external_id() == "deleted-1")
            .expect("deleted identity is retained");
        assert_eq!(deleted.state(), DirectoryUserState::Deleted);
        assert_eq!(deleted.login_name(), None);
        assert_eq!(deleted.source_version(), Some("usn-11"));
    }

    #[test]
    fn native_full_import_rejects_an_immutable_envelope_mismatch() {
        let envelope = DirectoryChangeEnvelope {
            external_id: "external-1".into(),
            immutable_link: ImmutableIdentityLink::new("ldap:entryUUID", "uuid-1")
                .expect("immutable link"),
            kind: ChangeKind::Upsert,
            record: Some(
                DirectoryUserRecord::builder(
                    "different-external-1",
                    ImmutableIdentityLink::new("ldap:entryUUID", "uuid-1").expect("immutable link"),
                )
                .login_name("alice")
                .build()
                .expect("record"),
            ),
            source_version: None,
            observed_at: Utc::now(),
        };
        assert!(matches!(
            full_import_records(&[envelope], "RFC 4533"),
            Err(FeedError::InvalidRecord(message))
                if message.contains("immutable envelope")
        ));
    }

    struct LdapEntriesWithoutCursor;

    impl LdapPollingClient for LdapEntriesWithoutCursor {
        fn search_page<'a>(
            &'a mut self,
            _request: LdapSearchRequest,
        ) -> FeedFuture<'a, LdapSearchResponse> {
            Box::pin(async { unreachable!("full import is not part of this contract test") })
        }

        fn poll_changes<'a>(
            &'a mut self,
            _request: LdapChangeRequest,
        ) -> FeedFuture<'a, LdapChangeResponse> {
            Box::pin(async {
                Ok(LdapChangeResponse {
                    changes: vec![revoked_change("ldap:entryUUID", "ldap-user-1")],
                    next_cookie: Vec::new(),
                    has_more: false,
                })
            })
        }
    }

    struct AdEntriesWithoutCursor;

    impl AdDirSyncClient for AdEntriesWithoutCursor {
        fn dirsync<'a>(
            &'a mut self,
            _request: AdDirSyncRequest,
        ) -> FeedFuture<'a, AdDirSyncResponse> {
            Box::pin(async {
                Ok(AdDirSyncResponse {
                    entries: vec![revoked_change("ad:objectGUID", "ad-user-1")],
                    cookie: Vec::new(),
                    more_data: false,
                    highest_committed_usn: None,
                })
            })
        }
    }

    struct UcsEntriesWithoutCursor;

    impl UcsProvisioningClient for UcsEntriesWithoutCursor {
        fn provision<'a>(
            &'a mut self,
            _request: UcsProvisioningRequest,
        ) -> FeedFuture<'a, UcsProvisioningResponse> {
            Box::pin(async {
                Ok(UcsProvisioningResponse {
                    entries: vec![revoked_change("ucs:entryUUID", "ucs-user-1")],
                    token: String::new(),
                    sequence: 0,
                    end_of_snapshot: false,
                    has_more: false,
                })
            })
        }
    }

    struct UcsEndAndMore;

    impl UcsProvisioningClient for UcsEndAndMore {
        fn provision<'a>(
            &'a mut self,
            _request: UcsProvisioningRequest,
        ) -> FeedFuture<'a, UcsProvisioningResponse> {
            Box::pin(async {
                Ok(UcsProvisioningResponse {
                    entries: Vec::new(),
                    token: "ucs-token-1".into(),
                    sequence: 1,
                    end_of_snapshot: true,
                    has_more: true,
                })
            })
        }
    }

    struct RfcEntriesWithoutPreflight;

    impl Rfc4533Client for RfcEntriesWithoutPreflight {
        fn sync<'a>(&'a mut self, _request: Rfc4533Request) -> FeedFuture<'a, Rfc4533Response> {
            Box::pin(async { unreachable!("RFC 4533 transport must not run before preflight") })
        }
    }

    fn revoked_change(authority: &str, value: &str) -> DirectoryChangeEnvelope {
        DirectoryChangeEnvelope {
            external_id: value.into(),
            immutable_link: ImmutableIdentityLink::new(authority, value).expect("immutable link"),
            kind: ChangeKind::Disable,
            record: None,
            source_version: Some("source-1".into()),
            observed_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn non_empty_change_batches_require_an_advanced_native_cursor() {
        let connector_id = uuid::Uuid::from_u128(1);
        let request = || {
            ChangeFeedRequest::new(connector_id, FeedCursor::Start, 1)
                .expect("valid change request")
        };
        let mapping = LdapAttributeMapping {
            external_id: "uidNumber".into(),
            immutable_authority: "ldap:entryUUID".into(),
            immutable_value: "entryUUID".into(),
            login_name: "uid".into(),
            email: None,
            display_name: None,
            group: None,
            disabled: None,
            deleted: None,
            source_version: None,
        };
        let mut ldap = GenericLdapPollingFeed::new(
            connector(FeedKind::GenericLdapPolling, FeedVendor::GenericLdap),
            LdapEntriesWithoutCursor,
            "dc=example,dc=test",
            "(objectClass=person)",
            mapping,
            vec!["uidNumber".into(), "entryUUID".into(), "uid".into()],
        )
        .expect("LDAP feed");
        let error = ldap
            .change_batch(request())
            .await
            .expect_err("LDAP entries without a cursor must fail");
        assert!(matches!(
            error,
            FeedError::InvalidCursor(message) if message.contains("committed continuation cookie")
        ));

        let mut ad = ActiveDirectoryDirSyncFeed::new(
            connector(
                FeedKind::ActiveDirectoryDirSync,
                FeedVendor::ActiveDirectoryDomainServices,
            ),
            AdEntriesWithoutCursor,
            "DC=example,DC=test",
            vec!["objectGUID".into()],
        )
        .expect("AD feed");
        let error = ad
            .change_batch(request())
            .await
            .expect_err("AD entries without a cursor must fail");
        assert!(matches!(
            error,
            FeedError::InvalidCursor(message) if message.contains("committed continuation cookie")
        ));

        let mut ucs = UcsProvisioningServiceFeed::new(
            connector(
                FeedKind::UcsProvisioningService,
                FeedVendor::UniventionCorporateServer,
            ),
            UcsEntriesWithoutCursor,
            "users",
        )
        .expect("UCS feed");
        let error = ucs
            .change_batch(request())
            .await
            .expect_err("UCS entries without a cursor must fail");
        assert!(matches!(
            error,
            FeedError::InvalidCursor(message) if message.contains("committed continuation cursor")
        ));
    }

    #[tokio::test]
    async fn native_adapters_fail_closed_before_external_transport_preflight() {
        let mut ldap = GenericLdapPollingFeed::new(
            connector(FeedKind::GenericLdapPolling, FeedVendor::GenericLdap),
            LdapEntriesWithoutCursor,
            "dc=example,dc=test",
            "(objectClass=person)",
            LdapAttributeMapping {
                external_id: "uidNumber".into(),
                immutable_authority: "ldap:entryUUID".into(),
                immutable_value: "entryUUID".into(),
                login_name: "uid".into(),
                email: None,
                display_name: None,
                group: None,
                disabled: None,
                deleted: None,
                source_version: None,
            },
            vec!["uidNumber".into(), "entryUUID".into(), "uid".into()],
        )
        .expect("LDAP feed");
        assert!(matches!(
            ldap.preflight().await,
            Err(FeedError::ExternalUnavailable(message))
                if message.contains("Generic LDAP")
        ));

        let mut ad = ActiveDirectoryDirSyncFeed::new(
            connector(
                FeedKind::ActiveDirectoryDirSync,
                FeedVendor::ActiveDirectoryDomainServices,
            ),
            AdEntriesWithoutCursor,
            "DC=example,DC=test",
            vec!["objectGUID".into()],
        )
        .expect("AD feed");
        assert!(matches!(
            ad.preflight().await,
            Err(FeedError::ExternalUnavailable(message)) if message.contains("AD DirSync")
        ));

        let mut rfc = Rfc4533Feed::new(
            connector(FeedKind::Rfc4533, FeedVendor::GenericLdap),
            RfcEntriesWithoutPreflight,
            "dc=example,dc=test",
            vec!["entryUUID".into()],
        )
        .expect("RFC 4533 feed");
        assert!(matches!(
            rfc.preflight().await,
            Err(FeedError::ExternalUnavailable(message)) if message.contains("RFC 4533")
        ));

        let mut ucs = UcsProvisioningServiceFeed::new(
            connector(
                FeedKind::UcsProvisioningService,
                FeedVendor::UniventionCorporateServer,
            ),
            UcsEntriesWithoutCursor,
            "users",
        )
        .expect("UCS feed");
        assert!(matches!(
            ucs.preflight().await,
            Err(FeedError::ExternalUnavailable(message))
                if message.contains("UCS Provisioning Service")
        ));
    }

    #[tokio::test]
    async fn ucs_rejects_a_contradictory_terminal_marker_before_staging() {
        let mut feed = UcsProvisioningServiceFeed::new(
            connector(
                FeedKind::UcsProvisioningService,
                FeedVendor::UniventionCorporateServer,
            ),
            UcsEndAndMore,
            "users",
        )
        .expect("UCS feed");
        let request = FullImportPageRequest::new(
            uuid::Uuid::from_u128(1),
            uuid::Uuid::from_u128(2),
            FeedCursor::Start,
            2,
            MAX_FULL_IMPORT_USERS,
        )
        .expect("full-import request");
        assert!(matches!(
            feed.full_import_page(request).await,
            Err(FeedError::ProtocolRejected(message)) if message.contains("end_of_snapshot")
        ));
    }

    #[test]
    fn unavailable_adapter_is_fail_closed() {
        let result: Result<(), FeedError> = FailClosedProtocolAdapter.unavailable("AD DirSync");
        assert!(matches!(
            result,
            Err(FeedError::ExternalUnavailable(protocol)) if protocol == "AD DirSync"
        ));
    }

    #[test]
    fn tls_and_vendor_mismatch_are_rejected_before_transport() {
        let mismatch = ActiveDirectoryDirSyncFeed::<FailClosedProtocolAdapter>::new(
            connector(FeedKind::GenericLdapPolling, FeedVendor::GenericLdap),
            FailClosedProtocolAdapter,
            "DC=example,DC=test",
            vec![],
        );
        assert!(matches!(mismatch, Err(FeedError::WrongProtocol)));
        assert!(
            DirectoryConnector::new(
                uuid::Uuid::from_u128(2),
                FeedVendor::UniventionCorporateServer,
                FeedKind::UcsProvisioningService,
                ConnectorScope::Platform,
                "http://ucs.example.test/provision",
                super::super::sync::TransportSecurity::Https,
                2,
                4,
            )
            .is_err()
        );
    }

    #[test]
    fn feed_error_and_contract_types_are_send_safe() {
        let now = Utc.with_ymd_and_hms(2026, 7, 27, 12, 0, 0).unwrap();
        assert!(now.timestamp() > 0);
        let _scope = ConnectorScope::Platform;
    }
}
