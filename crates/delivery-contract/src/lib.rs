//! Target-neutral Console delivery contracts.
//!
//! Web adapters expose these operations as Dioxus Server Functions and derive
//! authority from the HttpOnly session. Native adapters construct the trusted
//! contexts after validating their credential and call the gRPC catalog
//! directly. UI code never owns SQLx, generated protobuf, or an arbitrary JSON
//! RPC envelope.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    str::FromStr,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DELIVERY_CONTRACT_VERSION: u16 = 2;
pub const SERVICE_ID_FIRST_: u16 = 201;
pub const SERVICE_ID_LAST_: u16 = 265;
pub const SERVICE_COUNT_: usize = 65;
pub const SESSION_CACHE_MAX_ENTRIES: usize = 64;
pub const SESSION_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PAGE_SIZE_: u16 = 200;

/// Canonical UI workspaces. Commerce and Talent intentionally remain Web-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Workspace {
    Operations,
    Finance,
    Sales,
    Procurement,
    Inventory,
    Asset,
    Maintenance,
    Quality,
    Marketing,
    People,
    Pos,
    Legal,
    Manufacturing,
    Planning,
    Project,
    Commerce,
    Talent,
    Learning,
    Benefits,
    FieldService,
    Ai,
}

impl Workspace {
    pub const ALL: [Self; 21] = [
        Self::Operations,
        Self::Finance,
        Self::Sales,
        Self::Procurement,
        Self::Inventory,
        Self::Asset,
        Self::Maintenance,
        Self::Quality,
        Self::Marketing,
        Self::People,
        Self::Pos,
        Self::Legal,
        Self::Manufacturing,
        Self::Planning,
        Self::Project,
        Self::Commerce,
        Self::Talent,
        Self::Learning,
        Self::Benefits,
        Self::FieldService,
        Self::Ai,
    ];

    pub const NATIVE: [Self; 19] = [
        Self::Operations,
        Self::Finance,
        Self::Sales,
        Self::Procurement,
        Self::Inventory,
        Self::Asset,
        Self::Maintenance,
        Self::Quality,
        Self::Marketing,
        Self::People,
        Self::Pos,
        Self::Legal,
        Self::Manufacturing,
        Self::Planning,
        Self::Project,
        Self::Learning,
        Self::Benefits,
        Self::FieldService,
        Self::Ai,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Operations => "operations",
            Self::Finance => "finance",
            Self::Sales => "sales",
            Self::Procurement => "procurement",
            Self::Inventory => "inventory",
            Self::Asset => "asset",
            Self::Maintenance => "maintenance",
            Self::Quality => "quality",
            Self::Marketing => "marketing",
            Self::People => "people",
            Self::Pos => "pos",
            Self::Legal => "legal",
            Self::Manufacturing => "manufacturing",
            Self::Planning => "planning",
            Self::Project => "project",
            Self::Commerce => "commerce",
            Self::Talent => "talent",
            Self::Learning => "learning",
            Self::Benefits => "benefits",
            Self::FieldService => "field-service",
            Self::Ai => "ai",
        }
    }

    pub const fn is_web_only(self) -> bool {
        matches!(self, Self::Commerce | Self::Talent)
    }

    pub const fn available_on(self, channel: ClientChannel) -> bool {
        matches!(channel, ClientChannel::Web) || !self.is_web_only()
    }
}

impl fmt::Display for Workspace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.slug())
    }
}

impl FromStr for Workspace {
    type Err = InvalidWorkspace;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|workspace| workspace.slug() == value)
            .ok_or(InvalidWorkspace)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidWorkspace;

impl fmt::Display for InvalidWorkspace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("workspace is not canonical")
    }
}

impl std::error::Error for InvalidWorkspace {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientChannel {
    Web,
    MacOs,
    Ios,
    Android,
}

/// A non-empty, non-nil UUID rendered only in lowercase hyphenated form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RouteUuid(Uuid);

impl RouteUuid {
    pub fn try_new(value: Uuid) -> Result<Self, InvalidRouteUuid> {
        (!value.is_nil())
            .then_some(Self(value))
            .ok_or(InvalidRouteUuid)
    }

