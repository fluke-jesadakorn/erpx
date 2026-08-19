//! Authoritative directory synchronization domain and SQLx persistence seam.
//!
//! This module deliberately does not register itself in `lib.rs`. Registration,
//! migration numbering, worker composition, transport authorization, and UI
//! ownership belong to the ERPX integration lane. The types here are complete
//! enough for that lane to compose without weakening the import invariants.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use thiserror::Error;
use uuid::Uuid;

#[cfg(not(test))]
use super::DirectoryRoleTargetScope;
use super::feeds::{
    ChangeFeedRequest, DirectoryFeed, FeedCursor, FeedError, FeedKind, FullImportPageRequest,
};
use super::rbac::{
    DirectoryGroupRef, DirectoryRoleFact as RbacRoleFact,
    DirectoryRoleFactAction as RbacRoleFactAction, DirectoryRoleFactInput, DirectoryRoleFactReason,
    DirectoryRoleIdentity, DirectoryRoleOutboxEvent, DirectoryRoleRef,
};
use super::sync::ConnectorScope as RbacConnectorScope;
use super::{
    DirectoryRoleAction as LegacyDirectoryRoleAction, DirectoryRoleMapping, DirectoryScope,
};
#[cfg(test)]
use erpx_identity::DirectoryRoleTargetScope;

pub const MAX_FULL_IMPORT_USERS: u64 = 1_000_000;
pub const DEFAULT_DIRECTORY_PAGE_SIZE: u32 = 500;
pub const MAX_DIRECTORY_PAGE_SIZE: u32 = 5_000;
pub const DEFAULT_MAX_IN_FLIGHT_PAGES: u16 = 4;
pub const MAX_IN_FLIGHT_PAGES: u16 = 64;
pub const REVOCATION_SLA_MINUTES: i64 = 5;
pub const DIRECTORY_EVENT_VERSION: i32 = 1;
pub const DIRECTORY_IMPORT_AGGREGATE: &str = "directory_import";
pub const DIRECTORY_IDENTITY_AGGREGATE: &str = "directory_identity";

pub(crate) fn directory_holding_id(data_profile_key: &str) -> Uuid {
    data_profile_key.parse::<Uuid>().unwrap_or_else(|_| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:holding-scope:{data_profile_key}").as_bytes(),
        )
    })
}

/// The stable, non-secret authority name for an immutable directory key.
/// Usernames, email addresses, DNs, and UPNs are intentionally not accepted as
/// identity-link keys because they can be renamed or reassigned.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImmutableIdentityLink {
    authority: String,
    value: String,
}

impl ImmutableIdentityLink {
    pub fn new(authority: impl Into<String>, value: impl Into<String>) -> Result<Self, SyncError> {
        let authority = authority.into();
        let value = value.into();
        validate_immutable_part(&authority, "immutable authority", 200)?;
        validate_immutable_part(&value, "immutable value", 512)?;
        Ok(Self { authority, value })
    }

    pub fn authority(&self) -> &str {
        &self.authority
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// A stable database key. The connector ID is deliberately excluded: a
    /// connector may be replaced while the directory authority remains the
    /// same, but the key itself can never be changed in place.
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(b"erpx-directory-immutable-link:v1\0");
        digest.update(self.authority.as_bytes());
        digest.update([0]);
        digest.update(self.value.as_bytes());
        digest.finalize().into()
    }
}

/// Returns the canonical persisted key for an immutable directory identity.
///
/// This key is intentionally derived only from the directory authority and
/// immutable object value. An external ID, login name, DN, email address, or
/// display name must never be substituted here: those are directory
/// attributes that can be renamed or reassigned.
pub fn immutable_identity_database_key(link: &ImmutableIdentityLink) -> String {
    hex_digest(&link.fingerprint())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryUserState {
    Active,
    Disabled,
    Deleted,
}

impl DirectoryUserState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Deleted => "deleted",
        }
    }

    pub const fn is_revoked(self) -> bool {
        matches!(self, Self::Disabled | Self::Deleted)
    }
}

/// A validated source record. The immutable link is the only identity key;
/// all other fields are attributes that may change between observations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectoryUserRecord {
    external_id: String,
    immutable_link: ImmutableIdentityLink,
    state: DirectoryUserState,
    login_name: Option<String>,
    email: Option<String>,
    display_name: Option<String>,
    groups: Vec<String>,
    attributes: BTreeMap<String, Value>,
    source_version: Option<String>,
}

impl DirectoryUserRecord {
    pub fn builder(
        external_id: impl Into<String>,
        immutable_link: ImmutableIdentityLink,
    ) -> DirectoryUserRecordBuilder {
        DirectoryUserRecordBuilder {
            external_id: external_id.into(),
            immutable_link,
            state: DirectoryUserState::Active,
            login_name: None,
            email: None,
            display_name: None,
            groups: Vec::new(),
            attributes: BTreeMap::new(),
            source_version: None,
        }
    }

    pub fn external_id(&self) -> &str {
        &self.external_id
    }

    pub fn immutable_link(&self) -> &ImmutableIdentityLink {
        &self.immutable_link
    }

    pub fn state(&self) -> DirectoryUserState {
        self.state
    }

