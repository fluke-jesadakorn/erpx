//! Target-neutral Console delivery v2 contracts.
//!
//! Web adapters expose these operations as Dioxus Server Functions and derive
//! authority from the HttpOnly session. Native adapters construct the trusted
//! contexts after validating their credential and call the gRPC v2 catalog
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
pub const SERVICE_ID_FIRST_V2: u16 = 201;
pub const SERVICE_ID_LAST_V2: u16 = 265;
pub const SERVICE_COUNT_V2: usize = 65;
pub const SESSION_CACHE_MAX_ENTRIES: usize = 64;
pub const SESSION_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PAGE_SIZE_V2: u16 = 200;

/// Canonical UI workspaces. Commerce and Talent intentionally remain Web-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceV2 {
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

impl WorkspaceV2 {
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

    pub const fn available_on(self, channel: ClientChannelV2) -> bool {
        matches!(channel, ClientChannelV2::Web) || !self.is_web_only()
    }
}

impl fmt::Display for WorkspaceV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.slug())
    }
}

impl FromStr for WorkspaceV2 {
    type Err = InvalidWorkspaceV2;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|workspace| workspace.slug() == value)
            .ok_or(InvalidWorkspaceV2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidWorkspaceV2;

impl fmt::Display for InvalidWorkspaceV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("workspace is not canonical")
    }
}

impl std::error::Error for InvalidWorkspaceV2 {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientChannelV2 {
    Web,
    MacOs,
    Ios,
    Android,
}

/// A non-empty, non-nil UUID rendered only in lowercase hyphenated form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RouteUuidV2(Uuid);

impl RouteUuidV2 {
    pub fn try_new(value: Uuid) -> Result<Self, InvalidRouteUuidV2> {
        (!value.is_nil())
            .then_some(Self(value))
            .ok_or(InvalidRouteUuidV2)
    }

    pub const fn get(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for RouteUuidV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.hyphenated())
    }
}

impl FromStr for RouteUuidV2 {
    type Err = InvalidRouteUuidV2;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = Uuid::parse_str(value).map_err(|_| InvalidRouteUuidV2)?;
        if parsed.is_nil() || parsed.hyphenated().to_string() != value {
            return Err(InvalidRouteUuidV2);
        }
        Ok(Self(parsed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidRouteUuidV2;

impl fmt::Display for InvalidRouteUuidV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("route UUID must be non-nil lowercase hyphenated UUID")
    }
}

impl std::error::Error for InvalidRouteUuidV2 {}

/// Canonical resource segment: lowercase ASCII words separated by one dash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceNameV2(String);

impl ResourceNameV2 {
    pub fn try_new(value: impl Into<String>) -> Result<Self, InvalidResourceNameV2> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 64
            && !value.starts_with('-')
            && !value.ends_with('-')
            && !value.contains("--")
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        valid.then_some(Self(value)).ok_or(InvalidResourceNameV2)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ResourceNameV2 {
    fn default() -> Self {
        Self("overview".to_owned())
    }
}

impl fmt::Display for ResourceNameV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ResourceNameV2 {
    type Err = InvalidResourceNameV2;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidResourceNameV2;

impl fmt::Display for InvalidResourceNameV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("resource name is not canonical")
    }
}

impl std::error::Error for InvalidResourceNameV2 {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedAuthorityV2 {
    actor_ref: String,
    tenant_ref: String,
    role_ref: String,
}

impl ValidatedAuthorityV2 {
    /// Called only by a trusted session or native credential adapter after
    /// authentication and tenant/role authorization have succeeded.
    pub fn from_trusted_adapter(
        actor_ref: impl Into<String>,
        tenant_ref: impl Into<String>,
        role_ref: impl Into<String>,
    ) -> Result<Self, DeliveryErrorV2> {
        let value = Self {
            actor_ref: actor_ref.into(),
            tenant_ref: tenant_ref.into(),
            role_ref: role_ref.into(),
        };
        if [&value.actor_ref, &value.tenant_ref, &value.role_ref]
            .into_iter()
            .any(|part| !valid_reference(part))
        {
            return Err(DeliveryErrorV2::validation("invalid trusted authority"));
        }
        Ok(value)
    }

    pub fn actor_ref(&self) -> &str {
        &self.actor_ref
    }

    pub fn tenant_ref(&self) -> &str {
        &self.tenant_ref
    }

    pub fn role_ref(&self) -> &str {
        &self.role_ref
    }