    pub const fn get(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for RouteUuid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.hyphenated())
    }
}

impl FromStr for RouteUuid {
    type Err = InvalidRouteUuid;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = Uuid::parse_str(value).map_err(|_| InvalidRouteUuid)?;
        if parsed.is_nil() || parsed.hyphenated().to_string() != value {
            return Err(InvalidRouteUuid);
        }
        Ok(Self(parsed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidRouteUuid;

impl fmt::Display for InvalidRouteUuid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("route UUID must be non-nil lowercase hyphenated UUID")
    }
}

impl std::error::Error for InvalidRouteUuid {}

/// Canonical resource segment: lowercase ASCII words separated by one dash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceName(String);

impl ResourceName {
    pub fn try_new(value: impl Into<String>) -> Result<Self, InvalidResourceName> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 64
            && !value.starts_with('-')
            && !value.ends_with('-')
            && !value.contains("--")
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        valid.then_some(Self(value)).ok_or(InvalidResourceName)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ResourceName {
    fn default() -> Self {
        Self("overview".to_owned())
    }
}

impl fmt::Display for ResourceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ResourceName {
    type Err = InvalidResourceName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidResourceName;

impl fmt::Display for InvalidResourceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("resource name is not canonical")
    }
}

impl std::error::Error for InvalidResourceName {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedAuthority {
    actor_ref: String,
    data_profile_key: String,
    role_ref: String,
}

impl ValidatedAuthority {
    /// Called only by a trusted session or native credential adapter after
    /// authentication and holding/role authorization have succeeded.
    pub fn from_trusted_adapter(
        actor_ref: impl Into<String>,
        data_profile_key: impl Into<String>,
        role_ref: impl Into<String>,
    ) -> Result<Self, DeliveryError> {
        let value = Self {
            actor_ref: actor_ref.into(),
            data_profile_key: data_profile_key.into(),
            role_ref: role_ref.into(),
        };
        if [&value.actor_ref, &value.data_profile_key, &value.role_ref]
            .into_iter()
            .any(|part| !valid_reference(part))
        {
            return Err(DeliveryError::validation("invalid trusted authority"));
        }
        Ok(value)
    }

    pub fn actor_ref(&self) -> &str {
        &self.actor_ref
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    pub fn role_ref(&self) -> &str {
        &self.role_ref
    }

    pub fn cache_scope(&self) -> SessionScope {
        SessionScope {
            actor_ref: self.actor_ref.clone(),
            data_profile_key: self.data_profile_key.clone(),
            role_ref: self.role_ref.clone(),
        }
    }
}

fn valid_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.chars().any(char::is_control)
        && !value.chars().any(char::is_whitespace)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandContext {
    authority: ValidatedAuthority,
    request_id: Uuid,
    idempotency_key: String,
    expected_version: Option<u64>,
    locale: String,
}

impl CommandContext {
    pub fn from_trusted_adapter(
        authority: ValidatedAuthority,
        request_id: Uuid,
        idempotency_key: impl Into<String>,
        expected_version: Option<u64>,
        locale: impl Into<String>,
    ) -> Result<Self, DeliveryError> {
        let idempotency_key = idempotency_key.into();
        let locale = locale.into();
        if request_id.is_nil()
            || idempotency_key.is_empty()
            || idempotency_key.len() > 128
            || !valid_locale(&locale)
        {
            return Err(DeliveryError::validation("invalid command context"));
        }
        Ok(Self {
            authority,
            request_id,
            idempotency_key,
            expected_version,
            locale,
        })
    }