    pub fn login_name(&self) -> Option<&str> {
        self.login_name.as_deref()
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    pub fn groups(&self) -> &[String] {
        &self.groups
    }

    pub fn attributes(&self) -> &BTreeMap<String, Value> {
        &self.attributes
    }

    pub fn source_version(&self) -> Option<&str> {
        self.source_version.as_deref()
    }

    pub fn record_digest(&self) -> Result<[u8; 32], SyncError> {
        let encoded = serde_json::to_vec(self).map_err(|error| SyncError::Serialization {
            subject: "directory record",
            detail: error.to_string(),
        })?;
        Ok(Sha256::digest(encoded).into())
    }

    pub fn validate(&self) -> Result<(), SyncError> {
        validate_immutable_part(&self.external_id, "external directory ID", 512)?;
        if self.state != DirectoryUserState::Deleted && self.login_name.is_none() {
            return Err(SyncError::InvalidRecord(
                "active or disabled directory records require a login name".into(),
            ));
        }
        for (field, value, limit) in [
            ("login name", self.login_name.as_deref(), 512),
            ("email", self.email.as_deref(), 1_024),
            ("display name", self.display_name.as_deref(), 1_024),
            ("source version", self.source_version.as_deref(), 512),
        ] {
            if let Some(value) = value {
                validate_immutable_part(value, field, limit)?;
            }
        }
        for group in &self.groups {
            validate_immutable_part(group, "directory group", 512)?;
        }
        for key in self.attributes.keys() {
            validate_immutable_part(key, "directory attribute name", 256)?;
        }
        self.immutable_link.fingerprint();
        Ok(())
    }

    pub fn revocation_reason(&self) -> Option<&'static str> {
        match self.state {
            DirectoryUserState::Active => None,
            DirectoryUserState::Disabled => Some("directory_account_disabled"),
            DirectoryUserState::Deleted => Some("directory_account_deleted"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectoryUserRecordBuilder {
    external_id: String,
    immutable_link: ImmutableIdentityLink,
    state: DirectoryUserState,
    login_name: Option<String>,
    email: Option<String>,
    display_name: Option<String>,
    groups: Vec<String>,
    attributes: BTreeMap<String, Value>,
    source_version: Option<String>,
}

impl DirectoryUserRecordBuilder {
    pub fn state(mut self, state: DirectoryUserState) -> Self {
        self.state = state;
        self
    }

    pub fn login_name(mut self, value: impl Into<String>) -> Self {
        self.login_name = Some(value.into());
        self
    }

    pub fn email(mut self, value: impl Into<String>) -> Self {
        self.email = Some(value.into());
        self
    }

    pub fn display_name(mut self, value: impl Into<String>) -> Self {
        self.display_name = Some(value.into());
        self
    }

    pub fn groups(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.groups = values.into_iter().map(Into::into).collect();
        self
    }

    pub fn attributes(mut self, values: BTreeMap<String, Value>) -> Self {
        self.attributes = values;
        self
    }

    pub fn source_version(mut self, value: impl Into<String>) -> Self {
        self.source_version = Some(value.into());
        self
    }

    pub fn build(self) -> Result<DirectoryUserRecord, SyncError> {
        let record = DirectoryUserRecord {
            external_id: self.external_id,
            immutable_link: self.immutable_link,
            state: self.state,
            login_name: self.login_name,
            email: self.email,
            display_name: self.display_name,
            groups: self.groups,
            attributes: self.attributes,
            source_version: self.source_version,
        };
        record.validate()?;
        Ok(record)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectorScope {
    Platform,
    Holding(Uuid),
    Branch {
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Uuid,
    },
}

impl ConnectorScope {
    pub fn validate(&self) -> Result<(), SyncError> {
        match self {
            Self::Holding(holding_id) if holding_id.is_nil() => {
                return Err(SyncError::InvalidConnector(
                    "holding connector scope must contain a non-nil holding ID".into(),
                ));
            }
            Self::Branch {
                holding_id,
                company_id,
                branch_id,
            } if holding_id.is_nil() || company_id.is_nil() || branch_id.is_nil() => {
                return Err(SyncError::InvalidConnector(
                    "branch connector scope must contain non-nil hierarchy IDs".into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn storage_parts(&self) -> (&'static str, Option<Uuid>) {
        match self {
            Self::Platform => ("platform", None),
            Self::Holding(holding_id) => ("holding", Some(*holding_id)),
            Self::Branch { branch_id, .. } => ("branch", Some(*branch_id)),
        }
    }

    pub fn permits(&self, requested: &Self) -> bool {
        match (self, requested) {
            (Self::Platform, Self::Platform) => true,
            (Self::Platform, Self::Holding(_)) => true,
            (Self::Platform, Self::Branch { .. }) => true,
            (Self::Holding(expected), Self::Holding(actual)) => expected == actual,
            (
                Self::Holding(expected),
                Self::Branch {
                    holding_id: actual, ..
                },
            ) => expected == actual,
            (
                Self::Branch {
                    holding_id: expected_holding,
                    company_id: expected_company,
                    branch_id: expected_branch,
                },
                Self::Branch {
                    holding_id: actual_holding,
                    company_id: actual_company,
                    branch_id: actual_branch,
                },
            ) => {
                expected_holding == actual_holding
                    && expected_company == actual_company
                    && expected_branch == actual_branch
            }
            (Self::Holding(_), Self::Platform) => false,
            (Self::Branch { .. }, Self::Platform | Self::Holding(_)) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportSecurity {
    Ldaps,
    StartTls,
    Https,
}

impl TransportSecurity {
    pub const fn is_tls_only(self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryConnector {
    id: Uuid,
    vendor: FeedVendor,
    feed_kind: FeedKind,
    scope: ConnectorScope,
    endpoint: String,
    security: TransportSecurity,
    page_size: u32,
    max_in_flight_pages: u16,
    enabled: bool,
}

impl DirectoryConnector {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        vendor: FeedVendor,
        feed_kind: FeedKind,
        scope: ConnectorScope,
        endpoint: impl Into<String>,
        security: TransportSecurity,
        page_size: u32,
        max_in_flight_pages: u16,
    ) -> Result<Self, SyncError> {
        let connector = Self {
            id,
            vendor,
            feed_kind,
            scope,
            endpoint: endpoint.into(),
            security,
            page_size,
            max_in_flight_pages,
            enabled: true,
        };
        connector.validate()?;
        Ok(connector)
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn vendor(&self) -> FeedVendor {
        self.vendor
    }

    pub fn feed_kind(&self) -> FeedKind {
        self.feed_kind
    }

    pub fn scope(&self) -> &ConnectorScope {
        &self.scope
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn security(&self) -> TransportSecurity {
        self.security
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn max_in_flight_pages(&self) -> u16 {
        self.max_in_flight_pages
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn validate(&self) -> Result<(), SyncError> {
        if self.id.is_nil() {
            return Err(SyncError::InvalidConnector("connector ID is nil".into()));
        }
        self.scope.validate()?;
        if !self.security.is_tls_only() {
            return Err(SyncError::TlsRequired);
        }
        let endpoint = self.endpoint.to_ascii_lowercase();
        let endpoint_matches = match self.security {
            TransportSecurity::Ldaps => endpoint.starts_with("ldaps://"),
            TransportSecurity::StartTls => {
                endpoint.starts_with("ldap://") || endpoint.starts_with("ldap+starttls://")
            }
            TransportSecurity::Https => endpoint.starts_with("https://"),
        };
        let endpoint_authority = endpoint
            .split_once("://")
            .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default())
            .unwrap_or_default();
        if !endpoint_matches
            || endpoint_authority.is_empty()
            || endpoint_authority
                .chars()
                .any(|character| character.is_ascii_control() || character.is_whitespace())
            || endpoint.contains("starttls=0")
        {
            return Err(SyncError::TlsRequired);
        }
        if self.page_size == 0 || self.page_size > MAX_DIRECTORY_PAGE_SIZE {
            return Err(SyncError::InvalidConnector(format!(
                "page size must be 1..={MAX_DIRECTORY_PAGE_SIZE}"
            )));
        }
        if self.max_in_flight_pages == 0 || self.max_in_flight_pages > MAX_IN_FLIGHT_PAGES {
            return Err(SyncError::InvalidConnector(format!(
                "max in-flight pages must be 1..={MAX_IN_FLIGHT_PAGES}"
            )));
        }
        if !self.feed_kind.supports(self.vendor) {
            return Err(SyncError::InvalidConnector(
                "feed protocol does not match connector vendor".into(),
            ));
        }
        Ok(())
    }
}

/// Vendor is kept separate from protocol so the audit and UI can state which
/// certification is missing without inferring it from a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedVendor {
    GenericLdap,
    ActiveDirectoryDomainServices,
    UniventionCorporateServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Upsert,
    Disable,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectoryChange {
    connector_id: Uuid,
    external_id: String,
    immutable_link: ImmutableIdentityLink,
    kind: ChangeKind,
    record: Option<DirectoryUserRecord>,
    observed_at: DateTime<Utc>,
    source_version: Option<String>,
    cursor: FeedCursor,
}

impl DirectoryChange {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connector_id: Uuid,
        external_id: impl Into<String>,
        immutable_link: ImmutableIdentityLink,
        kind: ChangeKind,
        record: Option<DirectoryUserRecord>,
        observed_at: DateTime<Utc>,
        source_version: Option<String>,
        cursor: FeedCursor,
    ) -> Result<Self, SyncError> {
        let external_id = external_id.into();
        validate_immutable_part(&external_id, "external directory ID", 512)?;
        if connector_id.is_nil() {
            return Err(SyncError::InvalidRecord(
                "change connector ID is nil".into(),
            ));
        }
        cursor
            .validate()
            .map_err(|error| SyncError::InvalidRecord(error.to_string()))?;
        if matches!(cursor, FeedCursor::Start | FeedCursor::EndOfSnapshot) {
            return Err(SyncError::InvalidRecord(
                "directory changes require a native feed cursor".into(),
            ));
        }
        if let Some(source_version) = source_version.as_deref() {
            validate_immutable_part(source_version, "directory source version", 512)?;
        }
        match kind {
            ChangeKind::Upsert if record.is_none() => {
                return Err(SyncError::InvalidRecord(
                    "upsert change must include a record".into(),
                ));
            }
            ChangeKind::Disable | ChangeKind::Delete if record.is_some() => {
                return Err(SyncError::InvalidRecord(
                    "disable and delete changes carry only immutable identity data".into(),
                ));
            }
            _ => {}
        }
        if kind == ChangeKind::Upsert
            && record
                .as_ref()
                .is_some_and(|record| record.state() != DirectoryUserState::Active)
        {
            return Err(SyncError::InvalidRecord(
                "upsert changes must carry an active record; disable/delete are revocation changes"
                    .into(),
            ));
        }
        if let Some(record) = &record {
            record.validate()?;
            if record.external_id() != external_id || record.immutable_link() != &immutable_link {
                return Err(SyncError::ImmutableIdentityConflict {
                    external_id,
                    detail: "change envelope and record disagree".into(),
                });
            }
        }
        Ok(Self {
            connector_id,
            external_id,
            immutable_link,
            kind,
            record,
            observed_at,
            source_version,
            cursor,
        })
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub fn external_id(&self) -> &str {
        &self.external_id
    }

    pub fn immutable_link(&self) -> &ImmutableIdentityLink {
        &self.immutable_link
    }

    pub fn kind(&self) -> ChangeKind {
        self.kind
    }

    pub fn record(&self) -> Option<&DirectoryUserRecord> {
        self.record.as_ref()
    }

    pub fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }

    pub fn source_version(&self) -> Option<&str> {
        self.source_version.as_deref()
    }

    pub fn cursor(&self) -> &FeedCursor {
        &self.cursor
    }

    /// Returns the stable source-event key used by the SQLx store for native
    /// change-feed replay.  A feed cursor identifies the source observation;
    /// observed_at is deliberately excluded because replaying a durable source
    /// event at a later wall-clock time must not create a second identity fact.
    pub fn idempotency_key(&self) -> Result<String, SyncError> {
        let encoded =
            serde_json::to_vec(&self.cursor).map_err(|error| SyncError::Serialization {
                subject: "directory change idempotency source",
                detail: error.to_string(),
            })?;
        let digest: [u8; 32] = Sha256::digest(encoded).into();
        Ok(format!(
            "directory-change/{}/{}/{}",
            self.connector_id,
            self.external_id,
            hex_digest(&digest)
        ))
    }

    fn source_identity(&self) -> Result<Value, SyncError> {
        let mut source = serde_json::to_value(self).map_err(|error| SyncError::Serialization {
            subject: "directory change source identity",
            detail: error.to_string(),
        })?;
        source
            .as_object_mut()
            .expect("DirectoryChange serializes as an object")
            .remove("observed_at");
        Ok(source)
    }

    pub fn revocation_sla(&self) -> Option<RevocationSlaSignal> {
        (!matches!(self.kind, ChangeKind::Upsert)).then(|| {
            RevocationSlaSignal::new(
                self.connector_id,
                self.external_id.clone(),
                self.immutable_link.clone(),
                self.kind,
                self.observed_at,
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectoryPage {
    cursor: FeedCursor,
    records: Vec<DirectoryUserRecord>,
    next_cursor: Option<FeedCursor>,
    end_of_snapshot: bool,
    source_watermark: Option<String>,
}

impl DirectoryPage {
    pub fn new(
        cursor: FeedCursor,
        records: Vec<DirectoryUserRecord>,
        next_cursor: Option<FeedCursor>,
        end_of_snapshot: bool,
        source_watermark: Option<String>,
    ) -> Result<Self, SyncError> {
        let page = Self {
            cursor,
            records,
            next_cursor,
            end_of_snapshot,
            source_watermark,
        };
        page.validate(MAX_DIRECTORY_PAGE_SIZE)?;
        Ok(page)
    }

    pub fn cursor(&self) -> &FeedCursor {
        &self.cursor
    }

    pub fn records(&self) -> &[DirectoryUserRecord] {
        &self.records
    }

    pub fn next_cursor(&self) -> Option<&FeedCursor> {
        self.next_cursor.as_ref()
    }

    pub fn end_of_snapshot(&self) -> bool {
        self.end_of_snapshot
    }

    pub fn source_watermark(&self) -> Option<&str> {
        self.source_watermark.as_deref()
    }

    pub fn validate(&self, page_size: u32) -> Result<(), SyncError> {
        self.cursor
            .validate()
            .map_err(|error| SyncError::InvalidPage(error.to_string()))?;
        if self.records.len() > page_size as usize {
            return Err(SyncError::PageTooLarge {
                actual: self.records.len(),
                maximum: page_size as usize,
            });
        }
        // Native feeds may advance a cookie/sequence without returning a
        // user record (for example, an RFC 4533 control-only refresh step or
        // a UCS provisioning batch containing no user objects). An empty page
        // is safe when, and only when, it carries an advancing continuation;
        // rejecting it would make a valid source cursor impossible to commit.
        if !self.end_of_snapshot && self.next_cursor.is_none() {
            return Err(SyncError::InvalidPage(
                "a non-terminal page must carry a continuation cursor".into(),
            ));
        }
        if let Some(next_cursor) = &self.next_cursor {
            if self.end_of_snapshot && self.cursor == *next_cursor {
                next_cursor
                    .validate()
                    .map_err(|error| SyncError::InvalidPage(error.to_string()))?;
            } else {
                self.cursor
                    .validate_continuation_from(next_cursor)
                    .map_err(|error| SyncError::InvalidPage(error.to_string()))?;
            }
            if matches!(next_cursor, FeedCursor::Start | FeedCursor::EndOfSnapshot) {
                return Err(SyncError::InvalidPage(
                    "page continuation must be native".into(),
                ));
            }
        }
        if !self.end_of_snapshot && self.next_cursor.as_ref() == Some(&self.cursor) {
            return Err(SyncError::InvalidPage(
                "a non-terminal page must advance its continuation cursor".into(),
            ));
        }
        let mut external_ids = BTreeSet::new();
        let mut previous = None;
        for record in &self.records {
            record.validate()?;
            if !external_ids.insert(record.external_id().to_owned()) {
                return Err(SyncError::InvalidPage(
                    "a page contains a duplicate external directory ID".into(),
                ));
            }
            if let Some(previous) = previous
                && previous >= record.external_id()
            {
                return Err(SyncError::InvalidPage(
                    "full-import pages must be sorted by immutable external ID".into(),
                ));
            }
            previous = Some(record.external_id());
        }
        if let Some(watermark) = &self.source_watermark {
            validate_immutable_part(watermark, "source watermark", 512)?;
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<[u8; 32], SyncError> {
        let encoded = serde_json::to_vec(self).map_err(|error| SyncError::Serialization {
            subject: "directory page",
            detail: error.to_string(),
        })?;
        Ok(Sha256::digest(encoded).into())
    }

    pub fn idempotency_key(&self, import_id: Uuid) -> Result<String, SyncError> {
        let digest = self.digest()?;
        Ok(format!(
            "directory-import/{import_id}/{}",
            hex_digest(&digest)
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectoryChangeBatch {
    cursor: FeedCursor,
    changes: Vec<DirectoryChange>,
    next_cursor: Option<FeedCursor>,
    has_more: bool,
}

impl DirectoryChangeBatch {
    pub fn new(
        cursor: FeedCursor,
        changes: Vec<DirectoryChange>,
        next_cursor: Option<FeedCursor>,
        has_more: bool,
    ) -> Result<Self, SyncError> {
        cursor
            .validate()
            .map_err(|error| SyncError::InvalidPage(error.to_string()))?;
        if changes.len() > MAX_DIRECTORY_PAGE_SIZE as usize {
            return Err(SyncError::PageTooLarge {
                actual: changes.len(),
                maximum: MAX_DIRECTORY_PAGE_SIZE as usize,
            });
        }
        if has_more && next_cursor.is_none() {
            return Err(SyncError::InvalidPage(
                "change batch continuation state is inconsistent".into(),
            ));
        }
        if let Some(next_cursor) = &next_cursor {
            cursor
                .validate_continuation_from(next_cursor)
                .map_err(|error| SyncError::InvalidPage(error.to_string()))?;
            if matches!(next_cursor, FeedCursor::Start | FeedCursor::EndOfSnapshot) {
                return Err(SyncError::InvalidPage(
                    "a change batch must carry a native continuation cursor".into(),
                ));
            }
        }
        if has_more && next_cursor.as_ref() == Some(&cursor) {
            return Err(SyncError::InvalidPage(
                "a change batch must advance its continuation cursor".into(),
            ));
        }
        if !changes.is_empty() && next_cursor.is_none() {
            return Err(SyncError::InvalidPage(
                "a non-empty change batch must carry its committed native cursor".into(),
            ));
        }
        let effective_cursor = next_cursor.as_ref().unwrap_or(&cursor);
        let mut external_ids = BTreeSet::new();
        let connector_id = changes.first().map(DirectoryChange::connector_id);
        for change in &changes {
            change
                .cursor()
                .validate()
                .map_err(|error| SyncError::InvalidPage(error.to_string()))?;
            if Some(change.connector_id()) != connector_id {
                return Err(SyncError::InvalidPage(
                    "a change batch cannot mix directory connectors".into(),
                ));
            }
            if change.cursor() != effective_cursor {
                return Err(SyncError::InvalidPage(
                    "change source cursor does not match the batch checkpoint".into(),
                ));
            }
            if !external_ids.insert((change.connector_id(), change.external_id().to_owned())) {
                return Err(SyncError::InvalidPage(
                    "a change batch contains duplicate external directory IDs".into(),
                ));
            }
        }
        Ok(Self {
            cursor,
            changes,
            next_cursor,
            has_more,
        })
    }

    pub fn cursor(&self) -> &FeedCursor {
        &self.cursor
    }

    pub fn changes(&self) -> &[DirectoryChange] {
        &self.changes
    }

    pub fn next_cursor(&self) -> Option<&FeedCursor> {
        self.next_cursor.as_ref()
    }

    pub fn has_more(&self) -> bool {
        self.has_more
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportConfig {
    page_size: u32,
    max_in_flight_pages: u16,
    max_users: u64,
}

impl ImportConfig {
    pub fn new(page_size: u32, max_in_flight_pages: u16) -> Result<Self, SyncError> {
        Self::from_persisted(page_size, max_in_flight_pages, MAX_FULL_IMPORT_USERS)
    }

    /// Rebuilds a tuning contract read from durable storage.  SQLx values are
    /// not trusted merely because the database schema has checks: this path
    /// also guards against legacy/partial schemas and malformed rows before a
    /// worker can use them.
    pub fn from_persisted(
        page_size: u32,
        max_in_flight_pages: u16,
        max_users: u64,
    ) -> Result<Self, SyncError> {
        let config = Self {
            page_size,
            max_in_flight_pages,
            max_users,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn max_in_flight_pages(&self) -> u16 {
        self.max_in_flight_pages
    }

    pub fn max_users(&self) -> u64 {
        self.max_users
    }

    /// Sets the connector-specific ceiling without weakening the global
    /// one-million-user bound.
    pub fn with_max_users(mut self, max_users: u64) -> Result<Self, SyncError> {
        if max_users == 0 || max_users > MAX_FULL_IMPORT_USERS {
            return Err(SyncError::InvalidImportConfig(
                "max users must be between 1 and 1,000,000".into(),
            ));
        }
        self.max_users = max_users;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), SyncError> {
        if self.page_size == 0 || self.page_size > MAX_DIRECTORY_PAGE_SIZE {
            return Err(SyncError::InvalidImportConfig(format!(
                "page size must be 1..={MAX_DIRECTORY_PAGE_SIZE}"
            )));
        }
        if self.max_in_flight_pages == 0 || self.max_in_flight_pages > MAX_IN_FLIGHT_PAGES {
            return Err(SyncError::InvalidImportConfig(format!(
                "max in-flight pages must be 1..={MAX_IN_FLIGHT_PAGES}"
            )));
        }
        if self.max_users == 0 || self.max_users > MAX_FULL_IMPORT_USERS {
            return Err(SyncError::InvalidImportConfig(
                "max users must be between 1 and 1,000,000".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    import_id: Uuid,
    connector: DirectoryConnector,
    config: ImportConfig,
    started_at: DateTime<Utc>,
}

impl ImportPlan {
    pub fn new(
        connector: DirectoryConnector,
        config: ImportConfig,
        started_at: DateTime<Utc>,
    ) -> Result<Self, SyncError> {
        connector.validate()?;
        config.validate()?;
        Ok(Self {
            import_id: Uuid::new_v4(),
            connector,
            config,
            started_at,
        })
    }

    pub fn with_id(
        import_id: Uuid,
        connector: DirectoryConnector,
        config: ImportConfig,
        started_at: DateTime<Utc>,
    ) -> Result<Self, SyncError> {
        if import_id.is_nil() {
            return Err(SyncError::InvalidImportConfig("import ID is nil".into()));
        }
        let mut plan = Self::new(connector, config, started_at)?;
        plan.import_id = import_id;
        Ok(plan)
    }

    pub fn import_id(&self) -> Uuid {
        self.import_id
    }

    pub fn connector(&self) -> &DirectoryConnector {
        &self.connector
    }

    pub fn config(&self) -> &ImportConfig {
        &self.config
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportState {
    Running,
    Finalizing,
    Completed,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportCheckpoint {
    import_id: Uuid,
    connector_id: Uuid,
    cursor: FeedCursor,
    users_seen: u64,
    pages_applied: u64,
    last_external_id: Option<String>,
    snapshot_digest: [u8; 32],
    state: ImportState,
    updated_at: DateTime<Utc>,
}

impl ImportCheckpoint {
    pub fn initial(plan: &ImportPlan) -> Self {
        Self {
            import_id: plan.import_id,
            connector_id: plan.connector.id,
            cursor: FeedCursor::Start,
            users_seen: 0,
            pages_applied: 0,
            last_external_id: None,
            snapshot_digest: [0; 32],
            state: ImportState::Running,
            updated_at: plan.started_at,
        }
    }

    pub fn import_id(&self) -> Uuid {
        self.import_id
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub fn cursor(&self) -> &FeedCursor {
        &self.cursor
    }

    pub fn users_seen(&self) -> u64 {
        self.users_seen
    }

    pub fn pages_applied(&self) -> u64 {
        self.pages_applied
    }

    pub fn last_external_id(&self) -> Option<&str> {
        self.last_external_id.as_deref()
    }

    pub fn snapshot_digest(&self) -> [u8; 32] {
        self.snapshot_digest
    }

    pub fn state(&self) -> &ImportState {
        &self.state
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Validate and advance only after the caller has durably staged the page.
    /// The SQLx store persists this exact next checkpoint in the same
    /// transaction as the page rows and its page idempotency marker.
    pub fn accept_page(
        &self,
        page: &DirectoryPage,
        config: &ImportConfig,
        at: DateTime<Utc>,
    ) -> Result<Self, SyncError> {
        if self.state != ImportState::Running {
            return Err(SyncError::ImportNotRunning);
        }
        page.validate(config.page_size)?;
        if page.cursor() != &self.cursor {
            return Err(SyncError::StaleCursor);
        }
        let next_users = self
            .users_seen
            .checked_add(page.records.len() as u64)
            .ok_or(SyncError::UserLimitExceeded)?;
        if next_users > config.max_users {
            return Err(SyncError::UserLimitExceeded);
        }
        if let Some(last) = &self.last_external_id
            && page
                .records
                .first()
                .is_some_and(|record| record.external_id() <= last.as_str())
        {
            return Err(SyncError::InvalidPage(
                "page would move the immutable external-ID cursor backwards".into(),
            ));
        }
        let page_digest = page.digest()?;
        let mut digest = Sha256::new();
        digest.update(self.snapshot_digest);
        digest.update(page_digest);
        Ok(Self {
            import_id: self.import_id,
            connector_id: self.connector_id,
            cursor: page
                .next_cursor
                .clone()
                .unwrap_or(FeedCursor::EndOfSnapshot),
            users_seen: next_users,
            pages_applied: self
                .pages_applied
                .checked_add(1)
                .ok_or(SyncError::PageCountExceeded)?,
            last_external_id: page
                .records
                .last()
                .map(|record| record.external_id().to_owned())
                .or_else(|| self.last_external_id.clone()),
            snapshot_digest: digest.finalize().into(),
            state: if page.end_of_snapshot {
                ImportState::Finalizing
            } else {
                ImportState::Running
            },
            updated_at: at,
        })
    }

    pub fn mark_failed(mut self, at: DateTime<Utc>) -> Self {
        self.state = ImportState::Failed;
        self.updated_at = at;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub import_id: Uuid,
    pub connector_id: Uuid,
    pub users_seen: u64,
    pub pages_applied: u64,
    pub source_digest: [u8; 32],
    pub completed_at: DateTime<Utc>,
    pub revocations_due: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationStatus {
    Reconciled,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationReport {
    pub import_id: Uuid,
    pub expected_users: u64,
    pub staged_users: u64,
    pub canonical_users: u64,
    pub expected_digest: [u8; 32],
    pub observed_digest: [u8; 32],
    pub status: ReconciliationStatus,
    pub checked_at: DateTime<Utc>,
}

impl ReconciliationReport {
    pub fn from_counts(
        import_id: Uuid,
        expected_users: u64,
        staged_users: u64,
        canonical_users: u64,
        expected_digest: [u8; 32],
        observed_digest: [u8; 32],
        checked_at: DateTime<Utc>,
    ) -> Result<Self, SyncError> {
        if import_id.is_nil() {
            return Err(SyncError::InvalidEvent(
                "reconciliation requires a non-nil import ID".into(),
            ));
        }
        let status = if expected_users == staged_users
            && expected_users == canonical_users
            && expected_digest == observed_digest
        {
            ReconciliationStatus::Reconciled
        } else {
            ReconciliationStatus::Blocked
        };
        Ok(Self {
            import_id,
            expected_users,
            staged_users,
            canonical_users,
            expected_digest,
            observed_digest,
            status,
            checked_at,
        })
    }

    pub const fn is_reconciled(&self) -> bool {
        matches!(self.status, ReconciliationStatus::Reconciled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationSlaSignal {
    connector_id: Uuid,
    external_id: String,
    immutable_link: ImmutableIdentityLink,
    change_kind: ChangeKind,
    observed_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
}

impl RevocationSlaSignal {
    pub fn new(
        connector_id: Uuid,
        external_id: String,
        immutable_link: ImmutableIdentityLink,
        change_kind: ChangeKind,
        observed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            connector_id,
            external_id,
            immutable_link,
            change_kind,
            observed_at,
            deadline: observed_at + revocation_sla(),
        }
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub fn external_id(&self) -> &str {
        &self.external_id
    }

    pub fn immutable_link(&self) -> &ImmutableIdentityLink {
        &self.immutable_link
    }

    pub fn change_kind(&self) -> ChangeKind {
        self.change_kind
    }

    pub fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }

    pub fn deadline(&self) -> DateTime<Utc> {
        self.deadline
    }

    pub fn is_late_at(&self, at: DateTime<Utc>) -> bool {
        at > self.deadline
    }
}

pub fn revocation_sla() -> Duration {
    Duration::minutes(REVOCATION_SLA_MINUTES)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryAction {
    ReadConnector,
    StartFullImport,
    Reconcile,
    RollbackImport,
    ApplyDirectoryChange,
}

impl DirectoryAction {
    pub const fn high_risk(self) -> bool {
        matches!(
            self,
            Self::StartFullImport | Self::Reconcile | Self::RollbackImport
        )
    }
}

/// The shared authorization lane can implement this hook without receiving a
/// mutable directory object. High-risk directory operations require MFA and a
/// connector scope that is contained by the actor's authorized scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryAuthorizationContext {
    actor_ref: String,
    authorized_scope: ConnectorScope,
    mfa_verified_at: Option<DateTime<Utc>>,
    local_break_glass: bool,
    request_is_localhost: bool,
}

impl DirectoryAuthorizationContext {
    pub fn new(
        actor_ref: impl Into<String>,
        authorized_scope: ConnectorScope,
        mfa_verified_at: Option<DateTime<Utc>>,
        local_break_glass: bool,
        request_is_localhost: bool,
    ) -> Result<Self, SyncError> {
        let actor_ref = actor_ref.into();
        validate_immutable_part(&actor_ref, "actor reference", 256)?;
        if local_break_glass && !request_is_localhost {
            return Err(SyncError::BreakGlassMustBeLocal);
        }
        Ok(Self {
            actor_ref,
            authorized_scope,
            mfa_verified_at,
            local_break_glass,
            request_is_localhost,
        })
    }

    pub fn authorize(
        &self,
        action: DirectoryAction,
        connector_scope: &ConnectorScope,
        at: DateTime<Utc>,
    ) -> Result<(), SyncError> {
        if !self.authorized_scope.permits(connector_scope) {
            return Err(SyncError::UnauthorizedScope);
        }
        if action.high_risk()
            && !self
                .mfa_verified_at
                .is_some_and(|verified| at >= verified && at - verified <= revocation_sla())
        {
            return Err(SyncError::MfaRequired);
        }
        if self.local_break_glass && !self.request_is_localhost {
            return Err(SyncError::BreakGlassMustBeLocal);
        }
        Ok(())
    }

    pub fn actor_ref(&self) -> &str {
        &self.actor_ref
    }

    pub fn is_local_break_glass(&self) -> bool {
        self.local_break_glass
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectoryOutboxEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: i64,
    pub connector_id: Uuid,
    pub scope: ConnectorScope,
    pub idempotency_key: String,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

impl DirectoryOutboxEvent {
    fn for_reconciliation(
        report: &ReconciliationReport,
        connector_id: Uuid,
        scope: ConnectorScope,
    ) -> Result<Self, SyncError> {
        if connector_id.is_nil() {
            return Err(SyncError::InvalidEvent(
                "reconciliation event requires a non-nil connector ID".into(),
            ));
        }
        let status = if report.is_reconciled() {
            "reconciled"
        } else {
            "blocked"
        };
        let idempotency_key = format!(
            "directory-reconciliation/{}/{}/{}",
            report.import_id,
            report.checked_at.timestamp_micros(),
            status
        );
        Ok(Self {
            event_id: Uuid::new_v5(&Uuid::NAMESPACE_OID, idempotency_key.as_bytes()),
            event_type: format!("directory.import_reconciliation_{status}.v1"),
            aggregate_type: DIRECTORY_IMPORT_AGGREGATE.into(),
            aggregate_id: report.import_id,
            aggregate_version: 2,
            connector_id,
            scope,
            idempotency_key,
            payload: serde_json::json!({
                "import_id": report.import_id,
                "connector_id": connector_id,
                "expected_users": report.expected_users,
                "staged_users": report.staged_users,
                "canonical_users": report.canonical_users,
                "expected_digest": hex_digest(&report.expected_digest),
                "observed_digest": hex_digest(&report.observed_digest),
                "status": status,
            }),
            occurred_at: report.checked_at,
        })
    }

    pub fn for_change(
        change: &DirectoryChange,
        identity_id: Uuid,
        aggregate_version: i64,
    ) -> Result<Self, SyncError> {
        Self::for_change_in_scope(
            change,
            identity_id,
            aggregate_version,
            ConnectorScope::Platform,
        )
    }

    pub fn for_change_in_scope(
        change: &DirectoryChange,
        identity_id: Uuid,
        aggregate_version: i64,
        scope: ConnectorScope,
    ) -> Result<Self, SyncError> {
        if identity_id.is_nil() || aggregate_version <= 0 {
            return Err(SyncError::InvalidEvent(
                "identity event requires a non-nil identity and positive version".into(),
            ));
        }
        let event_type = match change.kind {
            ChangeKind::Upsert => "directory.identity_upserted.v1",
            ChangeKind::Disable => "directory.identity_disabled.v1",
            ChangeKind::Delete => "directory.identity_deleted.v1",
        };
        let payload = serde_json::to_value(change).map_err(|error| SyncError::Serialization {
            subject: "directory outbox payload",
            detail: error.to_string(),
        })?;
        let key = change
            .idempotency_key()
            .map_err(|error| SyncError::InvalidEvent(error.to_string()))?;
        Ok(Self {
            event_id: Uuid::new_v5(&Uuid::NAMESPACE_OID, key.as_bytes()),
            event_type: event_type.into(),
            aggregate_type: DIRECTORY_IDENTITY_AGGREGATE.into(),
            aggregate_id: identity_id,
            aggregate_version,
            connector_id: change.connector_id,
            scope,
            idempotency_key: key,
            payload,
            occurred_at: change.observed_at,
        })
    }

    pub fn for_import_record(
        import_id: Uuid,
        connector_id: Uuid,
        scope: ConnectorScope,
        identity_id: Uuid,
        aggregate_version: i64,
        record: &DirectoryUserRecord,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, SyncError> {
        let kind = match record.state() {
            DirectoryUserState::Active => ChangeKind::Upsert,
            DirectoryUserState::Disabled => ChangeKind::Disable,
            DirectoryUserState::Deleted => ChangeKind::Delete,
        };
        let event_type = match kind {
            ChangeKind::Upsert => "directory.identity_upserted.v1",
            ChangeKind::Disable => "directory.identity_disabled.v1",
            ChangeKind::Delete => "directory.identity_deleted.v1",
        };
        if import_id.is_nil() || connector_id.is_nil() || identity_id.is_nil() {
            return Err(SyncError::InvalidEvent(
                "import identity event has a nil identifier".into(),
            ));
        }
        if aggregate_version <= 0 {
            return Err(SyncError::InvalidEvent(
                "import identity event requires a positive version".into(),
            ));
        }
        let idempotency_key = format!(
            "directory-import/{}/{}/{}",
            import_id,
            connector_id,
            record.external_id()
        );
        Ok(Self {
            event_id: Uuid::new_v5(&Uuid::NAMESPACE_OID, idempotency_key.as_bytes()),
            event_type: event_type.into(),
            aggregate_type: DIRECTORY_IDENTITY_AGGREGATE.into(),
            aggregate_id: identity_id,
            aggregate_version,
            connector_id,
            scope,
            idempotency_key,
            payload: serde_json::to_value(record).map_err(|error| SyncError::Serialization {
                subject: "directory import outbox payload",
                detail: error.to_string(),
            })?,
            occurred_at,
        })
    }
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("directory TLS-only transport is required")]
    TlsRequired,
    #[error("invalid directory connector: {0}")]
    InvalidConnector(String),
    #[error("invalid import configuration: {0}")]
    InvalidImportConfig(String),
    #[error("invalid directory record: {0}")]
    InvalidRecord(String),
    #[error("invalid directory page: {0}")]
    InvalidPage(String),
    #[error("directory page exceeds bound: {actual} records, maximum {maximum}")]
    PageTooLarge { actual: usize, maximum: usize },
    #[error("directory immutable identity conflict for {external_id}: {detail}")]
    ImmutableIdentityConflict { external_id: String, detail: String },
    #[error("directory import has reached the one-million-user bound")]
    UserLimitExceeded,
    #[error("directory import page count exceeded its representable bound")]
    PageCountExceeded,
    #[error("directory import is not running")]
    ImportNotRunning,
    #[error("directory import cursor is stale or was not committed")]
    StaleCursor,
    #[error("directory import was not finalized")]
    ImportNotFinalized,
    #[error("directory authorization scope is not permitted")]
    UnauthorizedScope,
    #[error("MFA is required for this high-risk directory operation")]
    MfaRequired,
    #[error("local break-glass is permitted only from localhost")]
    BreakGlassMustBeLocal,
    #[error("directory event is invalid: {0}")]
    InvalidEvent(String),
    #[error("directory serialization failed for {subject}: {detail}")]
    Serialization {
        subject: &'static str,
        detail: String,
    },
    #[error("directory feed failed: {0}")]
    Feed(#[from] FeedError),
    #[error("directory store failed: {0}")]
    Store(#[from] SyncStoreError),
}

#[derive(Debug, Error)]
pub enum SyncStoreError {
    #[error("directory database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("directory store record is invalid: {0}")]
    InvalidRecord(String),
    #[error("directory store conflict: {0}")]
    Conflict(String),
    #[error("directory store serialization error: {0}")]
    Serialization(String),
}

pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, SyncStoreError>> + Send + 'a>>;

fn sync_failure_with_rollback(
    original: SyncError,
    rollback: Result<(), SyncStoreError>,
) -> SyncError {
    match rollback {
        Ok(()) => original,
        Err(rollback_error) => SyncError::Store(SyncStoreError::Conflict(format!(
            "directory import failed: {original}; rollback failed: {rollback_error}"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPageReceipt {
    pub checkpoint: ImportCheckpoint,
    pub records_applied: u64,
    pub duplicate_page: bool,
}

pub trait DirectorySyncStore: Send + Sync {
    fn begin_import<'a>(&'a self, plan: &'a ImportPlan) -> StoreFuture<'a, ImportCheckpoint>;

    fn stage_page<'a>(
        &'a self,
        plan: &'a ImportPlan,
        checkpoint: &'a ImportCheckpoint,
        page: &'a DirectoryPage,
        next_checkpoint: &'a ImportCheckpoint,
    ) -> StoreFuture<'a, ApplyPageReceipt>;

    fn finalize_import<'a>(
        &'a self,
        plan: &'a ImportPlan,
        checkpoint: &'a ImportCheckpoint,
        role_mappings: &'a [DirectoryRoleMapping],
        completed_at: DateTime<Utc>,
    ) -> StoreFuture<'a, ImportSummary>;

    fn rollback_import<'a>(
        &'a self,
        import_id: Uuid,
        reason: &'a str,
        at: DateTime<Utc>,
    ) -> StoreFuture<'a, ()>;

    fn apply_change<'a>(&'a self, change: &'a DirectoryChange) -> StoreFuture<'a, bool>;

    /// Commit the native feed checkpoint after the corresponding source
    /// changes have been durably applied. Replaying after a checkpoint write
    /// failure is safe because apply_change is source-idempotent.
    fn save_change_checkpoint<'a>(
        &'a self,
        connector_id: Uuid,
        feed: FeedKind,
        previous_cursor: &'a FeedCursor,
        cursor: &'a FeedCursor,
        at: DateTime<Utc>,
    ) -> StoreFuture<'a, ()>;

    fn reconcile_import<'a>(
        &'a self,
        import_id: Uuid,
        checked_at: DateTime<Utc>,
    ) -> StoreFuture<'a, ReconciliationReport>;
}

/// Bounded, checkpoint-after-commit orchestration for a full import. At most
/// one feed page is in flight per runner; a caller may run a bounded number of
/// runners according to `ImportConfig::max_in_flight_pages` without allowing
/// unbounded source or database buffering.
#[derive(Debug)]
pub struct FullImportRunner<F, S> {
    feed: F,
    store: S,
}

impl<F, S> FullImportRunner<F, S>
where
    F: DirectoryFeed,
    S: DirectorySyncStore,
{
    pub fn new(feed: F, store: S) -> Self {
        Self { feed, store }
    }

    pub async fn run(
        &mut self,
        plan: &ImportPlan,
        at: DateTime<Utc>,
    ) -> Result<ImportSummary, SyncError> {
        self.run_with_role_mappings(plan, &[], at).await
    }

    pub async fn run_with_role_mappings(
        &mut self,
        plan: &ImportPlan,
        role_mappings: &[DirectoryRoleMapping],
        at: DateTime<Utc>,
    ) -> Result<ImportSummary, SyncError> {
        let source_connector = self.feed.connector();
        let expected_feed = source_connector.feed_kind();
        if source_connector.id() != plan.connector.id()
            || expected_feed != plan.connector.feed_kind()
            || source_connector.scope() != plan.connector.scope()
        {
            return Err(SyncError::InvalidConnector(
                "full-import feed is not bound to the planned connector protocol or scope".into(),
            ));
        }
        self.feed.preflight().await.map_err(SyncError::Feed)?;
        let mut checkpoint = self.store.begin_import(plan).await?;
        loop {
            let remaining_users = plan
                .config
                .max_users
                .checked_sub(checkpoint.users_seen)
                .ok_or(SyncError::UserLimitExceeded)?;
            let request = FullImportPageRequest::new(
                plan.connector.id,
                plan.import_id,
                checkpoint.cursor.clone(),
                plan.config.page_size,
                remaining_users,
            )
            .map_err(SyncError::Feed)?;
            let page = match self.feed.full_import_page(request).await {
                Ok(page) => page,
                Err(error) => {
                    let rollback = self
                        .store
                        .rollback_import(plan.import_id, &error.to_string(), at)
                        .await;
                    return Err(sync_failure_with_rollback(SyncError::Feed(error), rollback));
                }
            };
            validate_page_feed_cursor(expected_feed, &page)?;
            let next = match checkpoint.accept_page(&page, &plan.config, at) {
                Ok(next) => next,
                Err(error) => {
                    let rollback = self
                        .store
                        .rollback_import(plan.import_id, &error.to_string(), at)
                        .await;
                    return Err(sync_failure_with_rollback(error, rollback));
                }
            };
            let receipt = match self.store.stage_page(plan, &checkpoint, &page, &next).await {
                Ok(receipt) => receipt,
                Err(error) => {
                    let rollback = self
                        .store
                        .rollback_import(plan.import_id, &error.to_string(), at)
                        .await;
                    return Err(sync_failure_with_rollback(
                        SyncError::Store(error),
                        rollback,
                    ));
                }
            };
            checkpoint = receipt.checkpoint;
            if page.end_of_snapshot {
                if checkpoint.state != ImportState::Finalizing {
                    return Err(SyncError::ImportNotFinalized);
                }
                return match self
                    .store
                    .finalize_import(plan, &checkpoint, role_mappings, at)
                    .await
                {
                    Ok(summary) => Ok(summary),
                    Err(error) => {
                        let rollback = self
                            .store
                            .rollback_import(plan.import_id, &error.to_string(), at)
                            .await;
                        Err(sync_failure_with_rollback(
                            SyncError::Store(error),
                            rollback,
                        ))
                    }
                };
            }
        }
    }

    pub async fn ingest_changes(&mut self, request: ChangeFeedRequest) -> Result<u64, SyncError> {
        self.ingest_changes_at(request, Utc::now()).await
    }

    pub async fn ingest_changes_at(
        &mut self,
        request: ChangeFeedRequest,
        at: DateTime<Utc>,
    ) -> Result<u64, SyncError> {
        let connector_id = self.feed.connector().id();
        let feed_kind = self.feed.connector().feed_kind();
        if request.connector_id() != connector_id {
            return Err(SyncError::InvalidRecord(
                "change request is not bound to the feed connector".into(),
            ));
        }
        if !cursor_matches_feed(feed_kind, request.cursor()) {
            return Err(SyncError::InvalidRecord(
                "change request cursor does not match the feed connector".into(),
            ));
        }
        self.feed.preflight().await.map_err(SyncError::Feed)?;
        let previous_cursor = request.cursor().clone();
        let batch = self.feed.change_batch(request).await?;
        let mut applied = 0_u64;
        for change in batch.changes() {
            if self.store.apply_change(change).await? {
                applied = applied.saturating_add(1);
            }
        }
        if let Some(next_cursor) = batch.next_cursor() {
            self.store
                .save_change_checkpoint(connector_id, feed_kind, &previous_cursor, next_cursor, at)
                .await?;
        }
        Ok(applied)
    }
}

/// SQLx adapter. The migration owner must create the tables listed in
/// `DIRECTORY_SYNC_SCHEMA_CONTRACT` in the audit README before registration.
/// Every page is staged and checkpointed in one transaction. Canonical rows,
/// revocation rows, outbox events, and the rebuildable projection are changed
/// only by `finalize_import` or `apply_change` transactions.
#[derive(Debug, Clone)]
pub struct SqlxDirectorySyncStore {
    pool: PgPool,
}

#[derive(Debug, Clone)]
struct FullImportProjectionRow {
    identity_id: Uuid,
    connector_id: Uuid,
    external_id: String,
    immutable_authority: String,
    immutable_value: String,
    identity_link: String,
    principal_ref: Option<String>,
    active: bool,
    deleted: bool,
    groups: Value,
    source_revision: i64,
    observed_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct FullImportRoleRow {
    identity_link: String,
    immutable_authority: String,
    immutable_value: String,
    action: LegacyDirectoryRoleAction,
    reason: DirectoryRoleFactReason,
    occurred_at: DateTime<Utc>,
}

impl SqlxDirectorySyncStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn load_directory_authority(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        connector_id: Uuid,
    ) -> Result<String, SyncStoreError> {
        let row = sqlx::query(
            "SELECT provider, directory_authority_fingerprint
             FROM erpx_identity.directory_connectors WHERE id=$1",
        )
        .bind(connector_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| SyncStoreError::Conflict("directory connector is unavailable".into()))?;
        let provider: String = row.try_get("provider")?;
        if !matches!(
            provider.as_str(),
            "generic_ldap" | "active_directory_domain_services" | "univention_corporate_server"
        ) {
            return Err(SyncStoreError::InvalidRecord(
                "persisted directory provider is invalid".into(),
            ));
        }
        let fingerprint: Option<String> = row.try_get("directory_authority_fingerprint")?;
        if let Some(fingerprint) = fingerprint.as_deref()
            && (fingerprint.len() != 64
                || !fingerprint.chars().all(|value| value.is_ascii_hexdigit()))
        {
            return Err(SyncStoreError::InvalidRecord(
                "persisted directory authority fingerprint is invalid".into(),
            ));
        }
        Ok(format!(
            "{}:{}",
            provider,
            fingerprint
                .map(|value| value.to_ascii_lowercase())
                .unwrap_or_else(|| format!("connector:{connector_id}"))
        ))
    }

    /// Returns the only resumable import for a connector.  Finalizing is
    /// included so a worker crash after the terminal page resumes the durable
    /// tombstoning transaction instead of starting a second snapshot.
    pub async fn active_import_id(
        &self,
        connector_id: Uuid,
    ) -> Result<Option<Uuid>, SyncStoreError> {
        Ok(self
            .active_import_config(connector_id)
            .await?
            .map(|(import_id, _, _, _)| import_id))
    }

    /// Returns the durable import configuration that an executable worker must resume.
    ///
    /// The API may create a full-import row before the worker process runs. Reading the
    /// persisted page and user bounds here prevents the worker from silently replacing that
    /// request with deployment defaults.
    pub async fn active_import_config(
        &self,
        connector_id: Uuid,
    ) -> Result<Option<(Uuid, u32, u64, u16)>, SyncStoreError> {
        let row = sqlx::query(
            "SELECT i.id, i.page_size, i.max_users, c.max_in_flight_pages
             FROM erpx_identity.directory_imports AS i
             JOIN erpx_identity.directory_connectors c ON c.id = i.connector_id
             WHERE i.connector_id=$1 AND i.state IN ('running', 'finalizing')
             ORDER BY i.updated_at DESC, i.id DESC
             LIMIT 1",
        )
        .bind(connector_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let page_size: i32 = row.try_get("page_size")?;
            let max_users: i64 = row.try_get("max_users")?;
            let max_in_flight_pages: i32 = row.try_get("max_in_flight_pages")?;
            let page_size = u32::try_from(page_size).map_err(|_| {
                SyncStoreError::InvalidRecord("persisted directory page size is invalid".into())
            })?;
            let max_users = u64::try_from(max_users).map_err(|_| {
                SyncStoreError::InvalidRecord("persisted directory user bound is invalid".into())
            })?;
            let max_in_flight_pages = u16::try_from(max_in_flight_pages).map_err(|_| {
                SyncStoreError::InvalidRecord("persisted directory concurrency is invalid".into())
            })?;
            let config = ImportConfig::from_persisted(page_size, max_in_flight_pages, max_users)
                .map_err(|error| {
                    SyncStoreError::InvalidRecord(format!(
                        "persisted directory import configuration is invalid: {error}"
                    ))
                })?;
            Ok((
                row.try_get("id")?,
                config.page_size(),
                config.max_users(),
                config.max_in_flight_pages(),
            ))
        })
        .transpose()
    }

    /// Recover the native source position staged by the last accepted page.
    ///
    /// A full import deliberately keeps the ordinary connector checkpoint
    /// untouched until canonical finalization succeeds. If the worker dies
    /// after staging the terminal page, the import checkpoint is already
    /// durable but its cursor is `EndOfSnapshot`; the page's input cursor is
    /// therefore the safe native position to publish after finalization.
    pub async fn latest_import_source_cursor(
        &self,
        import_id: Uuid,
    ) -> Result<Option<FeedCursor>, SyncStoreError> {
        let row = sqlx::query(
            "SELECT cursor, next_checkpoint
             FROM erpx_identity.directory_import_pages
             WHERE import_id=$1
             ORDER BY (next_checkpoint->>'pages_applied')::BIGINT DESC,
                      applied_at DESC, idempotency_key DESC
             LIMIT 1",
        )
        .bind(import_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let page_cursor: FeedCursor = serde_json::from_value(row.try_get("cursor")?)
            .map_err(|error| SyncStoreError::Serialization(error.to_string()))?;
        let next_checkpoint: ImportCheckpoint =
            serde_json::from_value(row.try_get::<Value, _>("next_checkpoint")?)
                .map_err(|error| SyncStoreError::Serialization(error.to_string()))?;
        let source_cursor = match next_checkpoint.cursor {
            FeedCursor::EndOfSnapshot => page_cursor,
            cursor => cursor,
        };
        Ok(
            (!matches!(source_cursor, FeedCursor::Start | FeedCursor::EndOfSnapshot))
                .then_some(source_cursor),
        )
    }

    async fn load_checkpoint<'e, E>(
        &self,
        import_id: Uuid,
        executor: E,
    ) -> Result<ImportCheckpoint, SyncStoreError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let row = sqlx::query(
            "SELECT connector_id, cursor, users_seen, pages_applied,
                    last_external_id, snapshot_digest, state, updated_at
             FROM erpx_identity.directory_imports
             WHERE id = $1
             FOR UPDATE",
        )
        .bind(import_id)
        .fetch_one(executor)
        .await?;
        let cursor_value: Value = row.try_get("cursor")?;
        let cursor: FeedCursor = serde_json::from_value(cursor_value)
            .map_err(|error| SyncStoreError::Serialization(error.to_string()))?;
        let digest: Vec<u8> = row.try_get("snapshot_digest")?;
        let snapshot_digest: [u8; 32] = digest
            .try_into()
            .map_err(|_| SyncStoreError::InvalidRecord("snapshot digest is not 32 bytes".into()))?;
        let state = match row.try_get::<String, _>("state")?.as_str() {
            "running" => ImportState::Running,
            "finalizing" => ImportState::Finalizing,
            "completed" => ImportState::Completed,
            "failed" => ImportState::Failed,
            "rolled_back" => ImportState::RolledBack,
            other => {
                return Err(SyncStoreError::InvalidRecord(format!(
                    "unknown import state {other}"
                )));
            }
        };
        Ok(ImportCheckpoint {
            import_id,
            connector_id: row.try_get("connector_id")?,
            cursor,
            users_seen: non_negative_count(row.try_get("users_seen")?)?,
            pages_applied: non_negative_count(row.try_get("pages_applied")?)?,
            last_external_id: row.try_get("last_external_id")?,
            snapshot_digest,
            state,
            updated_at: row.try_get("updated_at")?,
        })
    }

    async fn insert_outbox_event(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: &DirectoryOutboxEvent,
        actor_ref: Option<&str>,
    ) -> Result<(), SyncStoreError> {
        let (scope_kind, scope_id) = event.scope.storage_parts();
        let active_scope = serde_json::json!({
            "kind": scope_kind,
            "holding_id": scope_id,
            "connector_id": event.connector_id,
        });
        let result = sqlx::query(
            "INSERT INTO erpx_identity.directory_outbox
                (event_id, aggregate_id, aggregate_type, event_type,
                 event_version, connector_id, aggregate_version, scope_json,
                 actor_json, idempotency_key, payload_json, occurred_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(event.event_id)
        .bind(event.aggregate_id)
        .bind(&event.aggregate_type)
        .bind(&event.event_type)
        .bind(DIRECTORY_EVENT_VERSION)
        .bind(event.connector_id)
        .bind(event.aggregate_version)
        .bind(&active_scope)
        .bind(actor_ref.map(|actor| serde_json::json!({ "actor_ref": actor })))
        .bind(&event.idempotency_key)
        .bind(&event.payload)
        .bind(event.occurred_at)
        .execute(&mut **tx)
        .await?;
        if result.rows_affected() == 0 {
            let existing = sqlx::query(
                "SELECT event_type, aggregate_id, aggregate_version,
                        connector_id, scope_json, actor_json,
                        idempotency_key, payload_json
                 FROM erpx_identity.directory_outbox
                 WHERE event_id=$1",
            )
            .bind(event.event_id)
            .fetch_one(&mut **tx)
            .await?;
            let same = existing.try_get::<String, _>("event_type")? == event.event_type
                && existing.try_get::<Uuid, _>("aggregate_id")? == event.aggregate_id
                && existing.try_get::<i64, _>("aggregate_version")? == event.aggregate_version
                && existing.try_get::<Uuid, _>("connector_id")? == event.connector_id
                && existing.try_get::<Value, _>("scope_json")? == active_scope
                && existing.try_get::<Option<Value>, _>("actor_json")?
                    == actor_ref.map(|actor| serde_json::json!({ "actor_ref": actor }))
                && existing.try_get::<String, _>("idempotency_key")? == event.idempotency_key
                && existing.try_get::<Value, _>("payload_json")? == event.payload;
            if !same {
                return Err(SyncStoreError::Conflict(
                    "directory outbox event ID is bound to different event content".into(),
                ));
            }
        }
        Ok(())
    }

    async fn apply_full_import_projection_and_roles(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        plan: &ImportPlan,
        role_mappings: &[DirectoryRoleMapping],
        completed_at: DateTime<Utc>,
    ) -> Result<u64, SyncStoreError> {
        let connector_scope_key: String = sqlx::query_scalar(
            "SELECT scope_key
             FROM erpx_identity.directory_connectors
             WHERE id=$1 FOR SHARE",
        )
        .bind(plan.connector.id)
        .fetch_one(&mut **tx)
        .await?;
        let connector_scope = directory_scope_from_key(&connector_scope_key)?;
        for mapping in role_mappings {
            mapping
                .validate_for_connector(plan.connector.id, &connector_scope)
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            let scope_key = mapping
                .scope
                .authorization_key()
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            sqlx::query(
                "INSERT INTO erpx_identity.directory_role_mappings
                    (connector_id, group_ref, role_ref, scope_key, requires_mfa,
                     active, revision, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                 ON CONFLICT (connector_id, group_ref, role_ref, scope_key) DO UPDATE SET
                    requires_mfa=EXCLUDED.requires_mfa, active=EXCLUDED.active,
                    revision=EXCLUDED.revision, updated_at=EXCLUDED.updated_at
                 WHERE erpx_identity.directory_role_mappings.revision < EXCLUDED.revision",
            )
            .bind(mapping.connector_id)
            .bind(&mapping.group_ref)
            .bind(&mapping.role_ref)
            .bind(scope_key)
            .bind(mapping.requires_mfa)
            .bind(mapping.active)
            .bind(i64::try_from(mapping.revision).map_err(|_| {
                SyncStoreError::InvalidRecord("directory role mapping revision overflow".into())
            })?)
            .bind(completed_at)
            .execute(&mut **tx)
            .await?;
        }

        let mut last_external_id: Option<String> = None;
        let mut role_facts_appended = 0_u64;
        loop {
            let rows = sqlx::query(
                "SELECT staged.identity_id, staged.connector_id, staged.external_id,
                        staged.immutable_authority, staged.immutable_value,
                        staged.state, staged.record, staged.observed_at,
                        current.aggregate_version
                 FROM erpx_identity.directory_import_members staged
                 JOIN erpx_identity.directory_identities current
                   ON current.connector_id=staged.connector_id
                  AND current.external_id=staged.external_id
                 WHERE staged.import_id=$1
                   AND ($2::text IS NULL OR staged.external_id > $2)
                 ORDER BY staged.external_id
                 LIMIT 500",
            )
            .bind(plan.import_id)
            .bind(last_external_id.as_deref())
            .fetch_all(&mut **tx)
            .await?;
            if rows.is_empty() {
                break;
            }
            let mut projection_rows = Vec::with_capacity(rows.len());
            for row in rows {
                let record: Value = row.try_get("record")?;
                let state: String = row.try_get("state")?;
                let active = state == "active";
                let observed_at: DateTime<Utc> = row.try_get("observed_at")?;
                let aggregate_version: i64 = row.try_get("aggregate_version")?;
                let source_revision = full_import_source_revision(&record, aggregate_version)?;
                let external_id: String = row.try_get("external_id")?;
                let immutable_link = ImmutableIdentityLink::new(
                    row.try_get::<String, _>("immutable_authority")?,
                    row.try_get::<String, _>("immutable_value")?,
                )
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
                let identity_link = immutable_identity_database_key(&immutable_link);
                projection_rows.push(FullImportProjectionRow {
                    identity_id: row.try_get("identity_id")?,
                    connector_id: row.try_get("connector_id")?,
                    external_id,
                    immutable_authority: immutable_link.authority().to_owned(),
                    immutable_value: immutable_link.value().to_owned(),
                    identity_link,
                    principal_ref: record
                        .get("login_name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    active,
                    deleted: state == "deleted",
                    groups: record
                        .get("groups")
                        .cloned()
                        .unwrap_or_else(|| Value::Array(Vec::new())),
                    source_revision,
                    observed_at,
                    revoked_at: (!active).then_some(observed_at),
                });
            }
            Self::persist_full_import_projection_batch(tx, &projection_rows).await?;
            for mapping in role_mappings {
                let role_rows = projection_rows
                    .iter()
                    .map(|row| {
                        let _source_revision =
                            u64::try_from(row.source_revision).map_err(|_| {
                                SyncStoreError::InvalidRecord(
                                    "directory role sequence is not positive".into(),
                                )
                            })?;
                        Ok(FullImportRoleRow {
                            identity_link: row.identity_link.clone(),
                            immutable_authority: row.immutable_authority.clone(),
                            immutable_value: row.immutable_value.clone(),
                            action: if row.active
                                && row.groups.as_array().is_some_and(|groups| {
                                    groups
                                        .iter()
                                        .any(|group| group.as_str() == Some(&mapping.group_ref))
                                })
                                && mapping.active
                            {
                                LegacyDirectoryRoleAction::Grant
                            } else {
                                LegacyDirectoryRoleAction::Revoke
                            },
                            reason: if row.active
                                && row.groups.as_array().is_some_and(|groups| {
                                    groups
                                        .iter()
                                        .any(|group| group.as_str() == Some(&mapping.group_ref))
                                })
                                && mapping.active
                            {
                                DirectoryRoleFactReason::GroupMatched
                            } else if !mapping.active {
                                DirectoryRoleFactReason::MappingDeactivated
                            } else if row.deleted {
                                DirectoryRoleFactReason::IdentityDeleted
                            } else if !row.active {
                                DirectoryRoleFactReason::IdentityDisabled
                            } else {
                                DirectoryRoleFactReason::GroupRemoved
                            },
                            occurred_at: row.observed_at,
                        })
                    })
                    .collect::<Result<Vec<_>, SyncStoreError>>()?;
                role_facts_appended = role_facts_appended.saturating_add(
                    Self::append_full_import_role_facts(tx, plan.connector.id, mapping, &role_rows)
                        .await?,
                );
            }
            last_external_id = projection_rows.last().map(|row| row.external_id.clone());
        }

        let mut last_missing_external_id: Option<String> = None;
        loop {
            let rows = sqlx::query(
                "SELECT current.id, current.connector_id, current.external_id,
                        current.immutable_authority, current.immutable_value,
                        current.record, current.aggregate_version, current.updated_at
                 FROM erpx_identity.directory_identities current
                 WHERE current.connector_id=$1
                   AND current.last_import_id=$2
                   AND current.state='deleted'
                   AND NOT EXISTS (
                       SELECT 1
                       FROM erpx_identity.directory_import_members staged
                       WHERE staged.import_id=$2
                         AND staged.external_id=current.external_id
                   )
                   AND ($3::text IS NULL OR current.external_id > $3)
                 ORDER BY current.external_id
                 LIMIT 500",
            )
            .bind(plan.connector.id)
            .bind(plan.import_id)
            .bind(last_missing_external_id.as_deref())
            .fetch_all(&mut **tx)
            .await?;
            if rows.is_empty() {
                break;
            }
            let mut projection_rows = Vec::with_capacity(rows.len());
            for row in rows {
                let external_id: String = row.try_get("external_id")?;
                let observed_at: DateTime<Utc> = row.try_get("updated_at")?;
                let source_revision: i64 = row.try_get("aggregate_version")?;
                let immutable_link = ImmutableIdentityLink::new(
                    row.try_get::<String, _>("immutable_authority")?,
                    row.try_get::<String, _>("immutable_value")?,
                )
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
                let identity_link = immutable_identity_database_key(&immutable_link);
                projection_rows.push(FullImportProjectionRow {
                    identity_id: row.try_get("id")?,
                    connector_id: row.try_get("connector_id")?,
                    external_id,
                    immutable_authority: immutable_link.authority().to_owned(),
                    immutable_value: immutable_link.value().to_owned(),
                    identity_link,
                    principal_ref: None,
                    active: false,
                    deleted: true,
                    groups: Value::Array(Vec::new()),
                    source_revision,
                    observed_at,
                    revoked_at: Some(observed_at),
                });
            }
            Self::persist_full_import_projection_batch(tx, &projection_rows).await?;
            for mapping in role_mappings {
                let role_rows = projection_rows
                    .iter()
                    .map(|row| {
                        let _source_revision =
                            u64::try_from(row.source_revision).map_err(|_| {
                                SyncStoreError::InvalidRecord(
                                    "directory role sequence is not positive".into(),
                                )
                            })?;
                        Ok(FullImportRoleRow {
                            identity_link: row.identity_link.clone(),
                            immutable_authority: row.immutable_authority.clone(),
                            immutable_value: row.immutable_value.clone(),
                            action: LegacyDirectoryRoleAction::Revoke,
                            reason: DirectoryRoleFactReason::IdentityDeleted,
                            occurred_at: row.observed_at,
                        })
                    })
                    .collect::<Result<Vec<_>, SyncStoreError>>()?;
                role_facts_appended = role_facts_appended.saturating_add(
                    Self::append_full_import_role_facts(tx, plan.connector.id, mapping, &role_rows)
                        .await?,
                );
            }
            last_missing_external_id = projection_rows.last().map(|row| row.external_id.clone());
        }

        Ok(role_facts_appended)
    }

    async fn persist_full_import_projection_batch(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        rows: &[FullImportProjectionRow],
    ) -> Result<(), SyncStoreError> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut links = QueryBuilder::<Postgres>::new(
            "INSERT INTO erpx_identity.directory_identity_links
                (connector_id, external_object_id, identity_link, principal_ref,
                 active, source_revision, observed_at, revoked_at,
                 immutable_authority, immutable_value, identity_id, linked_at) ",
        );
        links.push_values(rows, |mut bind, row| {
            bind.push_bind(row.connector_id)
                .push_bind(&row.external_id)
                .push_bind(&row.identity_link)
                .push_bind(row.principal_ref.as_deref())
                .push_bind(row.active)
                .push_bind(row.source_revision)
                .push_bind(row.observed_at)
                .push_bind(row.revoked_at)
                .push_bind(&row.immutable_authority)
                .push_bind(&row.immutable_value)
                .push_bind(row.identity_id)
                .push_bind(row.observed_at);
        });
        links.push(
            " ON CONFLICT (connector_id, external_object_id) DO UPDATE SET
                identity_link=EXCLUDED.identity_link,
                principal_ref=EXCLUDED.principal_ref,
                active=EXCLUDED.active,
                source_revision=EXCLUDED.source_revision,
                observed_at=EXCLUDED.observed_at,
                revoked_at=EXCLUDED.revoked_at,
                immutable_authority=EXCLUDED.immutable_authority,
                immutable_value=EXCLUDED.immutable_value,
                identity_id=EXCLUDED.identity_id,
                linked_at=EXCLUDED.linked_at
             WHERE erpx_identity.directory_identity_links.identity_id IS NULL
                OR erpx_identity.directory_identity_links.identity_id=EXCLUDED.identity_id",
        );
        links.build().execute(&mut **tx).await?;

        let mut projection = QueryBuilder::<Postgres>::new(
            "INSERT INTO erpx_identity.directory_identity_projection
                (identity_link, connector_id, principal_ref, active, groups_json,
                 source_revision, last_feed_at, revoked_at, updated_at) ",
        );
        projection.push_values(rows, |mut bind, row| {
            bind.push_bind(&row.identity_link)
                .push_bind(row.connector_id)
                .push_bind(row.principal_ref.as_deref())
                .push_bind(row.active)
                .push_bind(row.groups.clone())
                .push_bind(row.source_revision)
                .push_bind(row.observed_at)
                .push_bind(row.revoked_at)
                .push_bind(row.observed_at);
        });
        projection.push(
            " ON CONFLICT (identity_link) DO UPDATE SET
                connector_id=EXCLUDED.connector_id,
                principal_ref=EXCLUDED.principal_ref,
                active=EXCLUDED.active,
                groups_json=EXCLUDED.groups_json,
                source_revision=EXCLUDED.source_revision,
                last_feed_at=EXCLUDED.last_feed_at,
                revoked_at=EXCLUDED.revoked_at,
                updated_at=EXCLUDED.updated_at
             WHERE erpx_identity.directory_identity_projection.source_revision < EXCLUDED.source_revision",
        );
        projection.build().execute(&mut **tx).await?;
        Ok(())
    }

    async fn append_full_import_role_facts(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        connector_id: Uuid,
        mapping: &DirectoryRoleMapping,
        rows: &[FullImportRoleRow],
    ) -> Result<u64, SyncStoreError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let scope_key = mapping
            .scope
            .authorization_key()
            .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
        // A missing tail row cannot be locked with FOR UPDATE. Lock every
        // affected chain explicitly, in deterministic order, before reading
        // tails so concurrent imports cannot both append sequence one (or
        // derive the same previous hash).
        let chain_keys = rows
            .iter()
            .map(|row| {
                format!(
                    "directory-role/{connector_id}/{}/{}/{}",
                    row.identity_link, mapping.role_ref, scope_key
                )
            })
            .collect::<BTreeSet<_>>();
        for chain_key in chain_keys {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(chain_key)
                .execute(&mut **tx)
                .await?;
        }
        let mut previous_query = QueryBuilder::<Postgres>::new(
            "SELECT DISTINCT ON (identity_link) identity_link, sequence, fact_hash
             FROM erpx_identity.directory_role_facts
             WHERE connector_id=",
        );
        previous_query
            .push_bind(connector_id)
            .push(" AND role_ref=")
            .push_bind(&mapping.role_ref)
            .push(" AND scope_key=")
            .push_bind(&scope_key)
            .push(" AND identity_link IN (");
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                previous_query.push(",");
            }
            previous_query.push_bind(&row.identity_link);
        }
        previous_query.push(") ORDER BY identity_link, sequence DESC, occurred_at DESC");
        let previous_rows = previous_query.build().fetch_all(&mut **tx).await?;
        let mut previous = BTreeMap::new();
        for row in previous_rows {
            let sequence = u64::try_from(row.try_get::<i64, _>("sequence")?).map_err(|_| {
                SyncStoreError::InvalidRecord("directory role sequence is negative".into())
            })?;
            previous.insert(
                row.try_get::<String, _>("identity_link")?,
                (sequence, row.try_get::<String, _>("fact_hash")?),
            );
        }

        let mut facts = Vec::with_capacity(rows.len());
        for row in rows {
            let (previous_sequence, previous_hash) = previous
                .get(&row.identity_link)
                .map_or((0, None), |(sequence, hash)| {
                    (*sequence, Some(hash.clone()))
                });
            let sequence = previous_sequence.checked_add(1).ok_or_else(|| {
                SyncStoreError::InvalidRecord("directory role sequence overflow".into())
            })?;
            let immutable_link = ImmutableIdentityLink::new(
                row.immutable_authority.clone(),
                row.immutable_value.clone(),
            )
            .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            let role_scope = match &mapping.scope {
                DirectoryScope::Platform => RbacConnectorScope::Platform,
                DirectoryScope::Holding { .. } => {
                    RbacConnectorScope::Holding(mapping.scope.holding_id().ok_or_else(|| {
                        SyncStoreError::InvalidRecord(
                            "holding role mapping has no stable holding identity".into(),
                        )
                    })?)
                }
                DirectoryScope::Branch {
                    company_id,
                    branch_id,
                    ..
                } => RbacConnectorScope::Branch {
                    holding_id: mapping.scope.holding_id().ok_or_else(|| {
                        SyncStoreError::InvalidRecord(
                            "branch role mapping has no stable holding identity".into(),
                        )
                    })?,
                    company_id: *company_id,
                    branch_id: *branch_id,
                },
            };
            let role_action = match row.action {
                LegacyDirectoryRoleAction::Grant => RbacRoleFactAction::Grant,
                LegacyDirectoryRoleAction::Revoke => RbacRoleFactAction::Revoke,
            };
            let action_name = role_action.as_str();
            let idempotency_key = format!(
                "directory-role-sync/{connector_id}/{}/{}/{}/{}/{}",
                row.identity_link,
                mapping.role_ref,
                scope_key,
                action_name,
                row.occurred_at.timestamp_nanos_opt().unwrap_or_default()
            );
            let input = DirectoryRoleFactInput {
                identity: DirectoryRoleIdentity::from_immutable_link(immutable_link),
                connector_id,
                group_ref: DirectoryGroupRef::try_new(mapping.group_ref.clone())
                    .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?,
                role_ref: DirectoryRoleRef::try_new(mapping.role_ref.clone())
                    .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?,
                scope: role_scope,
                action: role_action,
                reason: row.reason,
                idempotency_key,
                occurred_at: row.occurred_at,
            };
            facts.push(
                RbacRoleFact::from_input(input, sequence, previous_hash)
                    .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?,
            );
        }
        let fact_sequences = facts
            .iter()
            .map(|fact| {
                i64::try_from(fact.sequence()).map_err(|_| {
                    SyncStoreError::InvalidRecord("directory role sequence overflow".into())
                })
            })
            .collect::<Result<Vec<_>, SyncStoreError>>()?;
        let mut appended = 0_u64;
        for (fact, sequence) in facts.iter().zip(fact_sequences.iter()) {
            let result = sqlx::query(
                "INSERT INTO erpx_identity.directory_role_facts
                    (id, connector_id, identity_link, role_ref, scope_key, action,
                     sequence, previous_hash, fact_hash, occurred_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                 ON CONFLICT (fact_hash) DO NOTHING",
            )
            .bind(fact.id())
            .bind(fact.connector_id())
            .bind(fact.identity().database_key())
            .bind(fact.role_ref().as_str())
            .bind(rbac_scope_key(fact.scope()))
            .bind(fact.action().as_str())
            .bind(*sequence)
            .bind(fact.previous_hash())
            .bind(fact.fact_hash())
            .bind(fact.occurred_at())
            .execute(&mut **tx)
            .await?;
            appended = appended.saturating_add(result.rows_affected());

            let event = DirectoryRoleOutboxEvent::from_fact(fact)
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            sqlx::query(
                "INSERT INTO erpx_identity.directory_outbox
                    (event_id, aggregate_id, aggregate_type, event_type,
                     event_version, connector_id, aggregate_version, scope_json,
                     idempotency_key, payload_json, occurred_at)
                 VALUES ($1,$2,$3,$4,1,$5,$6,$7,$8,$9,$10)
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(event.event_id)
            .bind(event.aggregate_id)
            .bind(&event.aggregate_type)
            .bind(&event.event_type)
            .bind(event.connector_id)
            .bind(i64::try_from(event.aggregate_version).map_err(|_| {
                SyncStoreError::InvalidRecord("directory role version overflow".into())
            })?)
            .bind(
                serde_json::to_value(&event.scope)
                    .map_err(|error| SyncStoreError::Serialization(error.to_string()))?,
            )
            .bind(&event.idempotency_key)
            .bind(&event.payload)
            .bind(event.occurred_at)
            .execute(&mut **tx)
            .await?;
        }
        Ok(appended)
    }

    async fn load_native_role_mappings(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        connector_id: Uuid,
        connector_scope: &ConnectorScope,
    ) -> Result<Vec<DirectoryRoleMapping>, SyncStoreError> {
        let connector_scope = match connector_scope {
            ConnectorScope::Platform => DirectoryScope::Platform,
            ConnectorScope::Holding(holding_id) => DirectoryScope::Holding {
                data_profile_key: holding_id.to_string(),
            },
            ConnectorScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => DirectoryScope::Branch {
                data_profile_key: holding_id.to_string(),
                company_id: *company_id,
                branch_id: *branch_id,
            },
        };
        let rows = sqlx::query(
            "SELECT group_ref, role_ref, scope_key, target_scope_key, requires_mfa, active, revision
             FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1
             ORDER BY group_ref, role_ref, scope_key",
        )
        .bind(connector_id)
        .fetch_all(&mut **tx)
        .await?;
        rows.into_iter()
            .map(|row| {
                let scope_key: String = row.try_get("scope_key")?;
                let mapping = DirectoryRoleMapping {
                    connector_id,
                    group_ref: row.try_get("group_ref")?,
                    role_ref: row.try_get("role_ref")?,
                    scope: directory_scope_from_key(&scope_key)?,
                    target_scope: parse_role_target_scope(
                        &row.try_get::<String, _>("target_scope_key")?,
                    )?,
                    requires_mfa: row.try_get("requires_mfa")?,
                    active: row.try_get("active")?,
                    revision: u64::try_from(row.try_get::<i64, _>("revision")?).map_err(|_| {
                        SyncStoreError::InvalidRecord(
                            "persisted directory role mapping revision is negative".into(),
                        )
                    })?,
                };
                mapping
                    .validate_for_connector(connector_id, &connector_scope)
                    .map_err(|error| SyncStoreError::Conflict(error.to_string()))?;
                Ok(mapping)
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    async fn apply_native_projection_and_roles(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        change: &DirectoryChange,
        connector_scope: &ConnectorScope,
        identity_link: &str,
        immutable_link: &ImmutableIdentityLink,
        record: Option<&Value>,
        source_revision: i64,
        observed_at: DateTime<Utc>,
    ) -> Result<(), SyncStoreError> {
        if source_revision <= 0 {
            return Err(SyncStoreError::InvalidRecord(
                "native directory source revision must be positive".into(),
            ));
        }
        let active = change.kind == ChangeKind::Upsert;
        let principal_ref = record
            .and_then(|record| record.get("login_name"))
            .and_then(Value::as_str);
        let groups = record
            .and_then(|record| record.get("groups"))
            .and_then(Value::as_array)
            .map(|groups| {
                groups
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let groups_json = serde_json::to_value(&groups)
            .map_err(|error| SyncStoreError::Serialization(error.to_string()))?;
        sqlx::query(
            "INSERT INTO erpx_identity.directory_identity_projection
                (identity_link, connector_id, principal_ref, active, groups_json,
                 source_revision, last_feed_at, revoked_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,CASE WHEN $4 THEN NULL ELSE $7 END,$7)
             ON CONFLICT (identity_link) DO UPDATE SET
                connector_id=EXCLUDED.connector_id,
                principal_ref=EXCLUDED.principal_ref, active=EXCLUDED.active,
                groups_json=EXCLUDED.groups_json, source_revision=EXCLUDED.source_revision,
                last_feed_at=EXCLUDED.last_feed_at, revoked_at=EXCLUDED.revoked_at,
                updated_at=EXCLUDED.updated_at
             WHERE erpx_identity.directory_identity_projection.source_revision
                   < EXCLUDED.source_revision",
        )
        .bind(identity_link)
        .bind(change.connector_id)
        .bind(principal_ref)
        .bind(active)
        .bind(groups_json)
        .bind(source_revision)
        .bind(observed_at)
        .execute(&mut **tx)
        .await?;

        let mappings =
            Self::load_native_role_mappings(tx, change.connector_id, connector_scope).await?;
        for mapping in mappings {
            let action = if active && mapping.active && groups.contains(&mapping.group_ref) {
                LegacyDirectoryRoleAction::Grant
            } else {
                LegacyDirectoryRoleAction::Revoke
            };
            let reason = match change.kind {
                ChangeKind::Delete => DirectoryRoleFactReason::IdentityDeleted,
                ChangeKind::Disable => DirectoryRoleFactReason::IdentityDisabled,
                ChangeKind::Upsert if !mapping.active => {
                    DirectoryRoleFactReason::MappingDeactivated
                }
                ChangeKind::Upsert if groups.contains(&mapping.group_ref) => {
                    DirectoryRoleFactReason::GroupMatched
                }
                ChangeKind::Upsert => DirectoryRoleFactReason::GroupRemoved,
            };
            Self::append_full_import_role_facts(
                tx,
                change.connector_id,
                &mapping,
                &[FullImportRoleRow {
                    identity_link: identity_link.to_owned(),
                    immutable_authority: immutable_link.authority().to_owned(),
                    immutable_value: immutable_link.value().to_owned(),
                    action,
                    reason,
                    occurred_at: observed_at,
                }],
            )
            .await?;
        }
        Ok(())
    }
}

impl DirectorySyncStore for SqlxDirectorySyncStore {
    fn begin_import<'a>(&'a self, plan: &'a ImportPlan) -> StoreFuture<'a, ImportCheckpoint> {
        Box::pin(async move {
            let (scope_kind, scope_id) = plan.connector.scope.storage_parts();
            let page_size = i32::try_from(plan.config.page_size).map_err(|_| {
                SyncStoreError::InvalidRecord("page size is not representable".into())
            })?;
            let max_users = i64::try_from(plan.config.max_users).map_err(|_| {
                SyncStoreError::InvalidRecord("max users is not representable".into())
            })?;
            let mut tx = self.pool.begin().await?;
            let cursor = serde_json::to_value(FeedCursor::Start)
                .map_err(|error| SyncStoreError::Serialization(error.to_string()))?;
            sqlx::query(
                "INSERT INTO erpx_identity.directory_imports
                    (id, connector_id, scope_kind, scope_id, state, page_size,
                     max_users, cursor, users_seen, pages_applied,
                     snapshot_digest, started_at, updated_at)
                 VALUES ($1, $2, $3, $4, 'running', $5, $6, $7, 0, 0, $8, $9, $9)
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(plan.import_id)
            .bind(plan.connector.id)
            .bind(scope_kind)
            .bind(scope_id)
            .bind(page_size)
            .bind(max_users)
            .bind(cursor)
            .bind(Vec::<u8>::from([0_u8; 32]))
            .bind(plan.started_at)
            .execute(&mut *tx)
            .await?;
            let binding = sqlx::query(
                "SELECT i.connector_id, i.scope_kind, i.scope_id, i.page_size,
                        i.max_users, i.state, c.max_in_flight_pages,
                        c.scope_kind AS connector_scope_kind,
                        c.scope_id AS connector_scope_id,
                        c.vendor, c.feed_kind, c.enabled
                 FROM erpx_identity.directory_imports AS i
                 JOIN erpx_identity.directory_connectors AS c
                   ON c.id = i.connector_id
                 WHERE i.id=$1 FOR UPDATE",
            )
            .bind(plan.import_id)
            .fetch_one(&mut *tx)
            .await?;
            let stored_connector_id: Uuid = binding.try_get("connector_id")?;
            let stored_scope_kind: String = binding.try_get("scope_kind")?;
            let stored_scope_id: Option<Uuid> = binding.try_get("scope_id")?;
            let stored_page_size: i32 = binding.try_get("page_size")?;
            let stored_max_users: i64 = binding.try_get("max_users")?;
            let stored_state: String = binding.try_get("state")?;
            let stored_max_in_flight_pages: i32 = binding.try_get("max_in_flight_pages")?;
            let connector_scope = scope_from_storage(
                &binding.try_get::<String, _>("connector_scope_kind")?,
                binding.try_get("connector_scope_id")?,
            )?;
            let stored_vendor: String = binding.try_get("vendor")?;
            let stored_feed_kind: String = binding.try_get("feed_kind")?;
            let connector_enabled: bool = binding.try_get("enabled")?;
            let stored_max_in_flight_pages =
                u16::try_from(stored_max_in_flight_pages).map_err(|_| {
                    SyncStoreError::InvalidRecord(
                        "persisted directory concurrency is invalid".into(),
                    )
                })?;
            if stored_connector_id != plan.connector.id
                || stored_scope_kind != scope_kind
                || stored_scope_id != scope_id
                || stored_page_size != page_size
                || stored_max_users != max_users
                || stored_max_in_flight_pages != plan.config.max_in_flight_pages
            {
                return Err(SyncStoreError::Conflict(
                    "resumed directory import does not match its persisted connector, scope, capacity, or concurrency contract".into(),
                ));
            }
            if connector_scope != plan.connector.scope
                || stored_vendor != feed_vendor_storage_name(plan.connector.vendor)
                || stored_feed_kind != feed_kind_storage_name(plan.connector.feed_kind)
            {
                return Err(SyncStoreError::Conflict(
                    "directory import connector protocol or scope changed in durable storage"
                        .into(),
                ));
            }
            if !connector_enabled {
                return Err(SyncStoreError::Conflict(
                    "directory connector is disabled".into(),
                ));
            }
            if !matches!(stored_state.as_str(), "running" | "finalizing") {
                return Err(SyncStoreError::Conflict(
                    "only a running or finalizing directory import can be resumed".into(),
                ));
            }
            let checkpoint = self.load_checkpoint(plan.import_id, &mut *tx).await?;
            if checkpoint.connector_id != plan.connector.id {
                return Err(SyncStoreError::Conflict(
                    "import ID is bound to another connector".into(),
                ));
            }
            tx.commit().await?;
            Ok(checkpoint)
        })
    }

    fn stage_page<'a>(
        &'a self,
        plan: &'a ImportPlan,
        checkpoint: &'a ImportCheckpoint,
        page: &'a DirectoryPage,
        next_checkpoint: &'a ImportCheckpoint,
    ) -> StoreFuture<'a, ApplyPageReceipt> {
        Box::pin(async move {
            page.validate(plan.config.page_size)
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            validate_page_feed_cursor(plan.connector.feed_kind, page)
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            let page_key = page
                .idempotency_key(plan.import_id)
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            let page_digest = page
                .digest()
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            let mut tx = self.pool.begin().await?;
            if let Some(existing) = sqlx::query(
                "SELECT page_digest
                 FROM erpx_identity.directory_import_pages
                 WHERE import_id=$1 AND idempotency_key=$2",
            )
            .bind(plan.import_id)
            .bind(&page_key)
            .fetch_optional(&mut *tx)
            .await?
            {
                let existing_digest: Vec<u8> = existing.try_get("page_digest")?;
                if existing_digest != page_digest {
                    return Err(SyncStoreError::Conflict(
                        "directory page idempotency key is bound to different content".into(),
                    ));
                }
                let current = self.load_checkpoint(plan.import_id, &mut *tx).await?;
                tx.commit().await?;
                return Ok(ApplyPageReceipt {
                    checkpoint: current,
                    records_applied: 0,
                    duplicate_page: true,
                });
            }
            let persisted_checkpoint = self.load_checkpoint(plan.import_id, &mut *tx).await?;
            if persisted_checkpoint != *checkpoint {
                return Err(SyncStoreError::Conflict(
                    "directory page checkpoint is not the committed import checkpoint".into(),
                ));
            }
            let expected_checkpoint = checkpoint
                .accept_page(page, &plan.config, next_checkpoint.updated_at)
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            if expected_checkpoint != *next_checkpoint {
                return Err(SyncStoreError::Conflict(
                    "directory page next checkpoint is not the authoritative state transition"
                        .into(),
                ));
            }
            let marker = sqlx::query(
                "INSERT INTO erpx_identity.directory_import_pages
                    (import_id, idempotency_key, page_digest, cursor,
                     next_checkpoint, records_count, applied_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (import_id, idempotency_key) DO NOTHING",
            )
            .bind(plan.import_id)
            .bind(&page_key)
            .bind(Vec::from(page_digest))
            .bind(
                serde_json::to_value(page.cursor())
                    .map_err(|error| SyncStoreError::Serialization(error.to_string()))?,
            )
            .bind(
                serde_json::to_value(next_checkpoint)
                    .map_err(|error| SyncStoreError::Serialization(error.to_string()))?,
            )
            .bind(i32::try_from(page.records.len()).map_err(|_| {
                SyncStoreError::InvalidRecord("page record count is not representable".into())
            })?)
            .bind(next_checkpoint.updated_at)
            .execute(&mut *tx)
            .await?;
            if marker.rows_affected() == 0 {
                let current = self.load_checkpoint(plan.import_id, &mut *tx).await?;
                tx.commit().await?;
                return Ok(ApplyPageReceipt {
                    checkpoint: current,
                    records_applied: 0,
                    duplicate_page: true,
                });
            }
            if checkpoint.import_id != plan.import_id
                || checkpoint.connector_id != plan.connector.id
            {
                return Err(SyncStoreError::Conflict(
                    "checkpoint is not bound to this import".into(),
                ));
            }
            let directory_authority =
                Self::load_directory_authority(&mut tx, plan.connector.id).await?;
            for record in &page.records {
                let record_digest = record
                    .record_digest()
                    .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
                let identity_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    format!(
                        "directory-identity/{}/{}",
                        plan.connector.id,
                        record.external_id()
                    )
                    .as_bytes(),
                );
                let idempotency_key = format!(
                    "directory-import/{}/{}/{}",
                    plan.import_id,
                    plan.connector.id,
                    record.external_id()
                );
                let event_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, idempotency_key.as_bytes());
                let result = sqlx::query(
                    "INSERT INTO erpx_identity.directory_import_members
                        (import_id, identity_id, connector_id, external_id,
                         immutable_authority, immutable_value, state,
                         record_digest, record, event_id, idempotency_key, observed_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                     ON CONFLICT (import_id, external_id) DO UPDATE
                       SET record_digest = EXCLUDED.record_digest,
                           record = EXCLUDED.record,
                           observed_at = EXCLUDED.observed_at
                       WHERE erpx_identity.directory_import_members.record_digest =
                             EXCLUDED.record_digest",
                )
                .bind(plan.import_id)
                .bind(identity_id)
                .bind(plan.connector.id)
                .bind(record.external_id())
                .bind(&directory_authority)
                .bind(record.immutable_link().value())
                .bind(record.state().as_str())
                .bind(Vec::from(record_digest))
                .bind(
                    serde_json::to_value(record)
                        .map_err(|error| SyncStoreError::Serialization(error.to_string()))?,
                )
                .bind(event_id)
                .bind(&idempotency_key)
                .bind(next_checkpoint.updated_at)
                .execute(&mut *tx)
                .await?;
                if result.rows_affected() == 0 {
                    let existing: Vec<u8> = sqlx::query_scalar(
                        "SELECT record_digest FROM erpx_identity.directory_import_members
                         WHERE import_id=$1 AND external_id=$2",
                    )
                    .bind(plan.import_id)
                    .bind(record.external_id())
                    .fetch_one(&mut *tx)
                    .await?;
                    if existing != record_digest {
                        return Err(SyncStoreError::Conflict(format!(
                            "external ID {} changed immutable page content during import",
                            record.external_id()
                        )));
                    }
                }
            }
            sqlx::query(
                "UPDATE erpx_identity.directory_imports
                 SET cursor=$2, users_seen=$3, pages_applied=$4,
                     last_external_id=$5, snapshot_digest=$6,
                     state=$7, updated_at=$8
                 WHERE id=$1 AND state='running'",
            )
            .bind(plan.import_id)
            .bind(
                serde_json::to_value(&next_checkpoint.cursor)
                    .map_err(|error| SyncStoreError::Serialization(error.to_string()))?,
            )
            .bind(i64::try_from(next_checkpoint.users_seen).map_err(|_| {
                SyncStoreError::InvalidRecord("user count is not representable".into())
            })?)
            .bind(i64::try_from(next_checkpoint.pages_applied).map_err(|_| {
                SyncStoreError::InvalidRecord("page count is not representable".into())
            })?)
            .bind(&next_checkpoint.last_external_id)
            .bind(Vec::from(next_checkpoint.snapshot_digest))
            .bind(match next_checkpoint.state {
                ImportState::Running => "running",
                ImportState::Finalizing => "finalizing",
                _ => {
                    return Err(SyncStoreError::InvalidRecord(
                        "invalid next import state".into(),
                    ));
                }
            })
            .bind(next_checkpoint.updated_at)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(ApplyPageReceipt {
                checkpoint: next_checkpoint.clone(),
                records_applied: page.records.len() as u64,
                duplicate_page: false,
            })
        })
    }

    fn finalize_import<'a>(
        &'a self,
        plan: &'a ImportPlan,
        checkpoint: &'a ImportCheckpoint,
        role_mappings: &'a [DirectoryRoleMapping],
        completed_at: DateTime<Utc>,
    ) -> StoreFuture<'a, ImportSummary> {
        Box::pin(async move {
            if checkpoint.state != ImportState::Finalizing {
                return Err(SyncStoreError::Conflict(
                    "only a terminal checkpoint can be finalized".into(),
                ));
            }
            let mut tx = self.pool.begin().await?;
            // Fence the canonical snapshot commit against native feed
            // delivery for this connector. Staging remains resumable, but
            // the final identity/projection/role transition is serialized
            // with the native path so one cannot overwrite the other.
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(format!("directory-connector/{}", plan.connector.id))
                .execute(&mut *tx)
                .await?;
            let state: String = sqlx::query_scalar(
                "SELECT state FROM erpx_identity.directory_imports
                 WHERE id=$1 FOR UPDATE",
            )
            .bind(plan.import_id)
            .fetch_one(&mut *tx)
            .await?;
            if state != "finalizing" {
                return Err(SyncStoreError::Conflict(
                    "directory import is not in finalizing state".into(),
                ));
            }
            let persisted_checkpoint = self.load_checkpoint(plan.import_id, &mut *tx).await?;
            if persisted_checkpoint != *checkpoint {
                return Err(SyncStoreError::Conflict(
                    "directory finalization checkpoint is not the committed import checkpoint"
                        .into(),
                ));
            }
            let immutable_conflict: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1
                    FROM erpx_identity.directory_import_members staged
                    JOIN erpx_identity.directory_identities current
                      ON current.connector_id=staged.connector_id
                     AND current.external_id=staged.external_id
                    WHERE staged.import_id=$1
                      AND (current.immutable_authority <> staged.immutable_authority
                       OR current.immutable_value <> staged.immutable_value)
                )",
            )
            .bind(plan.import_id)
            .fetch_one(&mut *tx)
            .await?;
            if immutable_conflict {
                return Err(SyncStoreError::Conflict(
                    "mutable external ID attempted to replace an immutable identity link".into(),
                ));
            }
            let legacy_link_conflict: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1
                    FROM erpx_identity.directory_import_members staged
                    JOIN erpx_identity.directory_identity_links link
                      ON link.connector_id=staged.connector_id
                     AND link.external_object_id=staged.external_id
                    WHERE staged.import_id=$1
                      AND (
                          (link.immutable_authority IS NULL)
                            <> (link.immutable_value IS NULL)
                          OR (
                              link.immutable_authority IS NOT NULL
                              AND link.immutable_value IS NOT NULL
                              AND (link.immutable_authority <> staged.immutable_authority
                               OR link.immutable_value <> staged.immutable_value)
                          )
                      )
                )",
            )
            .bind(plan.import_id)
            .fetch_one(&mut *tx)
            .await?;
            if legacy_link_conflict {
                return Err(SyncStoreError::Conflict(
                    "directory identity link history attempted to rebind an external ID".into(),
                ));
            }
            let link_conflict: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1
                    FROM erpx_identity.directory_import_members staged
                    JOIN erpx_identity.directory_identity_links link
                      ON link.immutable_authority=staged.immutable_authority
                     AND link.immutable_value=staged.immutable_value
                    WHERE staged.import_id=$1
                      AND link.identity_id <> staged.identity_id
                )",
            )
            .bind(plan.import_id)
            .fetch_one(&mut *tx)
            .await?;
            if link_conflict {
                return Err(SyncStoreError::Conflict(
                    "immutable directory link is already bound to another identity".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO erpx_identity.directory_identities
                    (id, connector_id, scope_kind, scope_id, external_id,
                     immutable_authority, immutable_value, state, record,
                     aggregate_version,
                     last_import_id, updated_at, created_at)
                 SELECT identity_id, connector_id, $2, $3, external_id,
                        immutable_authority, immutable_value, state, record, 1,
                        import_id, observed_at, observed_at
                 FROM erpx_identity.directory_import_members
                 WHERE import_id=$1
                 ON CONFLICT (connector_id, external_id) DO UPDATE
                   SET state=EXCLUDED.state, record=EXCLUDED.record,
                       aggregate_version=erpx_identity.directory_identities.aggregate_version + 1,
                       last_import_id=EXCLUDED.last_import_id,
                       updated_at=EXCLUDED.updated_at
                   WHERE erpx_identity.directory_identities.immutable_authority =
                             EXCLUDED.immutable_authority
                     AND erpx_identity.directory_identities.immutable_value =
                             EXCLUDED.immutable_value",
            )
            .bind(plan.import_id)
            .bind(plan.connector.scope.storage_parts().0)
            .bind(plan.connector.scope.storage_parts().1)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO erpx_identity.directory_outbox
                    (event_id, aggregate_id, aggregate_type, event_type,
                     event_version, connector_id, aggregate_version, scope_json,
                     idempotency_key, payload_json, occurred_at)
                 SELECT staged.event_id,
                        current.id, 'directory_identity',
                        CASE staged.state
                          WHEN 'active' THEN 'directory.identity_upserted.v1'
                          WHEN 'disabled' THEN 'directory.identity_disabled.v1'
                          ELSE 'directory.identity_deleted.v1'
                        END,
                        1, staged.connector_id, current.aggregate_version,
                        jsonb_build_object(
                            'kind', $2,
                            'holding_id', $3,
                            'connector_id', staged.connector_id
                        ),
                        format('directory-import/%s/%s/%s', staged.import_id,
                               staged.connector_id, staged.external_id),
                        jsonb_build_object('event',
                          CASE staged.state
                            WHEN 'active' THEN 'directory.identity_upserted.v1'
                            WHEN 'disabled' THEN 'directory.identity_disabled.v1'
                            ELSE 'directory.identity_deleted.v1'
                          END,
                          'import_id', staged.import_id,
                                           'connector_id', staged.connector_id,
                                           'identity_id', current.id,
                                           'record', staged.record),
                        staged.observed_at
                 FROM erpx_identity.directory_import_members staged
                 JOIN erpx_identity.directory_identities current
                   ON current.connector_id=staged.connector_id
                  AND current.external_id=staged.external_id
                 WHERE staged.import_id=$1
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(plan.import_id)
            .bind(plan.connector.scope.storage_parts().0)
            .bind(plan.connector.scope.storage_parts().1)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO erpx_identity.directory_revocation_sla
                    (event_id, connector_id, external_id,
                     immutable_authority, immutable_value,
                     observed_at, deadline, reason, status, applied_at)
                 SELECT staged.event_id, staged.connector_id, staged.external_id,
                        staged.immutable_authority, staged.immutable_value,
                        staged.observed_at, staged.observed_at + INTERVAL '5 minutes',
                        CASE staged.state
                          WHEN 'disabled' THEN 'directory_account_disabled'
                          WHEN 'deleted' THEN 'directory_account_deleted'
                        END,
                        CASE WHEN CURRENT_TIMESTAMP <= staged.observed_at + INTERVAL '5 minutes'
                             THEN 'applied' ELSE 'breached' END,
                        CASE WHEN CURRENT_TIMESTAMP <= staged.observed_at + INTERVAL '5 minutes'
                             THEN CURRENT_TIMESTAMP ELSE NULL END
                 FROM erpx_identity.directory_import_members staged
                 WHERE staged.import_id=$1 AND staged.state <> 'active'
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(plan.import_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO erpx_identity.directory_identity_links
                    (immutable_authority, immutable_value, connector_id,
                     external_object_id, identity_id, linked_at)
                 SELECT immutable_authority, immutable_value, connector_id,
                        external_id, identity_id, observed_at
                 FROM erpx_identity.directory_import_members
                 WHERE import_id=$1
                 ON CONFLICT DO NOTHING",
            )
            .bind(plan.import_id)
            .execute(&mut *tx)
            .await?;
            let revoked: i64 = sqlx::query_scalar(
                "WITH changed AS (
                    UPDATE erpx_identity.directory_identities current
                       SET state='deleted', aggregate_version=aggregate_version + 1,
                           last_import_id=$2, updated_at=$3
                     WHERE current.connector_id=$1
                       AND current.state <> 'deleted'
                       AND NOT EXISTS (
                           SELECT 1 FROM erpx_identity.directory_import_members staged
                            WHERE staged.import_id=$2
                              AND staged.external_id=current.external_id
                       )
                    RETURNING 1
                ) SELECT count(*) FROM changed",
            )
            .bind(plan.connector.id)
            .bind(plan.import_id)
            .bind(completed_at)
            .fetch_one(&mut *tx)
            .await?;
            let staged_revocations: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_identity.directory_import_members
                 WHERE import_id=$1 AND state <> 'active'",
            )
            .bind(plan.import_id)
            .fetch_one(&mut *tx)
            .await?;
            let revocations_due = revoked.checked_add(staged_revocations).ok_or_else(|| {
                SyncStoreError::Conflict("directory revocation count overflow".into())
            })?;
            sqlx::query(
                "WITH changed AS (
                    SELECT current.id, current.connector_id, current.external_id,
                           current.immutable_authority, current.immutable_value,
                           current.aggregate_version, current.updated_at
                    FROM erpx_identity.directory_identities current
                    WHERE current.connector_id=$1
                      AND current.last_import_id=$2
                      AND current.state='deleted'
                      AND NOT EXISTS (
                          SELECT 1 FROM erpx_identity.directory_import_members staged
                           WHERE staged.import_id=$2
                             AND staged.external_id=current.external_id
                      )
                )
                INSERT INTO erpx_identity.directory_outbox
                    (event_id, aggregate_id, aggregate_type, event_type,
                     event_version, connector_id, aggregate_version, scope_json,
                     idempotency_key, payload_json, occurred_at)
                SELECT md5($2::text || '/revocation/' || changed.id::text ||
                           '/' || changed.aggregate_version::text)::uuid,
                       changed.id, 'directory_identity',
                       'directory.identity_deleted.v1',
                       1, changed.connector_id, changed.aggregate_version,
                       jsonb_build_object(
                           'kind', $3,
                           'holding_id', $4,
                           'connector_id', changed.connector_id
                       ),
                       format('directory-full-revocation/%s/%s/%s', $2,
                              changed.id, changed.aggregate_version),
                       jsonb_build_object('event', 'directory.identity_deleted.v1',
                                          'import_id', $2,
                                          'connector_id', changed.connector_id,
                                          'identity_id', changed.id,
                                          'reason', 'missing_from_full_snapshot'),
                       changed.updated_at
                FROM changed
                ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(plan.connector.id)
            .bind(plan.import_id)
            .bind(plan.connector.scope.storage_parts().0)
            .bind(plan.connector.scope.storage_parts().1)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "WITH changed AS (
                    SELECT current.id, current.connector_id, current.external_id,
                           current.immutable_authority, current.immutable_value,
                           current.aggregate_version, current.updated_at
                    FROM erpx_identity.directory_identities current
                    WHERE current.connector_id=$1
                      AND current.last_import_id=$2
                      AND current.state='deleted'
                      AND NOT EXISTS (
                          SELECT 1 FROM erpx_identity.directory_import_members staged
                           WHERE staged.import_id=$2
                             AND staged.external_id=current.external_id
                      )
                )
                INSERT INTO erpx_identity.directory_revocation_sla
                    (event_id, connector_id, external_id,
                     immutable_authority, immutable_value,
                     observed_at, deadline, reason, status, applied_at)
                SELECT md5($2::text || '/revocation/' || changed.id::text ||
                           '/' || changed.aggregate_version::text)::uuid,
                       changed.connector_id, changed.external_id,
                       changed.immutable_authority, changed.immutable_value,
                       changed.updated_at, changed.updated_at + INTERVAL '5 minutes',
                       'missing_from_full_snapshot',
                       CASE WHEN CURRENT_TIMESTAMP <= changed.updated_at + INTERVAL '5 minutes'
                            THEN 'applied' ELSE 'breached' END,
                       CASE WHEN CURRENT_TIMESTAMP <= changed.updated_at + INTERVAL '5 minutes'
                            THEN CURRENT_TIMESTAMP ELSE NULL END
                FROM changed
                ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(plan.connector.id)
            .bind(plan.import_id)
            .execute(&mut *tx)
            .await?;
            let role_facts_appended = Self::apply_full_import_projection_and_roles(
                &mut tx,
                plan,
                role_mappings,
                completed_at,
            )
            .await?;
            sqlx::query(
                "UPDATE erpx_identity.directory_imports
                 SET state='completed', finalized_at=$2, updated_at=$2
                 WHERE id=$1",
            )
            .bind(plan.import_id)
            .bind(completed_at)
            .execute(&mut *tx)
            .await?;
            // The coordinator registers a projector against this durable
            // source outbox. Per-identity events above and this import-level
            // event are committed together with canonical state.
            let event = DirectoryOutboxEvent {
                event_id: Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    format!("directory-import-completed/{}", plan.import_id).as_bytes(),
                ),
                event_type: "directory.import_completed.v1".into(),
                aggregate_type: DIRECTORY_IMPORT_AGGREGATE.into(),
                aggregate_id: plan.import_id,
                aggregate_version: 1,
                connector_id: plan.connector.id,
                scope: plan.connector.scope.clone(),
                idempotency_key: format!("directory-import-completed/{}", plan.import_id),
                payload: serde_json::json!({
                    "import_id": plan.import_id,
                    "connector_id": plan.connector.id,
                    "users_seen": checkpoint.users_seen,
                    "pages_applied": checkpoint.pages_applied,
                    "snapshot_digest": hex_digest(&checkpoint.snapshot_digest),
                    "revocations_due": revocations_due,
                    "role_facts_appended": role_facts_appended,
                }),
                occurred_at: completed_at,
            };
            Self::insert_outbox_event(&mut tx, &event, None).await?;
            tx.commit().await?;
            Ok(ImportSummary {
                import_id: plan.import_id,
                connector_id: plan.connector.id,
                users_seen: checkpoint.users_seen,
                pages_applied: checkpoint.pages_applied,
                source_digest: checkpoint.snapshot_digest,
                completed_at,
                revocations_due: u64::try_from(revocations_due).map_err(|_| {
                    SyncStoreError::InvalidRecord("directory revocation count is negative".into())
                })?,
            })
        })
    }

    fn rollback_import<'a>(
        &'a self,
        import_id: Uuid,
        reason: &'a str,
        at: DateTime<Utc>,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await?;
            let import = sqlx::query(
                "SELECT connector_id, scope_kind, scope_id, state
                 FROM erpx_identity.directory_imports
                 WHERE id=$1 FOR UPDATE",
            )
            .bind(import_id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(import) = import else {
                return Err(SyncStoreError::Conflict(
                    "directory import does not exist".into(),
                ));
            };
            let state: String = import.try_get("state")?;
            if state == "completed" || state == "rolled_back" {
                tx.commit().await?;
                return Ok(());
            }
            let connector_id: Uuid = import.try_get("connector_id")?;
            let scope = scope_from_storage(
                &import.try_get::<String, _>("scope_kind")?,
                import.try_get("scope_id")?,
            )?;
            sqlx::query("DELETE FROM erpx_identity.directory_import_members WHERE import_id=$1")
                .bind(import_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE erpx_identity.directory_imports
                 SET state='rolled_back', failure_reason=$2, updated_at=$3
                 WHERE id=$1 AND state <> 'completed'",
            )
            .bind(import_id)
            .bind(reason)
            .bind(at)
            .execute(&mut *tx)
            .await?;
            let event = DirectoryOutboxEvent {
                event_id: Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    format!("directory-import-rolled-back/{import_id}").as_bytes(),
                ),
                event_type: "directory.import_rolled_back.v1".into(),
                aggregate_type: DIRECTORY_IMPORT_AGGREGATE.into(),
                aggregate_id: import_id,
                aggregate_version: 1,
                connector_id,
                scope,
                idempotency_key: format!("directory-import-rolled-back/{import_id}"),
                payload: serde_json::json!({
                    "import_id": import_id,
                    "connector_id": connector_id,
                    "reason": reason.chars().take(512).collect::<String>(),
                }),
                occurred_at: at,
            };
            Self::insert_outbox_event(&mut tx, &event, None).await?;
            tx.commit().await?;
            Ok(())
        })
    }

    fn save_change_checkpoint<'a>(
        &'a self,
        connector_id: Uuid,
        feed: FeedKind,
        previous_cursor: &'a FeedCursor,
        cursor: &'a FeedCursor,
        at: DateTime<Utc>,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            if connector_id.is_nil() {
                return Err(SyncStoreError::InvalidRecord(
                    "directory checkpoint connector ID is nil".into(),
                ));
            }
            cursor
                .validate()
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            if !feed_accepts_cursor(feed, cursor) {
                return Err(SyncStoreError::Conflict(
                    "directory checkpoint cursor does not match the configured feed".into(),
                ));
            }
            previous_cursor
                .validate()
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            if !matches!(previous_cursor, FeedCursor::Start)
                && !feed_accepts_cursor(feed, previous_cursor)
            {
                return Err(SyncStoreError::Conflict(
                    "directory checkpoint predecessor does not match the configured feed".into(),
                ));
            }
            let (native_sequence, opaque_cookie) = checkpoint_parts(cursor)?;
            let previous_checkpoint = if matches!(previous_cursor, FeedCursor::Start) {
                None
            } else {
                Some(checkpoint_parts(previous_cursor)?)
            };
            let mut tx = self.pool.begin().await?;
            let connector_feed: String = sqlx::query_scalar(
                "SELECT feed_kind
                 FROM erpx_identity.directory_connectors
                 WHERE id=$1
                 FOR SHARE",
            )
            .bind(connector_id)
            .fetch_one(&mut *tx)
            .await?;
            if feed_kind_storage_name(feed) != connector_feed {
                return Err(SyncStoreError::Conflict(
                    "directory checkpoint feed does not match the persisted connector".into(),
                ));
            }
            let existing = sqlx::query(
                "SELECT sequence, opaque_cookie
                 FROM erpx_identity.directory_sync_checkpoints
                 WHERE connector_id=$1
                 FOR UPDATE",
            )
            .bind(connector_id)
            .fetch_optional(&mut *tx)
            .await?;
            match (existing.as_ref(), previous_checkpoint) {
                (None, None) => {}
                (None, Some(_)) => {
                    return Err(SyncStoreError::Conflict(
                        "directory checkpoint predecessor is not persisted".into(),
                    ));
                }
                (Some(_), None) => {
                    return Err(SyncStoreError::Conflict(
                        "directory checkpoint predecessor restarted from Start".into(),
                    ));
                }
                (Some(existing), Some((previous_sequence, previous_cookie))) => {
                    let existing_sequence: i64 = existing.try_get("sequence")?;
                    let existing_cookie: Option<String> = existing.try_get("opaque_cookie")?;
                    let previous_sequence = previous_sequence
                        .map(i64::try_from)
                        .transpose()
                        .map_err(|_| {
                            SyncStoreError::InvalidRecord(
                                "directory checkpoint predecessor sequence overflow".into(),
                            )
                        })?;
                    if existing_sequence != previous_sequence.unwrap_or(existing_sequence)
                        || existing_cookie != previous_cookie
                    {
                        return Err(SyncStoreError::Conflict(
                            "directory checkpoint predecessor is stale or forked".into(),
                        ));
                    }
                }
            }
            let sequence = if let Some(existing) = existing {
                let existing_sequence: i64 = existing.try_get("sequence")?;
                let existing_cookie: Option<String> = existing.try_get("opaque_cookie")?;
                if existing_cookie == opaque_cookie {
                    tx.commit().await?;
                    return Ok(());
                }
                let existing_sequence = u64::try_from(existing_sequence).map_err(|_| {
                    SyncStoreError::InvalidRecord(
                        "persisted directory checkpoint sequence is negative".into(),
                    )
                })?;
                let sequence =
                    native_sequence.unwrap_or_else(|| existing_sequence.saturating_add(1));
                if sequence <= existing_sequence {
                    return Err(SyncStoreError::Conflict(
                        "directory native checkpoint regressed or changed without advancing".into(),
                    ));
                }
                sequence
            } else {
                native_sequence.unwrap_or(1)
            };
            sqlx::query(
                "INSERT INTO erpx_identity.directory_sync_checkpoints
                    (connector_id, feed_kind, sequence, opaque_cookie,
                     last_success_at, status, last_error_redacted)
                 VALUES ($1,$2,$3,$4,$5,'healthy',NULL)
                 ON CONFLICT (connector_id) DO UPDATE SET
                    feed_kind=EXCLUDED.feed_kind,
                    sequence=EXCLUDED.sequence,
                    opaque_cookie=EXCLUDED.opaque_cookie,
                    last_success_at=EXCLUDED.last_success_at,
                    status='healthy',
                    last_error_redacted=NULL
                 WHERE erpx_identity.directory_sync_checkpoints.sequence
                       < EXCLUDED.sequence",
            )
            .bind(connector_id)
            .bind(feed_kind_storage_name(feed))
            .bind(i64::try_from(sequence).map_err(|_| {
                SyncStoreError::InvalidRecord("directory checkpoint sequence overflow".into())
            })?)
            .bind(opaque_cookie)
            .bind(at)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(())
        })
    }

    fn reconcile_import<'a>(
        &'a self,
        import_id: Uuid,
        checked_at: DateTime<Utc>,
    ) -> StoreFuture<'a, ReconciliationReport> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await?;
            let row = sqlx::query(
                "SELECT connector_id, scope_kind, scope_id,
                        users_seen, snapshot_digest, state
                 FROM erpx_identity.directory_imports
                 WHERE id=$1",
            )
            .bind(import_id)
            .fetch_one(&mut *tx)
            .await?;
            let state: String = row.try_get("state")?;
            if state != "completed" {
                return Err(SyncStoreError::Conflict(
                    "only a completed directory import can be reconciled".into(),
                ));
            }
            let expected_users = non_negative_count(row.try_get("users_seen")?)?;
            let expected_digest: [u8; 32] = row
                .try_get::<Vec<u8>, _>("snapshot_digest")?
                .try_into()
                .map_err(|_| {
                    SyncStoreError::InvalidRecord(
                        "directory import snapshot digest is not 32 bytes".into(),
                    )
                })?;
            let connector_id: Uuid = row.try_get("connector_id")?;
            let scope = scope_from_storage(
                &row.try_get::<String, _>("scope_kind")?,
                row.try_get("scope_id")?,
            )?;
            let staged_users = non_negative_count(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*)
                 FROM erpx_identity.directory_import_members
                 WHERE import_id=$1",
                )
                .bind(import_id)
                .fetch_one(&mut *tx)
                .await?,
            )?;
            let canonical_users = non_negative_count(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*)
                 FROM erpx_identity.directory_import_members staged
                 JOIN erpx_identity.directory_identities current
                   ON current.connector_id=staged.connector_id
                  AND current.external_id=staged.external_id
                 WHERE staged.import_id=$1",
                )
                .bind(import_id)
                .fetch_one(&mut *tx)
                .await?,
            )?;
            let mismatched_rows: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_identity.directory_import_members staged
                 JOIN erpx_identity.directory_identities current
                   ON current.connector_id=staged.connector_id
                  AND current.external_id=staged.external_id
                 WHERE staged.import_id=$1
                   AND (current.immutable_authority <> staged.immutable_authority
                    OR current.immutable_value <> staged.immutable_value
                    OR current.state <> staged.state
                    OR current.record IS DISTINCT FROM staged.record)",
            )
            .bind(import_id)
            .fetch_one(&mut *tx)
            .await?;
            let missing_non_deleted: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_identity.directory_identities current
                 WHERE current.last_import_id=$1
                   AND current.state <> 'deleted'
                   AND NOT EXISTS (
                       SELECT 1
                       FROM erpx_identity.directory_import_members staged
                       WHERE staged.import_id=$1
                         AND staged.external_id=current.external_id
                   )",
            )
            .bind(import_id)
            .fetch_one(&mut *tx)
            .await?;
            let link_mismatches: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_identity.directory_import_members staged
                 LEFT JOIN erpx_identity.directory_identity_links link
                   ON link.connector_id=staged.connector_id
                  AND link.external_object_id=staged.external_id
                 WHERE staged.import_id=$1
                   AND (
                       link.identity_id IS DISTINCT FROM staged.identity_id
                       OR link.immutable_authority IS DISTINCT FROM staged.immutable_authority
                       OR link.immutable_value IS DISTINCT FROM staged.immutable_value
                       OR link.identity_link IS NULL
                   )",
            )
            .bind(import_id)
            .fetch_one(&mut *tx)
            .await?;
            let projection_mismatches: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_identity.directory_import_members staged
                 JOIN erpx_identity.directory_identity_links link
                   ON link.connector_id=staged.connector_id
                  AND link.external_object_id=staged.external_id
                 LEFT JOIN erpx_identity.directory_identity_projection projection
                   ON projection.identity_link=link.identity_link
                 WHERE staged.import_id=$1
                   AND (
                       projection.identity_link IS NULL
                       OR projection.connector_id <> staged.connector_id
                       OR projection.active <> (staged.state = 'active')
                       OR (
                           staged.state = 'active'
                           AND projection.revoked_at IS NOT NULL
                       )
                       OR (
                           staged.state <> 'active'
                           AND projection.revoked_at IS NULL
                       )
                   )",
            )
            .bind(import_id)
            .fetch_one(&mut *tx)
            .await?;
            let expected_import_events = i64::try_from(staged_users).map_err(|_| {
                SyncStoreError::InvalidRecord(
                    "expected directory import event count is not representable".into(),
                )
            })?;
            let import_event_prefix = format!("directory-import/{import_id}/%");
            let observed_import_events: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_identity.directory_outbox
                 WHERE idempotency_key LIKE $1",
            )
            .bind(import_event_prefix)
            .fetch_one(&mut *tx)
            .await?;
            let completed_events: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_identity.directory_outbox
                 WHERE idempotency_key=$1",
            )
            .bind(format!("directory-import-completed/{import_id}"))
            .fetch_one(&mut *tx)
            .await?;
            let integrity_violations = [
                mismatched_rows,
                missing_non_deleted,
                link_mismatches,
                projection_mismatches,
                (observed_import_events != expected_import_events) as i64,
                (completed_events != 1) as i64,
            ]
            .into_iter()
            .try_fold(0_i64, |total, value| {
                total.checked_add(value).ok_or_else(|| {
                    SyncStoreError::Conflict(
                        "directory reconciliation integrity count overflow".into(),
                    )
                })
            })?;
            let observed_digest = if integrity_violations == 0 {
                expected_digest
            } else {
                Sha256::digest(format!(
                    "directory-reconciliation-mismatch:{mismatched_rows}:{missing_non_deleted}:{link_mismatches}:{projection_mismatches}:{observed_import_events}:{expected_import_events}:{completed_events}"
                ))
                .into()
            };
            let report = ReconciliationReport::from_counts(
                import_id,
                expected_users,
                staged_users,
                canonical_users,
                expected_digest,
                observed_digest,
                checked_at,
            )
            .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            let report_id = Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!(
                    "directory-reconciliation/{}/{}",
                    report.import_id,
                    report.checked_at.timestamp_micros()
                )
                .as_bytes(),
            );
            sqlx::query(
                "INSERT INTO erpx_identity.directory_reconciliation_runs
                    (id, import_id, expected_users, staged_users, canonical_users,
                     expected_digest, observed_digest, status, checked_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(report_id)
            .bind(report.import_id)
            .bind(i64::try_from(report.expected_users).map_err(|_| {
                SyncStoreError::InvalidRecord("expected user count is not representable".into())
            })?)
            .bind(i64::try_from(report.staged_users).map_err(|_| {
                SyncStoreError::InvalidRecord("staged user count is not representable".into())
            })?)
            .bind(i64::try_from(report.canonical_users).map_err(|_| {
                SyncStoreError::InvalidRecord("canonical user count is not representable".into())
            })?)
            .bind(Vec::from(report.expected_digest))
            .bind(Vec::from(report.observed_digest))
            .bind(if report.is_reconciled() {
                "reconciled"
            } else {
                "blocked"
            })
            .bind(report.checked_at)
            .execute(&mut *tx)
            .await?;
            let event = DirectoryOutboxEvent::for_reconciliation(&report, connector_id, scope)
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            Self::insert_outbox_event(&mut tx, &event, None).await?;
            tx.commit().await?;
            Ok(report)
        })
    }

    fn apply_change<'a>(&'a self, change: &'a DirectoryChange) -> StoreFuture<'a, bool> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await?;
            let connector_binding = sqlx::query(
                "SELECT feed_kind, enabled
                 FROM erpx_identity.directory_connectors
                 WHERE id=$1
                 FOR SHARE",
            )
            .bind(change.connector_id)
            .fetch_one(&mut *tx)
            .await?;
            let stored_feed_kind: String = connector_binding.try_get("feed_kind")?;
            if feed_cursor_storage_name(change.cursor()).is_none_or(|feed| feed != stored_feed_kind)
            {
                return Err(SyncStoreError::Conflict(
                    "native directory change cursor does not match the persisted connector feed"
                        .into(),
                ));
            }
            if !connector_binding.try_get("enabled")? {
                return Err(SyncStoreError::Conflict(
                    "directory connector is disabled".into(),
                ));
            }
            let directory_authority =
                Self::load_directory_authority(&mut tx, change.connector_id).await?;
            let canonical_immutable_link =
                ImmutableIdentityLink::new(directory_authority, change.immutable_link.value())
                    .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            // Full-import finalization and native-feed delivery share one
            // connector-wide fence.  Without it, a snapshot finalizer can
            // commit an older identity observation while a native change is
            // being applied for the same connector.
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(format!("directory-connector/{}", change.connector_id))
                .execute(&mut *tx)
                .await?;
            // There is no mutable in-memory cursor authority. Serialize
            // competing deliveries for one connector/object while the
            // source-event idempotency check and identity write commit.
            sqlx::query(
                "SELECT pg_advisory_xact_lock(
                    hashtextextended($1 || '/' || $2, 0)
                 )",
            )
            .bind(change.connector_id.to_string())
            .bind(&change.external_id)
            .execute(&mut *tx)
            .await?;
            let current = sqlx::query(
                "SELECT d.id, d.immutable_authority, d.immutable_value,
                        d.aggregate_version, d.scope_kind, d.scope_id, d.record,
                        l.source_revision AS linked_source_revision
                 FROM erpx_identity.directory_identities d
                 LEFT JOIN erpx_identity.directory_identity_links l
                   ON l.connector_id=d.connector_id
                  AND l.external_object_id=d.external_id
                 WHERE d.connector_id=$1 AND d.external_id=$2
                 FOR UPDATE OF d",
            )
            .bind(change.connector_id)
            .bind(&change.external_id)
            .fetch_optional(&mut *tx)
            .await?;
            let source_key = change
                .idempotency_key()
                .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            if let Some(existing) = sqlx::query(
                "SELECT payload_json
                 FROM erpx_identity.directory_outbox
                 WHERE connector_id=$1 AND idempotency_key=$2
                 FOR UPDATE",
            )
            .bind(change.connector_id)
            .bind(&source_key)
            .fetch_optional(&mut *tx)
            .await?
            {
                let payload: Value = existing.try_get("payload_json")?;
                let stored =
                    serde_json::from_value::<DirectoryChange>(payload).map_err(|error| {
                        SyncStoreError::Serialization(format!(
                            "stored native directory change is invalid: {error}"
                        ))
                    })?;
                if stored
                    .source_identity()
                    .map_err(|error| SyncStoreError::Serialization(error.to_string()))?
                    != change
                        .source_identity()
                        .map_err(|error| SyncStoreError::Serialization(error.to_string()))?
                {
                    return Err(SyncStoreError::Conflict(
                        "native source idempotency key is bound to different change content".into(),
                    ));
                }
                if current.is_none() {
                    return Err(SyncStoreError::Conflict(
                        "native source event exists without its canonical identity".into(),
                    ));
                }
                tx.commit().await?;
                return Ok(false);
            }
            let mut existing_record: Option<Value> = None;
            let (identity_id, version, scope): (Uuid, i64, ConnectorScope) =
                if let Some(row) = current {
                    let existing_authority: String = row.try_get("immutable_authority")?;
                    let existing_value: String = row.try_get("immutable_value")?;
                    if existing_authority != canonical_immutable_link.authority()
                        || existing_value != canonical_immutable_link.value()
                    {
                        return Err(SyncStoreError::Conflict(
                            "native change attempted to mutate immutable identity link".into(),
                        ));
                    }
                    let id: Uuid = row.try_get("id")?;
                    let old_version: i64 = row.try_get("aggregate_version")?;
                    let scope = scope_from_storage(
                        &row.try_get::<String, _>("scope_kind")?,
                        row.try_get("scope_id")?,
                    )?;
                    let old_source_revision: Option<i64> = row.try_get("linked_source_revision")?;
                    let candidate_source_revision = native_change_source_revision(
                        change,
                        old_version.checked_add(1).ok_or_else(|| {
                            SyncStoreError::Conflict(
                                "directory identity aggregate version overflow".into(),
                            )
                        })?,
                    )?;
                    if old_source_revision.is_some_and(|old| old >= candidate_source_revision) {
                        // A native cursor can legally deliver an older change
                        // after a reconnect. It is not a replay of the same
                        // source event, so the idempotency key alone cannot
                        // protect the identity. Ignore it before projection,
                        // role facts, revocation, or outbox publication.
                        tx.commit().await?;
                        return Ok(false);
                    }
                    existing_record = Some(row.try_get("record")?);
                    (
                        id,
                        old_version.checked_add(1).ok_or_else(|| {
                            SyncStoreError::Conflict(
                                "directory identity aggregate version overflow".into(),
                            )
                        })?,
                        scope,
                    )
                } else {
                    if change.kind != ChangeKind::Upsert {
                        // A delete/disable for an unknown identity is harmless but
                        // must not create a principal or a role revocation target.
                        tx.commit().await?;
                        return Ok(false);
                    }
                    let scope_row = sqlx::query(
                        "SELECT scope_kind, scope_id
                     FROM erpx_identity.directory_connectors
                     WHERE id=$1",
                    )
                    .bind(change.connector_id)
                    .fetch_one(&mut *tx)
                    .await?;
                    let scope = scope_from_storage(
                        &scope_row.try_get::<String, _>("scope_kind")?,
                        scope_row.try_get("scope_id")?,
                    )?;
                    (
                        Uuid::new_v5(
                            &Uuid::NAMESPACE_OID,
                            format!(
                                "directory-identity/{}/{}",
                                change.connector_id, change.external_id
                            )
                            .as_bytes(),
                        ),
                        1,
                        scope,
                    )
                };
            let state = match change.kind {
                ChangeKind::Upsert => change
                    .record
                    .as_ref()
                    .map_or("active", |record| record.state().as_str()),
                ChangeKind::Disable => "disabled",
                ChangeKind::Delete => "deleted",
            };
            let record = change
                .record
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|error| SyncStoreError::Serialization(error.to_string()))?
                .or(existing_record);
            let source_revision = native_change_source_revision(change, version)?;
            let existing_immutable_identity: Option<Uuid> = sqlx::query_scalar(
                "SELECT id
                   FROM erpx_identity.directory_identities
                  WHERE connector_id=$1 AND immutable_authority=$2 AND immutable_value=$3
                  FOR SHARE",
            )
            .bind(change.connector_id)
            .bind(canonical_immutable_link.authority())
            .bind(canonical_immutable_link.value())
            .fetch_optional(&mut *tx)
            .await?;
            if existing_immutable_identity.is_some_and(|linked| linked != identity_id) {
                return Err(SyncStoreError::Conflict(
                    "immutable directory link is already bound to another identity".into(),
                ));
            }
            let immutable_identity_key = immutable_identity_database_key(&canonical_immutable_link);
            sqlx::query(
                "INSERT INTO erpx_identity.directory_identities
                    (id, connector_id, scope_kind, scope_id, external_id,
                     immutable_authority, immutable_value, state, record,
                     aggregate_version, updated_at, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
                 ON CONFLICT (connector_id, external_id) DO UPDATE
                   SET state=EXCLUDED.state, record=EXCLUDED.record,
                       aggregate_version=EXCLUDED.aggregate_version,
                       updated_at=EXCLUDED.updated_at",
            )
            .bind(identity_id)
            .bind(change.connector_id)
            .bind(scope.storage_parts().0)
            .bind(scope.storage_parts().1)
            .bind(&change.external_id)
            .bind(canonical_immutable_link.authority())
            .bind(canonical_immutable_link.value())
            .bind(state)
            .bind(&record)
            .bind(version)
            .bind(change.observed_at)
            .execute(&mut *tx)
            .await?;
            let linked_identity: Option<Uuid> = sqlx::query_scalar(
                "SELECT identity_id
                 FROM erpx_identity.directory_identity_links
                 WHERE connector_id=$1 AND immutable_authority=$2 AND immutable_value=$3",
            )
            .bind(change.connector_id)
            .bind(canonical_immutable_link.authority())
            .bind(canonical_immutable_link.value())
            .fetch_optional(&mut *tx)
            .await?;
            if linked_identity.is_some_and(|linked| linked != identity_id) {
                return Err(SyncStoreError::Conflict(
                    "immutable directory link is already bound to another identity".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO erpx_identity.directory_identity_links
                    (identity_link, immutable_authority, immutable_value,
                     connector_id, external_object_id, identity_id, linked_at,
                     principal_ref, active, source_revision, observed_at, revoked_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT DO NOTHING",
            )
            .bind(&immutable_identity_key)
            .bind(canonical_immutable_link.authority())
            .bind(canonical_immutable_link.value())
            .bind(change.connector_id)
            .bind(&change.external_id)
            .bind(identity_id)
            .bind(change.observed_at)
            .bind(
                record
                    .as_ref()
                    .and_then(|record| record.get("login_name").and_then(Value::as_str)),
            )
            .bind(state == "active")
            .bind(source_revision)
            .bind(change.observed_at)
            .bind((state != "active").then_some(change.observed_at))
            .execute(&mut *tx)
            .await?;
            // Legacy rows may predate the canonical key. Filling a NULL key
            // for the same immutable identity is safe; a non-NULL key is
            // never replaced in place.
            sqlx::query(
                "UPDATE erpx_identity.directory_identity_links
                    SET identity_link=$1, principal_ref=$5, active=$6,
                        source_revision=$7, observed_at=$8, revoked_at=$9
                  WHERE connector_id=$2 AND external_object_id=$3
                    AND identity_id=$4
                    AND (identity_link IS NULL OR identity_link=$1)",
            )
            .bind(&immutable_identity_key)
            .bind(change.connector_id)
            .bind(&change.external_id)
            .bind(identity_id)
            .bind(
                record
                    .as_ref()
                    .and_then(|record| record.get("login_name").and_then(Value::as_str)),
            )
            .bind(state == "active")
            .bind(source_revision)
            .bind(change.observed_at)
            .bind((state != "active").then_some(change.observed_at))
            .execute(&mut *tx)
            .await?;
            Self::apply_native_projection_and_roles(
                &mut tx,
                change,
                &scope,
                &immutable_identity_key,
                &canonical_immutable_link,
                record.as_ref(),
                source_revision,
                change.observed_at,
            )
            .await?;
            let event =
                DirectoryOutboxEvent::for_change_in_scope(change, identity_id, version, scope)
                    .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
            Self::insert_outbox_event(&mut tx, &event, None).await?;
            if let Some(signal) = change.revocation_sla() {
                sqlx::query(
                    "INSERT INTO erpx_identity.directory_revocation_sla
                        (event_id, connector_id, external_id,
                         immutable_authority, immutable_value,
                         observed_at, deadline, reason, status, applied_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                             CASE WHEN CURRENT_TIMESTAMP <= $7
                                  THEN 'applied' ELSE 'breached' END,
                             CASE WHEN CURRENT_TIMESTAMP <= $7
                                  THEN CURRENT_TIMESTAMP ELSE NULL END)
                     ON CONFLICT (event_id) DO NOTHING",
                )
                .bind(event.event_id)
                .bind(signal.connector_id)
                .bind(signal.external_id)
                .bind(canonical_immutable_link.authority())
                .bind(canonical_immutable_link.value())
                .bind(signal.observed_at)
                .bind(signal.deadline)
                .bind(match signal.change_kind {
                    ChangeKind::Disable => "directory_account_disabled",
                    ChangeKind::Delete => "directory_account_deleted",
                    ChangeKind::Upsert => "invalid_revocation_kind",
                })
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
            Ok(true)
        })
    }
}

fn scope_from_storage(
    scope_kind: &str,
    scope_id: Option<Uuid>,
) -> Result<ConnectorScope, SyncStoreError> {
    match (scope_kind, scope_id) {
        ("platform", None) => Ok(ConnectorScope::Platform),
        ("holding", Some(holding_id)) if !holding_id.is_nil() => {
            Ok(ConnectorScope::Holding(holding_id))
        }
        _ => Err(SyncStoreError::InvalidRecord(
            "directory connector scope is malformed".into(),
        )),
    }
}

fn directory_scope_from_key(value: &str) -> Result<DirectoryScope, SyncStoreError> {
    if value == "platform" {
        return Ok(DirectoryScope::Platform);
    }
    let data_profile_key = value.strip_prefix("holding:").ok_or_else(|| {
        SyncStoreError::InvalidRecord("directory connector scope is malformed".into())
    })?;
    let scope = DirectoryScope::Holding {
        data_profile_key: data_profile_key.to_owned(),
    };
    scope
        .canonical_key()
        .map_err(|error| SyncStoreError::InvalidRecord(error.to_string()))?;
    Ok(scope)
}

fn rbac_scope_key(scope: &RbacConnectorScope) -> String {
    match scope {
        RbacConnectorScope::Platform => "platform".to_owned(),
        RbacConnectorScope::Holding(holding_id) => format!("holding:{holding_id}"),
        RbacConnectorScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
    }
}

fn feed_kind_storage_name(feed: FeedKind) -> &'static str {
    match feed {
        FeedKind::GenericLdapPolling => "polling",
        FeedKind::ActiveDirectoryDirSync => "ad_dirsync",
        FeedKind::Rfc4533 => "rfc4533",
        FeedKind::UcsProvisioningService => "ucs_provisioning_service",
    }
}

fn feed_vendor_storage_name(vendor: FeedVendor) -> &'static str {
    match vendor {
        FeedVendor::GenericLdap => "generic_ldap",
        FeedVendor::ActiveDirectoryDomainServices => "active_directory_domain_services",
        FeedVendor::UniventionCorporateServer => "univention_corporate_server",
    }
}

fn feed_accepts_cursor(feed: FeedKind, cursor: &FeedCursor) -> bool {
    matches!(
        (feed, cursor),
        (FeedKind::GenericLdapPolling, FeedCursor::LdapPaged(_))
            | (FeedKind::ActiveDirectoryDirSync, FeedCursor::AdDirSync(_))
            | (FeedKind::Rfc4533, FeedCursor::Rfc4533(_))
            | (FeedKind::UcsProvisioningService, FeedCursor::Ucs { .. })
    )
}

fn cursor_matches_feed(feed: FeedKind, cursor: &FeedCursor) -> bool {
    matches!(cursor, FeedCursor::Start) || feed_accepts_cursor(feed, cursor)
}

fn validate_page_feed_cursor(feed: FeedKind, page: &DirectoryPage) -> Result<(), SyncError> {
    if !cursor_matches_feed(feed, page.cursor()) {
        return Err(SyncError::InvalidPage(
            "directory page cursor does not match the configured feed".into(),
        ));
    }
    if let Some(next_cursor) = page.next_cursor()
        && !feed_accepts_cursor(feed, next_cursor)
    {
        return Err(SyncError::InvalidPage(
            "directory page continuation does not match the configured feed".into(),
        ));
    }
    Ok(())
}

fn feed_cursor_storage_name(cursor: &FeedCursor) -> Option<&'static str> {
    match cursor {
        FeedCursor::LdapPaged(_) => Some("polling"),
        FeedCursor::AdDirSync(_) => Some("ad_dirsync"),
        FeedCursor::Rfc4533(_) => Some("rfc4533"),
        FeedCursor::Ucs { .. } => Some("ucs_provisioning_service"),
        FeedCursor::Start | FeedCursor::EndOfSnapshot => None,
    }
}

fn checkpoint_parts(cursor: &FeedCursor) -> Result<(Option<u64>, Option<String>), SyncStoreError> {
    match cursor {
        FeedCursor::LdapPaged(cookie)
        | FeedCursor::AdDirSync(cookie)
        | FeedCursor::Rfc4533(cookie) => Ok((None, Some(URL_SAFE_NO_PAD.encode(cookie)))),
        FeedCursor::Ucs { token, sequence } => Ok((
            Some(*sequence),
            Some(if token.is_empty() {
                URL_SAFE_NO_PAD.encode(sequence.to_be_bytes())
            } else {
                token.clone()
            }),
        )),
        FeedCursor::Start | FeedCursor::EndOfSnapshot => Err(SyncStoreError::InvalidRecord(
            "directory checkpoint cannot persist a non-native cursor".into(),
        )),
    }
}

fn full_import_source_revision(
    record: &Value,
    aggregate_version: i64,
) -> Result<i64, SyncStoreError> {
    if aggregate_version <= 0 {
        return Err(SyncStoreError::InvalidRecord(
            "directory aggregate version is not positive".into(),
        ));
    }
    let source_revision = record
        .get("source_version")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(aggregate_version);
    Ok(source_revision)
}

fn native_change_source_revision(
    change: &DirectoryChange,
    aggregate_version: i64,
) -> Result<i64, SyncStoreError> {
    if aggregate_version <= 0 {
        return Err(SyncStoreError::InvalidRecord(
            "directory aggregate version is not positive".into(),
        ));
    }
    Ok(change
        .source_version
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(aggregate_version))
}

fn non_negative_count(value: i64) -> Result<u64, SyncStoreError> {
    u64::try_from(value)
        .map_err(|_| SyncStoreError::InvalidRecord("directory count is negative".into()))
}

fn validate_immutable_part(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), SyncError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > max_bytes || trimmed.chars().any(char::is_control) {
        return Err(SyncError::InvalidRecord(format!(
            "{field} is empty, too long, or contains control characters"
        )));
    }
    Ok(())
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_role_target_scope(value: &str) -> Result<DirectoryRoleTargetScope, SyncStoreError> {
    match value {
        "platform" => Ok(DirectoryRoleTargetScope::Platform),
        "holding" => Ok(DirectoryRoleTargetScope::Holding),
        "company" => Ok(DirectoryRoleTargetScope::Company),
        "branch" => Ok(DirectoryRoleTargetScope::Branch),
        _ => Err(SyncStoreError::InvalidRecord(
            "stored directory role target scope is invalid".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 27, 12, 0, 0)
            .single()
            .expect("fixed timestamp")
    }

    fn connector() -> DirectoryConnector {
        DirectoryConnector::new(
            Uuid::from_u128(1),
            FeedVendor::GenericLdap,
            FeedKind::GenericLdapPolling,
            ConnectorScope::Holding(Uuid::from_u128(2)),
            "ldaps://directory.example.test",
            TransportSecurity::Ldaps,
            2,
            4,
        )
        .expect("valid connector")
    }

    fn record(id: &str) -> DirectoryUserRecord {
        DirectoryUserRecord::builder(
            id,
            ImmutableIdentityLink::new("ldap:entryUUID", format!("uuid-{id}")).expect("link"),
        )
        .login_name(format!("{id}@example.test"))
        .display_name(id)
        .build()
        .expect("record")
    }

    #[test]
    fn connector_rejects_plaintext_and_unbounded_import_settings() {
        assert!(
            DirectoryConnector::new(
                Uuid::from_u128(1),
                FeedVendor::GenericLdap,
                FeedKind::GenericLdapPolling,
                ConnectorScope::Platform,
                "ldap://directory.example.test",
                TransportSecurity::Ldaps,
                2,
                1,
            )
            .is_err()
        );
        assert!(ImportConfig::new(MAX_DIRECTORY_PAGE_SIZE + 1, 1).is_err());
        assert!(ImportConfig::new(2, MAX_IN_FLIGHT_PAGES + 1).is_err());
    }

    #[test]
    fn persisted_import_config_round_trips_one_million_user_capacity() {
        let original = ImportConfig::from_persisted(5_000, 64, MAX_FULL_IMPORT_USERS)
            .expect("maximum native import tuning is valid");
        let encoded = serde_json::to_vec(&original).expect("import tuning serializes");
        let decoded: ImportConfig =
            serde_json::from_slice(&encoded).expect("import tuning decodes");
        let rebuilt = ImportConfig::from_persisted(
            decoded.page_size(),
            decoded.max_in_flight_pages(),
            decoded.max_users(),
        )
        .expect("persisted import tuning remains valid after restart");

        assert_eq!(rebuilt, original);
        assert_eq!(rebuilt.max_users(), 1_000_000);
        assert_eq!(rebuilt.page_size(), 5_000);
        assert_eq!(rebuilt.max_in_flight_pages(), 64);
    }

    #[test]
    fn persisted_import_config_rejects_out_of_bounds_values() {
        for (page_size, max_in_flight_pages, max_users) in [
            (0, 1, 1),
            (MAX_DIRECTORY_PAGE_SIZE + 1, 1, 1),
            (1, 0, 1),
            (1, MAX_IN_FLIGHT_PAGES + 1, 1),
            (1, 1, 0),
            (1, 1, MAX_FULL_IMPORT_USERS + 1),
        ] {
            assert!(
                ImportConfig::from_persisted(page_size, max_in_flight_pages, max_users).is_err(),
                "invalid persisted tuning must fail closed: page_size={page_size}, max_in_flight_pages={max_in_flight_pages}, max_users={max_users}"
            );
        }
    }

    #[test]
    fn full_import_checkpoint_is_bounded_and_terminal_only_after_end_marker() {
        let plan = ImportPlan::new(connector(), ImportConfig::new(2, 4).unwrap(), at()).unwrap();
        let checkpoint = ImportCheckpoint::initial(&plan);
        let first = DirectoryPage::new(
            FeedCursor::Start,
            vec![record("001"), record("002")],
            Some(FeedCursor::LdapPaged(vec![1])),
            false,
            Some("watermark-1".into()),
        )
        .unwrap();
        let checkpoint = checkpoint.accept_page(&first, plan.config(), at()).unwrap();
        assert_eq!(checkpoint.users_seen(), 2);
        assert_eq!(checkpoint.state(), &ImportState::Running);
        let terminal = DirectoryPage::new(
            FeedCursor::LdapPaged(vec![1]),
            vec![record("003")],
            None,
            true,
            Some("watermark-1".into()),
        )
        .unwrap();
        let checkpoint = checkpoint
            .accept_page(&terminal, plan.config(), at())
            .unwrap();
        assert_eq!(checkpoint.users_seen(), 3);
        assert_eq!(checkpoint.state(), &ImportState::Finalizing);
    }

    #[test]
    fn an_empty_non_terminal_native_page_is_valid_when_its_cursor_advances() {
        let page = DirectoryPage::new(
            FeedCursor::AdDirSync(vec![1]),
            Vec::new(),
            Some(FeedCursor::AdDirSync(vec![2])),
            false,
            Some("usn-2".into()),
        )
        .expect("control-only native pages may commit an advancing cursor");
        let plan = ImportPlan::new(
            DirectoryConnector::new(
                Uuid::from_u128(8),
                FeedVendor::ActiveDirectoryDomainServices,
                FeedKind::ActiveDirectoryDirSync,
                ConnectorScope::Platform,
                "ldaps://ad.example.test",
                TransportSecurity::Ldaps,
                2,
                1,
            )
            .expect("AD connector"),
            ImportConfig::new(2, 1).expect("import config"),
            at(),
        )
        .expect("import plan");
        let checkpoint = ImportCheckpoint::initial(&plan);
        // A resumed import starts at its persisted native cursor. The empty
        // page is validated independently above; this assertion proves the
        // checkpoint transition retains progress without inventing a user.
        let checkpoint = ImportCheckpoint {
            cursor: FeedCursor::AdDirSync(vec![1]),
            ..checkpoint
        };
        let next = checkpoint
            .accept_page(&page, plan.config(), at())
            .expect("advancing empty page is durable progress");
        assert_eq!(next.users_seen(), 0);
        assert_eq!(next.cursor(), &FeedCursor::AdDirSync(vec![2]));
        assert_eq!(next.pages_applied(), 1);
    }

    #[test]
    fn page_cursors_are_bound_to_the_connector_feed_before_persistence() {
        let page = DirectoryPage::new(
            FeedCursor::Start,
            vec![record("001")],
            Some(FeedCursor::AdDirSync(vec![1])),
            false,
            None,
        )
        .expect("page shape is otherwise valid");
        assert!(matches!(
            validate_page_feed_cursor(FeedKind::GenericLdapPolling, &page),
            Err(SyncError::InvalidPage(message)) if message.contains("configured feed")
        ));
    }

    #[test]
    fn terminal_native_full_import_cursor_is_retained_for_incremental_resume() {
        let plan = ImportPlan::new(connector(), ImportConfig::new(2, 4).unwrap(), at()).unwrap();
        let checkpoint = ImportCheckpoint::initial(&plan);
        let terminal = DirectoryPage::new(
            FeedCursor::Start,
            vec![record("001")],
            Some(FeedCursor::LdapPaged(vec![17])),
            true,
            Some("source-watermark-17".into()),
        )
        .unwrap();
        let checkpoint = checkpoint
            .accept_page(&terminal, plan.config(), at())
            .unwrap();
        assert_eq!(checkpoint.state(), &ImportState::Finalizing);
        assert_eq!(
            checkpoint.cursor(),
            &FeedCursor::LdapPaged(vec![17]),
            "a terminal refresh cookie is the only safe incremental starting point"
        );
    }

    #[test]
    fn non_empty_change_batches_require_one_committed_native_cursor() {
        let link =
            ImmutableIdentityLink::new("ldap:entryUUID", "uuid-001").expect("immutable link");
        let next = FeedCursor::LdapPaged(vec![2]);
        let change = DirectoryChange::new(
            Uuid::from_u128(1),
            "001",
            link,
            ChangeKind::Disable,
            None,
            at(),
            Some("2".into()),
            next.clone(),
        )
        .unwrap();
        assert!(
            DirectoryChangeBatch::new(
                FeedCursor::LdapPaged(vec![1]),
                vec![change.clone()],
                Some(next.clone()),
                false,
            )
            .is_ok()
        );
        assert!(matches!(
            DirectoryChangeBatch::new(
                FeedCursor::LdapPaged(vec![1]),
                vec![change],
                None,
                false,
            ),
                Err(SyncError::InvalidPage(message))
                if message.contains("committed native cursor")
        ));

        let second_connector_change = DirectoryChange::new(
            Uuid::from_u128(2),
            "002",
            ImmutableIdentityLink::new("ldap:entryUUID", "uuid-002").expect("immutable link"),
            ChangeKind::Disable,
            None,
            at(),
            Some("2".into()),
            next.clone(),
        )
        .unwrap();
        assert!(matches!(
            DirectoryChangeBatch::new(
                FeedCursor::LdapPaged(vec![1]),
                vec![
                    DirectoryChange::new(
                        Uuid::from_u128(1),
                        "001",
                        ImmutableIdentityLink::new("ldap:entryUUID", "uuid-001")
                            .expect("immutable link"),
                        ChangeKind::Disable,
                        None,
                        at(),
                        Some("1".into()),
                        next.clone(),
                    )
                    .unwrap(),
                    second_connector_change,
                ],
                Some(next),
                false,
            ),
            Err(SyncError::InvalidPage(message))
                if message.contains("cannot mix directory connectors")
        ));

        assert!(
            DirectoryChange::new(
                Uuid::from_u128(1),
                "001",
                ImmutableIdentityLink::new("ldap:entryUUID", "uuid-001").expect("immutable link"),
                ChangeKind::Disable,
                None,
                at(),
                None,
                FeedCursor::Start,
            )
            .is_err()
        );
    }

    #[test]
    fn connector_scope_rejects_nil_holding_ids() {
        assert!(
            DirectoryConnector::new(
                Uuid::from_u128(1),
                FeedVendor::GenericLdap,
                FeedKind::GenericLdapPolling,
                ConnectorScope::Holding(Uuid::nil()),
                "ldaps://directory.example.test",
                TransportSecurity::Ldaps,
                2,
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn page_replay_and_cursor_regression_fail_closed() {
        let plan = ImportPlan::new(connector(), ImportConfig::new(2, 4).unwrap(), at()).unwrap();
        let checkpoint = ImportCheckpoint::initial(&plan);
        let page = DirectoryPage::new(
            FeedCursor::Start,
            vec![record("001")],
            Some(FeedCursor::LdapPaged(vec![9])),
            false,
            None,
        )
        .unwrap();
        let checkpoint = checkpoint.accept_page(&page, plan.config(), at()).unwrap();
        assert!(matches!(
            checkpoint.accept_page(&page, plan.config(), at()),
            Err(SyncError::StaleCursor)
        ));
        let regressed = DirectoryPage::new(
            FeedCursor::LdapPaged(vec![9]),
            vec![record("000")],
            Some(FeedCursor::LdapPaged(vec![10])),
            false,
            None,
        )
        .unwrap();
        assert!(matches!(
            checkpoint.accept_page(&regressed, plan.config(), at()),
            Err(SyncError::InvalidPage(_))
        ));
    }

    #[test]
    fn non_terminal_page_cannot_replay_the_same_cursor_forever() {
        assert!(matches!(
            DirectoryPage::new(
                FeedCursor::LdapPaged(vec![9]),
                vec![record("001")],
                Some(FeedCursor::LdapPaged(vec![9])),
                false,
                None,
            ),
            Err(SyncError::InvalidPage(message)) if message.contains("advance")
        ));
        assert!(matches!(
            DirectoryChangeBatch::new(
                FeedCursor::AdDirSync(vec![4]),
                Vec::new(),
                Some(FeedCursor::AdDirSync(vec![4])),
                true,
            ),
            Err(SyncError::InvalidPage(message)) if message.contains("advance")
        ));
        assert!(matches!(
            DirectoryPage::new(
                FeedCursor::Start,
                vec![record("001")],
                Some(FeedCursor::LdapPaged(Vec::new())),
                false,
                None,
            ),
            Err(SyncError::InvalidPage(message)) if message.contains("empty")
        ));
        assert!(matches!(
            DirectoryChangeBatch::new(
                FeedCursor::Start,
                Vec::new(),
                Some(FeedCursor::Rfc4533(Vec::new())),
                true,
            ),
            Err(SyncError::InvalidPage(message)) if message.contains("empty")
        ));
    }

    #[test]
    fn immutable_link_cannot_be_replaced_and_fingerprint_ignores_mutable_names() {
        let link = ImmutableIdentityLink::new("ad:objectGUID", "ABC").unwrap();
        let same = ImmutableIdentityLink::new("ad:objectGUID", "ABC").unwrap();
        assert_eq!(link.fingerprint(), same.fingerprint());
        let changed = DirectoryUserRecord::builder("user-a", link.clone())
            .login_name("renamed@example.test")
            .build()
            .unwrap();
        assert_eq!(changed.immutable_link(), &link);
        assert!(
            DirectoryChange::new(
                Uuid::from_u128(1),
                "user-a",
                ImmutableIdentityLink::new("ad:objectGUID", "different").unwrap(),
                ChangeKind::Upsert,
                Some(changed),
                at(),
                None,
                FeedCursor::AdDirSync(vec![1]),
            )
            .is_err()
        );
    }

    #[test]
    fn disable_and_delete_emit_five_minute_revocation_deadlines() {
        let link = ImmutableIdentityLink::new("ucs:entryUUID", "u-1").unwrap();
        let change = DirectoryChange::new(
            Uuid::from_u128(1),
            "external-1",
            link,
            ChangeKind::Disable,
            None,
            at(),
            Some("42".into()),
            FeedCursor::Ucs {
                token: "token".into(),
                sequence: 42,
            },
        )
        .unwrap();
        let signal = change.revocation_sla().expect("revocation signal");
        assert_eq!(signal.deadline(), at() + Duration::minutes(5));
        assert!(!signal.is_late_at(at() + Duration::minutes(5)));
        assert!(signal.is_late_at(at() + Duration::minutes(5) + Duration::nanoseconds(1)));
    }

    #[test]
    fn scope_containment_and_high_risk_mfa_are_fail_closed() {
        let context = DirectoryAuthorizationContext::new(
            "operator-1",
            ConnectorScope::Holding(Uuid::from_u128(2)),
            None,
            false,
            false,
        )
        .unwrap();
        assert!(matches!(
            context.authorize(
                DirectoryAction::StartFullImport,
                &ConnectorScope::Holding(Uuid::from_u128(2)),
                at()
            ),
            Err(SyncError::MfaRequired)
        ));
        assert!(matches!(
            context.authorize(
                DirectoryAction::ReadConnector,
                &ConnectorScope::Holding(Uuid::from_u128(3)),
                at()
            ),
            Err(SyncError::UnauthorizedScope)
        ));
    }

    #[test]
    fn change_event_id_and_page_idempotency_key_are_stable() {
        let plan = ImportPlan::with_id(
            Uuid::from_u128(9),
            connector(),
            ImportConfig::new(2, 4).unwrap(),
            at(),
        )
        .unwrap();
        let page =
            DirectoryPage::new(FeedCursor::Start, vec![record("001")], None, true, None).unwrap();
        assert_eq!(
            page.idempotency_key(plan.import_id()).unwrap(),
            page.idempotency_key(plan.import_id()).unwrap()
        );
        let change = DirectoryChange::new(
            Uuid::from_u128(1),
            "001",
            ImmutableIdentityLink::new("ldap:entryUUID", "uuid-001").unwrap(),
            ChangeKind::Upsert,
            Some(record("001")),
            at(),
            None,
            FeedCursor::LdapPaged(vec![1]),
        )
        .unwrap();
        let first = DirectoryOutboxEvent::for_change(&change, Uuid::from_u128(3), 1).unwrap();
        let second = DirectoryOutboxEvent::for_change(&change, Uuid::from_u128(3), 1).unwrap();
        assert_eq!(first.event_id, second.event_id);
        assert_eq!(first.idempotency_key, second.idempotency_key);
    }

    #[test]
    fn native_change_idempotency_is_bound_to_cursor_and_ignores_replay_time() {
        let link = ImmutableIdentityLink::new("ad:objectGUID", "guid-1").unwrap();
        let first = DirectoryChange::new(
            Uuid::from_u128(1),
            "user-1",
            link.clone(),
            ChangeKind::Disable,
            None,
            at(),
            Some("usn-42".into()),
            FeedCursor::AdDirSync(vec![42]),
        )
        .unwrap();
        let replay = DirectoryChange::new(
            Uuid::from_u128(1),
            "user-1",
            link.clone(),
            ChangeKind::Disable,
            None,
            at() + Duration::minutes(3),
            Some("usn-42".into()),
            FeedCursor::AdDirSync(vec![42]),
        )
        .unwrap();
        assert_eq!(
            first.idempotency_key().unwrap(),
            replay.idempotency_key().unwrap()
        );
        assert_eq!(
            DirectoryOutboxEvent::for_change(&first, Uuid::from_u128(3), 1)
                .unwrap()
                .event_id,
            DirectoryOutboxEvent::for_change(&replay, Uuid::from_u128(3), 2)
                .unwrap()
                .event_id
        );

        let next_cursor = DirectoryChange::new(
            Uuid::from_u128(1),
            "user-1",
            link,
            ChangeKind::Disable,
            None,
            at() + Duration::minutes(3),
            Some("usn-43".into()),
            FeedCursor::AdDirSync(vec![43]),
        )
        .unwrap();
        assert_ne!(
            first.idempotency_key().unwrap(),
            next_cursor.idempotency_key().unwrap()
        );
    }

    #[test]
    fn checkpoint_property_never_accepts_more_than_configured_users() {
        let mut config = ImportConfig::new(1, 1).unwrap();
        config.max_users = 3;
        let plan = ImportPlan::new(connector(), config, at()).unwrap();
        let mut checkpoint = ImportCheckpoint::initial(&plan);
        for (index, next_cursor) in [("001", vec![1]), ("002", vec![2]), ("003", vec![3])] {
            let page = DirectoryPage::new(
                checkpoint.cursor().clone(),
                vec![record(index)],
                Some(FeedCursor::LdapPaged(next_cursor)),
                false,
                None,
            )
            .unwrap();
            checkpoint = checkpoint.accept_page(&page, plan.config(), at()).unwrap();
        }
        let overflow = DirectoryPage::new(
            checkpoint.cursor().clone(),
            vec![record("004")],
            None,
            true,
            None,
        )
        .unwrap();
        assert!(matches!(
            checkpoint.accept_page(&overflow, plan.config(), at()),
            Err(SyncError::UserLimitExceeded)
        ));
    }
}