    pub fn cache_scope(&self) -> SessionScopeV2 {
        SessionScopeV2 {
            actor_ref: self.actor_ref.clone(),
            tenant_ref: self.tenant_ref.clone(),
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
pub struct CommandContextV2 {
    authority: ValidatedAuthorityV2,
    request_id: Uuid,
    idempotency_key: String,
    expected_version: Option<u64>,
    locale: String,
}

impl CommandContextV2 {
    pub fn from_trusted_adapter(
        authority: ValidatedAuthorityV2,
        request_id: Uuid,
        idempotency_key: impl Into<String>,
        expected_version: Option<u64>,
        locale: impl Into<String>,
    ) -> Result<Self, DeliveryErrorV2> {
        let idempotency_key = idempotency_key.into();
        let locale = locale.into();
        if request_id.is_nil()
            || idempotency_key.is_empty()
            || idempotency_key.len() > 128
            || !valid_locale(&locale)
        {
            return Err(DeliveryErrorV2::validation("invalid command context"));
        }
        Ok(Self {
            authority,
            request_id,
            idempotency_key,
            expected_version,
            locale,
        })
    }

    pub const fn authority(&self) -> &ValidatedAuthorityV2 {
        &self.authority
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryContextV2 {
    authority: ValidatedAuthorityV2,
    request_id: Uuid,
    cursor: Option<String>,
    limit: u16,
    locale: String,
}

impl QueryContextV2 {
    pub fn from_trusted_adapter(
        authority: ValidatedAuthorityV2,
        request_id: Uuid,
        cursor: Option<String>,
        limit: u16,
        locale: impl Into<String>,
    ) -> Result<Self, DeliveryErrorV2> {
        let locale = locale.into();
        if request_id.is_nil()
            || limit == 0
            || limit > MAX_PAGE_SIZE_V2
            || cursor.as_ref().is_some_and(|value| value.len() > 512)
            || !valid_locale(&locale)
        {
            return Err(DeliveryErrorV2::validation("invalid query context"));
        }
        Ok(Self {
            authority,
            request_id,
            cursor,
            limit,
            locale,
        })
    }

    pub const fn authority(&self) -> &ValidatedAuthorityV2 {
        &self.authority
    }
}

fn valid_locale(value: &str) -> bool {
    matches!(value, "en" | "th" | "en-US" | "th-TH")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageV2<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReceiptV2 {
    pub command_id: Uuid,
    pub aggregate_id: RouteUuidV2,
    pub aggregate_version: u64,
    pub invalidation_tags: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryErrorMetaV2 {
    pub correlation_id: Uuid,
    pub retry_after_ms: Option<u64>,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "meta", rename_all = "snake_case")]
pub enum DeliveryErrorV2 {
    Unauthenticated(DeliveryErrorMetaV2),
    Forbidden(DeliveryErrorMetaV2),
    NotFound(DeliveryErrorMetaV2),
    Conflict(DeliveryErrorMetaV2),
    Validation(DeliveryErrorMetaV2),
    RateLimited(DeliveryErrorMetaV2),
    Unavailable(DeliveryErrorMetaV2),
    Internal(DeliveryErrorMetaV2),
}

impl DeliveryErrorV2 {
    pub fn validation(_safe_reason: &'static str) -> Self {
        Self::Validation(DeliveryErrorMetaV2 {
            correlation_id: Uuid::new_v4(),
            retry_after_ms: None,
            retryable: false,
        })
    }

    pub fn unavailable(retry_after_ms: Option<u64>) -> Self {
        Self::Unavailable(DeliveryErrorMetaV2 {
            correlation_id: Uuid::new_v4(),
            retry_after_ms,
            retryable: true,
        })
    }

    pub const fn meta(&self) -> &DeliveryErrorMetaV2 {
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

impl fmt::Display for DeliveryErrorV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "delivery v2 error ({})",
            self.meta().correlation_id
        )
    }
}

impl std::error::Error for DeliveryErrorV2 {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapRequestV2 {
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBootstrapV2 {
    pub authenticated: bool,
    pub actor_ref: Option<String>,
    pub tenant_ref: Option<String>,
    pub role_ref: Option<String>,
    pub locale: String,
    pub channel: ClientChannelV2,
    pub catalog_version: u16,
}

impl SessionBootstrapV2 {
    pub fn cache_scope(&self) -> Option<SessionScopeV2> {
        Some(SessionScopeV2 {
            actor_ref: self.actor_ref.clone()?,
            tenant_ref: self.tenant_ref.clone()?,
            role_ref: self.role_ref.clone()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteQueryV2 {
    Work,
    Processes,
    Insights,
    Modules,
    Workspace {
        workspace: WorkspaceV2,
        resource: Option<ResourceNameV2>,
    },
    Record {
        workspace: WorkspaceV2,
        resource: ResourceNameV2,
        id: RouteUuidV2,
    },
    Reference {
        id: RouteUuidV2,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRequestV2 {
    pub route: RouteQueryV2,
    pub cursor: Option<String>,
    pub limit: u16,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSummaryV2 {
    pub id: RouteUuidV2,
    pub title: String,
    pub status: String,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteDataV2 {
    pub canonical_path: String,
    pub title: String,
    pub description: String,
    pub page: PageV2<ResourceSummaryV2>,
    pub freshness_tag: String,
    pub invalidation_tags: BTreeSet<String>,
    pub public_validator: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordTransitionV2 {
    Submit,
    Approve,
    Reject,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOperationV2 {
    CreateRecord {
        workspace: WorkspaceV2,
        resource: ResourceNameV2,
        title: String,
    },
    UpdateTitle {
        workspace: WorkspaceV2,
        resource: ResourceNameV2,
        id: RouteUuidV2,
        title: String,
    },
    TransitionRecord {
        workspace: WorkspaceV2,
        resource: ResourceNameV2,
        id: RouteUuidV2,
        transition: RecordTransitionV2,
    },
    ArchiveRecord {
        workspace: WorkspaceV2,
        resource: ResourceNameV2,
        id: RouteUuidV2,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRequestV2 {
    pub operation: CommandOperationV2,
    pub idempotency_key: String,
    pub expected_version: Option<u64>,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticationRequestV2 {
    pub account: String,
    pub proof: String,
    pub return_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicMarketingFormRequestV2 {
    pub form_id: RouteUuidV2,
    pub name: String,
    pub email: String,
    pub message: String,
    pub consent: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicMarketingPreviewRequestV2 {
    pub preview_id: RouteUuidV2,
    pub access_code: String,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationSubscriptionV2 {
    pub after_cursor: Option<String>,
    pub workspace: Option<WorkspaceV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationEventV2 {
    pub cursor: String,
    pub topic: String,
    pub summary: String,
}

#[async_trait(?Send)]
pub trait ConsoleGateway: Send + Sync {
    async fn bootstrap(
        &self,
        request: BootstrapRequestV2,
    ) -> Result<SessionBootstrapV2, DeliveryErrorV2>;

    async fn query(&self, request: QueryRequestV2) -> Result<RouteDataV2, DeliveryErrorV2>;

    async fn command(&self, request: CommandRequestV2)
    -> Result<CommandReceiptV2, DeliveryErrorV2>;

    async fn authenticate(
        &self,
        request: AuthenticationRequestV2,
    ) -> Result<SessionBootstrapV2, DeliveryErrorV2>;

    async fn submit_public_marketing_form(
        &self,
        request: PublicMarketingFormRequestV2,
    ) -> Result<CommandReceiptV2, DeliveryErrorV2>;

    async fn public_marketing_preview(
        &self,
        request: PublicMarketingPreviewRequestV2,
    ) -> Result<RouteDataV2, DeliveryErrorV2>;

    async fn collaboration_stream(
        &self,
        request: CollaborationSubscriptionV2,
    ) -> Result<Vec<CollaborationEventV2>, DeliveryErrorV2>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionScopeV2 {
    pub actor_ref: String,
    pub tenant_ref: String,
    pub role_ref: String,
}

#[derive(Debug, Clone)]
struct CacheEntryV2<V> {
    value: V,
    stored_at: Instant,
    fresh_for: Duration,
    byte_size: usize,
    tags: BTreeSet<String>,
}

/// Session-scoped LRU cache with both entry and byte ceilings.
#[derive(Debug)]
pub struct SessionCacheV2<K, V> {
    scope: Option<SessionScopeV2>,
    entries: BTreeMap<K, CacheEntryV2<V>>,
    lru: VecDeque<K>,
    total_bytes: usize,
}

impl<K, V> Default for SessionCacheV2<K, V> {
    fn default() -> Self {
        Self {
            scope: None,
            entries: BTreeMap::new(),
            lru: VecDeque::new(),
            total_bytes: 0,
        }
    }
}

impl<K, V> SessionCacheV2<K, V>
where
    K: Clone + Ord,
    V: Clone,
{
    pub fn set_scope(&mut self, next: Option<SessionScopeV2>) {
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
            CacheEntryV2 {
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

    fn scope(role: &str) -> SessionScopeV2 {
        SessionScopeV2 {
            actor_ref: "actor:one".to_owned(),
            tenant_ref: "tenant:one".to_owned(),
            role_ref: role.to_owned(),
        }
    }

    #[test]
    fn canonical_workspace_sets_cover_web_and_native_graphs() {
        assert_eq!(WorkspaceV2::ALL.len(), 21);
        assert_eq!(WorkspaceV2::NATIVE.len(), 19);
        assert!(WorkspaceV2::Commerce.is_web_only());
        assert!(WorkspaceV2::Talent.is_web_only());
        for workspace in WorkspaceV2::ALL {
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
                .parse::<RouteUuidV2>()
                .unwrap()
                .get(),
            id
        );
        assert!(id.simple().to_string().parse::<RouteUuidV2>().is_err());
        assert!(Uuid::nil().to_string().parse::<RouteUuidV2>().is_err());
        assert!("purchase-orders".parse::<ResourceNameV2>().is_ok());
        assert!("PurchaseOrders".parse::<ResourceNameV2>().is_err());
        assert!("purchase--orders".parse::<ResourceNameV2>().is_err());
    }

    #[test]
    fn cache_enforces_count_bytes_freshness_tags_and_scope() {
        let mut cache = SessionCacheV2::<u16, String>::default();
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
    fn service_range_is_exactly_sixty_five_v2_ids() {
        assert_eq!(
            SERVICE_ID_LAST_V2 - SERVICE_ID_FIRST_V2 + 1,
            SERVICE_COUNT_V2 as u16
        );
    }
}