    pub const fn authority(&self) -> &ValidatedAuthority {
        &self.authority
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryContext {
    authority: ValidatedAuthority,
    request_id: Uuid,
    cursor: Option<String>,
    limit: u16,
    locale: String,
}

impl QueryContext {
    pub fn from_trusted_adapter(
        authority: ValidatedAuthority,
        request_id: Uuid,
        cursor: Option<String>,
        limit: u16,
        locale: impl Into<String>,
    ) -> Result<Self, DeliveryError> {
        let locale = locale.into();
        if request_id.is_nil()
            || limit == 0
            || limit > MAX_PAGE_SIZE_
            || cursor.as_ref().is_some_and(|value| value.len() > 512)
            || !valid_locale(&locale)
        {
            return Err(DeliveryError::validation("invalid query context"));
        }
        Ok(Self {
            authority,
            request_id,
            cursor,
            limit,
            locale,
        })
    }

    pub const fn authority(&self) -> &ValidatedAuthority {
        &self.authority
    }
}

fn valid_locale(value: &str) -> bool {
    matches!(value, "en" | "th" | "en-US" | "th-TH")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReceipt {
    pub command_id: Uuid,
    pub aggregate_id: RouteUuid,
    pub aggregate_version: u64,
    pub invalidation_tags: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryErrorMeta {
    pub correlation_id: Uuid,
    pub retry_after_ms: Option<u64>,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "meta", rename_all = "snake_case")]
pub enum DeliveryError {
    Unauthenticated(DeliveryErrorMeta),
    Forbidden(DeliveryErrorMeta),
    NotFound(DeliveryErrorMeta),
    Conflict(DeliveryErrorMeta),
    Validation(DeliveryErrorMeta),
    RateLimited(DeliveryErrorMeta),
    Unavailable(DeliveryErrorMeta),
    Internal(DeliveryErrorMeta),
}

impl DeliveryError {
    pub fn validation(_safe_reason: &'static str) -> Self {
        Self::Validation(DeliveryErrorMeta {
            correlation_id: Uuid::new_v4(),
            retry_after_ms: None,
            retryable: false,
        })
    }

    pub fn unavailable(retry_after_ms: Option<u64>) -> Self {
        Self::Unavailable(DeliveryErrorMeta {
            correlation_id: Uuid::new_v4(),
            retry_after_ms,
            retryable: true,
        })
    }

    pub const fn meta(&self) -> &DeliveryErrorMeta {
        match self {
            Self::Unauthenticated(meta)
            | Self::Forbidden(meta)
            | Self::NotFound(meta)
            | Self::Conflict(meta)
            | Self::Validation(meta)
            | Self::RateLimited(meta)
            | Self::Unavailable(meta)
            | Self::Internal(meta) => meta,
        }
    }
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "delivery error ({})", self.meta().correlation_id)
    }
}

impl std::error::Error for DeliveryError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapRequest {
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBootstrap {
    pub authenticated: bool,
    pub actor_ref: Option<String>,
    pub data_profile_key: Option<String>,
    pub role_ref: Option<String>,
    pub locale: String,
    pub channel: ClientChannel,
    pub catalog_version: u16,
}

impl SessionBootstrap {
    pub fn cache_scope(&self) -> Option<SessionScope> {
        Some(SessionScope {
            actor_ref: self.actor_ref.clone()?,
            data_profile_key: self.data_profile_key.clone()?,
            role_ref: self.role_ref.clone()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteQuery {
    Work,
    Processes,
    Insights,
    Modules,
    PlatformHoldings,
    PlatformHolding {
        holding_id: RouteUuid,
    },
    Workspace {
        workspace: Workspace,
        resource: Option<ResourceName>,
    },
    Record {
        workspace: Workspace,
        resource: ResourceName,
        id: RouteUuid,
    },
    Reference {
        id: RouteUuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRequest {
    pub route: RouteQuery,
    pub cursor: Option<String>,
    pub limit: u16,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub id: RouteUuid,
    pub title: String,
    pub status: String,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteData {
    pub canonical_path: String,
    pub title: String,
    pub description: String,
    pub page: Page<ResourceSummary>,
    pub freshness_tag: String,
    pub invalidation_tags: BTreeSet<String>,
    pub public_validator: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordTransition {
    Submit,
    Approve,
    Reject,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOperation {
    CreateRecord {
        workspace: Workspace,
        resource: ResourceName,
        title: String,
    },
    UpdateTitle {
        workspace: Workspace,
        resource: ResourceName,
        id: RouteUuid,
        title: String,
    },
    TransitionRecord {
        workspace: Workspace,
        resource: ResourceName,
        id: RouteUuid,
        transition: RecordTransition,
    },
    ArchiveRecord {
        workspace: Workspace,
        resource: ResourceName,
        id: RouteUuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRequest {
    pub operation: CommandOperation,
    pub idempotency_key: String,
    pub expected_version: Option<u64>,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticationRequest {
    pub account: String,
    pub proof: String,
    pub return_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicMarketingFormRequest {
    pub form_id: RouteUuid,
    pub name: String,
    pub email: String,
    pub message: String,
    pub consent: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicMarketingPreviewRequest {
    pub preview_id: RouteUuid,
    pub access_code: String,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationSubscription {
    pub after_cursor: Option<String>,
    pub workspace: Option<Workspace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationEvent {
    pub cursor: String,
    pub topic: String,
    pub summary: String,
}

#[async_trait(?Send)]
pub trait ConsoleGateway: Send + Sync {
    async fn bootstrap(&self, request: BootstrapRequest)
    -> Result<SessionBootstrap, DeliveryError>;

    async fn query(&self, request: QueryRequest) -> Result<RouteData, DeliveryError>;

    async fn command(&self, request: CommandRequest) -> Result<CommandReceipt, DeliveryError>;

    async fn authenticate(
        &self,
        request: AuthenticationRequest,
    ) -> Result<SessionBootstrap, DeliveryError>;

    async fn submit_public_marketing_form(
        &self,
        request: PublicMarketingFormRequest,
    ) -> Result<CommandReceipt, DeliveryError>;

    async fn public_marketing_preview(
        &self,
        request: PublicMarketingPreviewRequest,
    ) -> Result<RouteData, DeliveryError>;

    async fn collaboration_stream(
        &self,
        request: CollaborationSubscription,
    ) -> Result<Vec<CollaborationEvent>, DeliveryError>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionScope {
    pub actor_ref: String,
    pub data_profile_key: String,
    pub role_ref: String,
}

#[derive(Debug, Clone)]
struct CacheEntry<V> {
    value: V,
    stored_at: Instant,
    fresh_for: Duration,
    byte_size: usize,
    tags: BTreeSet<String>,
}

/// Session-scoped LRU cache with both entry and byte ceilings.
#[derive(Debug)]
pub struct SessionCache<K, V> {
    scope: Option<SessionScope>,
    entries: BTreeMap<K, CacheEntry<V>>,
    lru: VecDeque<K>,
    total_bytes: usize,
}

impl<K, V> Default for SessionCache<K, V> {
    fn default() -> Self {
        Self {
            scope: None,
            entries: BTreeMap::new(),
            lru: VecDeque::new(),
            total_bytes: 0,
        }
    }
}

impl<K, V> SessionCache<K, V>
where
    K: Clone + Ord,
    V: Clone,
{
    pub fn set_scope(&mut self, next: Option<SessionScope>) {
        if self.scope != next {
            self.clear();
            self.scope = next;
        }
    }

    pub fn insert(
        &mut self,
        key: K,
        value: V,
        byte_size: usize,
        fresh_for: Duration,
        tags: impl IntoIterator<Item = String>,
    ) {
        if byte_size > SESSION_CACHE_MAX_BYTES {
            return;
        }
        self.remove(&key);
        self.total_bytes = self.total_bytes.saturating_add(byte_size);
        self.lru.push_back(key.clone());
        self.entries.insert(
            key,
            CacheEntry {
                value,
                stored_at: Instant::now(),
                fresh_for,
                byte_size,
                tags: tags.into_iter().collect(),
            },
        );
        while self.entries.len() > SESSION_CACHE_MAX_ENTRIES
            || self.total_bytes > SESSION_CACHE_MAX_BYTES
        {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.total_bytes = self.total_bytes.saturating_sub(entry.byte_size);
            }
        }
    }

    pub fn get_fresh(&mut self, key: &K) -> Option<V> {
        let entry = self.entries.get(key)?;
        if entry.stored_at.elapsed() > entry.fresh_for {
            self.remove(key);
            return None;
        }
        let value = entry.value.clone();
        self.lru.retain(|candidate| candidate != key);
        self.lru.push_back(key.clone());
        Some(value)
    }

    pub fn invalidate_tags<'a>(&mut self, tags: impl IntoIterator<Item = &'a str>) {
        let tags = tags.into_iter().collect::<BTreeSet<_>>();
        let keys = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                entry
                    .tags
                    .iter()
                    .any(|candidate| tags.contains(candidate.as_str()))
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.remove(&key);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
        self.total_bytes = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn remove(&mut self, key: &K) {
        self.lru.retain(|candidate| candidate != key);
        if let Some(entry) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.byte_size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(role: &str) -> SessionScope {
        SessionScope {
            actor_ref: "actor:one".to_owned(),
            data_profile_key: "holding:one".to_owned(),
            role_ref: role.to_owned(),
        }
    }

    #[test]
    fn canonical_workspace_sets_cover_web_and_native_graphs() {
        assert_eq!(Workspace::ALL.len(), 21);
        assert_eq!(Workspace::NATIVE.len(), 19);
        assert!(Workspace::Commerce.is_web_only());
        assert!(Workspace::Talent.is_web_only());
        for workspace in Workspace::ALL {
            assert_eq!(workspace.slug().parse(), Ok(workspace));
            assert_eq!(workspace.to_string(), workspace.slug());
        }
    }

    #[test]
    fn route_parameters_fail_closed() {
        let id = Uuid::new_v4();
        assert_eq!(
            id.hyphenated()
                .to_string()
                .parse::<RouteUuid>()
                .unwrap()
                .get(),
            id
        );
        assert!(id.simple().to_string().parse::<RouteUuid>().is_err());
        assert!(Uuid::nil().to_string().parse::<RouteUuid>().is_err());
        assert!("purchase-orders".parse::<ResourceName>().is_ok());
        assert!("PurchaseOrders".parse::<ResourceName>().is_err());
        assert!("purchase--orders".parse::<ResourceName>().is_err());
    }

    #[test]
    fn cache_enforces_count_bytes_freshness_tags_and_scope() {
        let mut cache = SessionCache::<u16, String>::default();
        cache.set_scope(Some(scope("role:one")));
        for key in 0..70 {
            cache.insert(
                key,
                key.to_string(),
                16,
                Duration::from_secs(60),
                [format!("workspace:{}", key % 2)],
            );
        }
        assert_eq!(cache.len(), SESSION_CACHE_MAX_ENTRIES);
        assert!(cache.get_fresh(&0).is_none());
        cache.invalidate_tags(["workspace:1"]);
        assert!(cache.len() < SESSION_CACHE_MAX_ENTRIES);
        cache.set_scope(Some(scope("role:two")));
        assert!(cache.is_empty());

        cache.insert(
            1,
            "large".to_owned(),
            SESSION_CACHE_MAX_BYTES + 1,
            Duration::from_secs(1),
            [],
        );
        assert!(cache.is_empty());

        cache.insert(2, "stale".to_owned(), 5, Duration::ZERO, []);
        std::thread::sleep(Duration::from_millis(1));
        assert!(cache.get_fresh(&2).is_none());
        assert_eq!(cache.total_bytes(), 0);
    }

    #[test]
    fn service_range_is_exactly_sixty_five_service_ids() {
        assert_eq!(
            SERVICE_ID_LAST_ - SERVICE_ID_FIRST_ + 1,
            SERVICE_COUNT_ as u16
        );
    }
}
