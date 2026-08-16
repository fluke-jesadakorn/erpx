//! Authorization-first BFF contracts for the federated Item Directory.
//!
//! The resolver intentionally distinguishes only success, neutral, and
//! degraded outcomes. Absence and denial are represented by the exact same
//! response and never expose type, title, route, backlink count, or freshness.

use std::collections::{BTreeMap, btree_map::Entry};

use erpx_integration_contracts::{ItemDescriptor, StableRecordRef};
use erpx_kernel::{RecordId, TenantId, TenantScope};

pub const CACHE_CONTROL_NO_STORE: &str = "no-store";
pub const MAX_PURPOSE_BYTES: usize = 128;
pub const MAX_SEARCH_BYTES: usize = 256;
pub const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRequest {
    record_id: RecordId,
    tenant_id: TenantId,
    authority_scope: TenantScope,
    purpose: String,
}

impl ReferenceRequest {
    pub fn try_new(
        record_id: RecordId,
        tenant_id: TenantId,
        authority_scope: TenantScope,
        purpose: impl Into<String>,
    ) -> Result<Self, DirectoryRequestError> {
        authority_scope
            .validate()
            .map_err(|_| DirectoryRequestError::InvalidScope)?;
        let purpose = purpose.into();
        validate_text(
            &purpose,
            MAX_PURPOSE_BYTES,
            DirectoryRequestError::InvalidPurpose,
        )?;
        Ok(Self {
            record_id,
            tenant_id,
            authority_scope,
            purpose,
        })
    }

    pub const fn record_id(&self) -> RecordId {
        self.record_id
    }

    pub const fn authority_scope(&self) -> &TenantScope {
        &self.authority_scope
    }

    pub const fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    tenant_id: TenantId,
    authority_scope: TenantScope,
    purpose: String,
    query: String,
    page_size: u32,
    cursor: Option<String>,
}

impl SearchRequest {
    pub fn try_new(
        tenant_id: TenantId,
        authority_scope: TenantScope,
        purpose: impl Into<String>,
        query: impl Into<String>,
        page_size: u32,
        cursor: Option<String>,
    ) -> Result<Self, DirectoryRequestError> {
        authority_scope
            .validate()
            .map_err(|_| DirectoryRequestError::InvalidScope)?;
        let purpose = purpose.into();
        let query = query.into();
        validate_text(
            &purpose,
            MAX_PURPOSE_BYTES,
            DirectoryRequestError::InvalidPurpose,
        )?;
        validate_text(
            &query,
            MAX_SEARCH_BYTES,
            DirectoryRequestError::InvalidSearch,
        )?;
        if page_size == 0 || page_size > MAX_PAGE_SIZE {
            return Err(DirectoryRequestError::InvalidPageSize);
        }
        if cursor.as_ref().is_some_and(|value| {
            value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        }) {
            return Err(DirectoryRequestError::InvalidCursor);
        }
        Ok(Self {
            tenant_id,
            authority_scope,
            purpose,
            query,
            page_size,
            cursor,
        })
    }
}

fn validate_text(
    value: &str,
    maximum: usize,
    error: DirectoryRequestError,
) -> Result<(), DirectoryRequestError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(error);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryRequestError {
    InvalidScope,
    InvalidPurpose,
    InvalidSearch,
    InvalidPageSize,
    InvalidCursor,
    DuplicateRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationDecision {
    Granted,
    Denied,
}

pub trait DirectoryAuthorizer: Send + Sync {
    /// Authorizes an opaque ID before a descriptor, match, or relation lookup.
    fn authorize_record(&self, request: &ReferenceRequest) -> AuthorizationDecision;

    /// Authorizes the exact scope before index selection or matching.
    fn authorize_search(&self, request: &SearchRequest) -> AuthorizationDecision;
}

/// Minimal opaque authorization index for local/server composition. It stores
/// identity and scope only; protected descriptor fields never enter the
/// authorization phase.
#[derive(Debug, Clone)]
pub struct ScopeBoundDirectoryAuthorizer {
    records: BTreeMap<RecordId, (TenantId, TenantScope)>,
}

impl ScopeBoundDirectoryAuthorizer {
    pub fn try_new(
        records: impl IntoIterator<Item = (RecordId, TenantId, TenantScope)>,
    ) -> Result<Self, DirectoryRequestError> {
        let mut index = BTreeMap::new();
        for (record_id, tenant_id, scope) in records {
            scope
                .validate()
                .map_err(|_| DirectoryRequestError::InvalidScope)?;
            match index.entry(record_id) {
                Entry::Vacant(entry) => {
                    entry.insert((tenant_id, scope));
                }
                Entry::Occupied(_) => return Err(DirectoryRequestError::DuplicateRecord),
            }
        }
        Ok(Self { records: index })
    }
}

impl DirectoryAuthorizer for ScopeBoundDirectoryAuthorizer {
    fn authorize_record(&self, request: &ReferenceRequest) -> AuthorizationDecision {
        self.records
            .get(&request.record_id)
            .filter(|(tenant_id, target_scope)| {
                *tenant_id == request.tenant_id && request.authority_scope.contains(target_scope)
            })
            .map_or(AuthorizationDecision::Denied, |_| {
                AuthorizationDecision::Granted
            })
    }

    fn authorize_search(&self, request: &SearchRequest) -> AuthorizationDecision {
        if self.records.values().any(|(tenant_id, target_scope)| {
            *tenant_id == request.tenant_id && request.authority_scope.contains(target_scope)
        }) {
            AuthorizationDecision::Granted
        } else {
            AuthorizationDecision::Denied
        }
    }
}

/// Capability created only after a current authorization decision. Projection
/// adapters can inspect it but cannot construct it themselves.
#[derive(Debug)]
pub struct AuthorizedReferenceQuery {
    record_id: RecordId,
    tenant_id: TenantId,
    authority_scope: TenantScope,
}

impl AuthorizedReferenceQuery {
    pub const fn record_id(&self) -> RecordId {
        self.record_id
    }

    pub const fn authority_scope(&self) -> &TenantScope {
        &self.authority_scope
    }

    pub const fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

#[derive(Debug)]
pub struct AuthorizedSearchQuery {
    tenant_id: TenantId,
    authority_scope: TenantScope,
    query: String,
    page_size: u32,
    cursor: Option<String>,
}

impl AuthorizedSearchQuery {
    pub const fn authority_scope(&self) -> &TenantScope {
        &self.authority_scope
    }

    pub const fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub const fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryFreshness {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProjection {
    pub descriptor: ItemDescriptor,
    pub freshness: DirectoryFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklinkProjection {
    pub incoming: Vec<StableRecordRef>,
    pub outgoing: Vec<StableRecordRef>,
    pub freshness: DirectoryFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchProjection {
    pub items: Vec<ItemDescriptor>,
    pub next_cursor: Option<String>,
    /// Count after scope, row, field, and record authorization. A projection
    /// must never populate this with the underlying hidden-match count.
    pub authorized_count: u64,
    pub freshness: DirectoryFreshness,
}

pub trait ItemDirectoryQueryPort: Send + Sync {
    fn resolve(&self, query: &AuthorizedReferenceQuery) -> Option<ResolvedProjection>;
    fn backlinks(&self, query: &AuthorizedReferenceQuery) -> Option<BacklinkProjection>;
    fn search(&self, query: &AuthorizedSearchQuery) -> SearchProjection;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceHttpResponse {
    Redirect {
        status: u16,
        location: String,
        cache_control: &'static str,
    },
    /// Identical response for denied and absent references.
    Neutral {
        status: u16,
        cache_control: &'static str,
    },
    Degraded {
        status: u16,
        cache_control: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BacklinkResponse {
    Visible {
        incoming: Vec<StableRecordRef>,
        outgoing: Vec<StableRecordRef>,
        authorized_count: u64,
    },
    Neutral,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchResponse {
    Visible {
        items: Vec<ItemDescriptor>,
        next_cursor: Option<String>,
        authorized_count: u64,
    },
    Neutral,
    Degraded,
}

pub struct ReferenceResolver<A, P> {
    authorizer: A,
    projection: P,
}

impl<A, P> ReferenceResolver<A, P>
where
    A: DirectoryAuthorizer,
    P: ItemDirectoryQueryPort,
{
    pub const fn new(authorizer: A, projection: P) -> Self {
        Self {
            authorizer,
            projection,
        }
    }

    pub fn resolve(&self, request: &ReferenceRequest) -> ReferenceHttpResponse {
        if self.authorizer.authorize_record(request) != AuthorizationDecision::Granted {
            return neutral_http();
        }
        let query = authorized_reference(request);
        let Some(result) = self.projection.resolve(&query) else {
            return neutral_http();
        };
        if result.descriptor.record().id() != request.record_id() {
            return degraded_http();
        }
        match result.freshness {
            DirectoryFreshness::Fresh => ReferenceHttpResponse::Redirect {
                status: 303,
                location: result.descriptor.canonical_route().to_owned(),
                cache_control: CACHE_CONTROL_NO_STORE,
            },
            DirectoryFreshness::Stale | DirectoryFreshness::Unavailable => degraded_http(),
        }
    }

    pub fn backlinks(&self, request: &ReferenceRequest) -> BacklinkResponse {
        if self.authorizer.authorize_record(request) != AuthorizationDecision::Granted {
            return BacklinkResponse::Neutral;
        }
        let Some(result) = self.projection.backlinks(&authorized_reference(request)) else {
            return BacklinkResponse::Neutral;
        };
        if result.freshness != DirectoryFreshness::Fresh {
            return BacklinkResponse::Degraded;
        }
        let incoming = self.authorized_links(request, result.incoming);
        let outgoing = self.authorized_links(request, result.outgoing);
        let authorized_count = u64::try_from(incoming.len() + outgoing.len()).unwrap_or(u64::MAX);
        BacklinkResponse::Visible {
            incoming,
            outgoing,
            authorized_count,
        }
    }

    pub fn search(&self, request: &SearchRequest) -> SearchResponse {
        if self.authorizer.authorize_search(request) != AuthorizationDecision::Granted {
            return SearchResponse::Neutral;
        }
        let query = AuthorizedSearchQuery {
            tenant_id: request.tenant_id,
            authority_scope: request.authority_scope.clone(),
            query: request.query.clone(),
            page_size: request.page_size,
            cursor: request.cursor.clone(),
        };
        let result = self.projection.search(&query);
        if result.freshness != DirectoryFreshness::Fresh
            || result.authorized_count < result.items.len() as u64
        {
            return SearchResponse::Degraded;
        }
        SearchResponse::Visible {
            items: result.items,
            next_cursor: result.next_cursor,
            authorized_count: result.authorized_count,
        }
    }

    fn authorized_links(
        &self,
        anchor: &ReferenceRequest,
        links: Vec<StableRecordRef>,
    ) -> Vec<StableRecordRef> {
        links
            .into_iter()
            .filter(|link| {
                let request = ReferenceRequest {
                    record_id: link.id(),
                    tenant_id: anchor.tenant_id,
                    authority_scope: anchor.authority_scope.clone(),
                    purpose: anchor.purpose.clone(),
                };
                self.authorizer.authorize_record(&request) == AuthorizationDecision::Granted
            })
            .collect()
    }
}

fn authorized_reference(request: &ReferenceRequest) -> AuthorizedReferenceQuery {
    AuthorizedReferenceQuery {
        record_id: request.record_id,
        tenant_id: request.tenant_id,
        authority_scope: request.authority_scope.clone(),
    }
}

const fn neutral_http() -> ReferenceHttpResponse {
    ReferenceHttpResponse::Neutral {
        status: 404,
        cache_control: CACHE_CONTROL_NO_STORE,
    }
}

const fn degraded_http() -> ReferenceHttpResponse {
    ReferenceHttpResponse::Degraded {
        status: 503,
        cache_control: CACHE_CONTROL_NO_STORE,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        sync::{
            Mutex,
            atomic::{AtomicU32, Ordering},
        },
    };

    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId, record_policy_id};
    use erpx_kernel::DataClassification;
    use uuid::Uuid;

    use super::*;

    struct TestAuthorizer {
        allowed: BTreeSet<RecordId>,
        search_allowed: bool,
        calls: Mutex<Vec<&'static str>>,
    }

    impl DirectoryAuthorizer for TestAuthorizer {
        fn authorize_record(&self, request: &ReferenceRequest) -> AuthorizationDecision {
            self.calls.lock().unwrap().push("authorize-record");
            if self.allowed.contains(&request.record_id()) {
                AuthorizationDecision::Granted
            } else {
                AuthorizationDecision::Denied
            }
        }

        fn authorize_search(&self, _: &SearchRequest) -> AuthorizationDecision {
            self.calls.lock().unwrap().push("authorize-search");
            if self.search_allowed {
                AuthorizationDecision::Granted
            } else {
                AuthorizationDecision::Denied
            }
        }
    }

    struct TestProjection {
        resolved: Option<ResolvedProjection>,
        backlinks: Option<BacklinkProjection>,
        search: SearchProjection,
        resolve_calls: AtomicU32,
        backlink_calls: AtomicU32,
        search_calls: AtomicU32,
    }

    impl ItemDirectoryQueryPort for TestProjection {
        fn resolve(&self, _: &AuthorizedReferenceQuery) -> Option<ResolvedProjection> {
            self.resolve_calls.fetch_add(1, Ordering::Relaxed);
            self.resolved.clone()
        }

        fn backlinks(&self, _: &AuthorizedReferenceQuery) -> Option<BacklinkProjection> {
            self.backlink_calls.fetch_add(1, Ordering::Relaxed);
            self.backlinks.clone()
        }

        fn search(&self, _: &AuthorizedSearchQuery) -> SearchProjection {
            self.search_calls.fetch_add(1, Ordering::Relaxed);
            self.search.clone()
        }
    }

    fn record(context: ContextId, kind: &str) -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            context,
            RecordTypeId::try_new(kind).unwrap(),
        )
        .unwrap()
    }

    fn tenant_id() -> TenantId {
        TenantId::try_from_uuid(Uuid::new_v4()).unwrap()
    }

    fn descriptor(record: StableRecordRef) -> ItemDescriptor {
        let authorization_policy = record_policy_id(&record).unwrap();
        ItemDescriptor::try_new(
            record,
            TenantScope::Platform,
            "open",
            authorization_policy,
            DataClassification::Internal,
            1,
            1,
        )
        .unwrap()
    }

    fn projection(resolved: Option<ResolvedProjection>) -> TestProjection {
        TestProjection {
            resolved,
            backlinks: None,
            search: SearchProjection {
                items: Vec::new(),
                next_cursor: None,
                authorized_count: 0,
                freshness: DirectoryFreshness::Fresh,
            },
            resolve_calls: AtomicU32::new(0),
            backlink_calls: AtomicU32::new(0),
            search_calls: AtomicU32::new(0),
        }
    }

    #[test]
    fn denial_happens_before_lookup_and_matches_absence() {
        let target = record(ContextId::Finance, "finance.invoice");
        let request =
            ReferenceRequest::try_new(target.id(), tenant_id(), TenantScope::Platform, "open")
                .expect("request");
        let denied_projection = projection(Some(ResolvedProjection {
            descriptor: descriptor(target.clone()),
            freshness: DirectoryFreshness::Fresh,
        }));
        let denied = ReferenceResolver::new(
            TestAuthorizer {
                allowed: BTreeSet::new(),
                search_allowed: false,
                calls: Mutex::new(Vec::new()),
            },
            denied_projection,
        );
        let denied_response = denied.resolve(&request);
        assert_eq!(denied.projection.resolve_calls.load(Ordering::Relaxed), 0);
        assert_eq!(denied.backlinks(&request), BacklinkResponse::Neutral);
        assert_eq!(denied.projection.backlink_calls.load(Ordering::Relaxed), 0);

        let absent = ReferenceResolver::new(
            TestAuthorizer {
                allowed: [target.id()].into_iter().collect(),
                search_allowed: false,
                calls: Mutex::new(Vec::new()),
            },
            projection(None),
        );
        let absent_response = absent.resolve(&request);
        assert_eq!(denied_response, absent_response);
        assert_eq!(absent.projection.resolve_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn fresh_resolution_is_303_no_store_and_stale_is_degraded() {
        let target = record(ContextId::Projects, "project.item");
        let request =
            ReferenceRequest::try_new(target.id(), tenant_id(), TenantScope::Platform, "open")
                .expect("request");
        let allowed = [target.id()].into_iter().collect();
        let resolver = ReferenceResolver::new(
            TestAuthorizer {
                allowed,
                search_allowed: false,
                calls: Mutex::new(Vec::new()),
            },
            projection(Some(ResolvedProjection {
                descriptor: descriptor(target.clone()),
                freshness: DirectoryFreshness::Fresh,
            })),
        );
        assert_eq!(
            resolver.resolve(&request),
            ReferenceHttpResponse::Redirect {
                status: 303,
                location: target.canonical_detail_route(),
                cache_control: CACHE_CONTROL_NO_STORE,
            }
        );

        let stale = ReferenceResolver::new(
            TestAuthorizer {
                allowed: [target.id()].into_iter().collect(),
                search_allowed: false,
                calls: Mutex::new(Vec::new()),
            },
            projection(Some(ResolvedProjection {
                descriptor: descriptor(target),
                freshness: DirectoryFreshness::Stale,
            })),
        );
        assert_eq!(stale.resolve(&request), degraded_http());
    }

    #[test]
    fn backlinks_remove_hidden_items_without_leaking_the_hidden_count() {
        let anchor = record(ContextId::Legal, "legal.contract");
        let visible = record(ContextId::Finance, "finance.invoice");
        let hidden = record(ContextId::Sales, "sales.order");
        let request =
            ReferenceRequest::try_new(anchor.id(), tenant_id(), TenantScope::Platform, "backlinks")
                .expect("request");
        let resolver = ReferenceResolver::new(
            TestAuthorizer {
                allowed: [anchor.id(), visible.id()].into_iter().collect(),
                search_allowed: false,
                calls: Mutex::new(Vec::new()),
            },
            TestProjection {
                resolved: None,
                backlinks: Some(BacklinkProjection {
                    incoming: vec![visible.clone(), hidden],
                    outgoing: Vec::new(),
                    freshness: DirectoryFreshness::Fresh,
                }),
                search: SearchProjection {
                    items: Vec::new(),
                    next_cursor: None,
                    authorized_count: 0,
                    freshness: DirectoryFreshness::Fresh,
                },
                resolve_calls: AtomicU32::new(0),
                backlink_calls: AtomicU32::new(0),
                search_calls: AtomicU32::new(0),
            },
        );
        assert_eq!(
            resolver.backlinks(&request),
            BacklinkResponse::Visible {
                incoming: vec![visible],
                outgoing: Vec::new(),
                authorized_count: 1,
            }
        );
    }

    #[test]
    fn search_authorization_precedes_projection_matching() {
        let request = SearchRequest::try_new(
            tenant_id(),
            TenantScope::Platform,
            "find",
            "invoice",
            25,
            None,
        )
        .expect("request");
        let resolver = ReferenceResolver::new(
            TestAuthorizer {
                allowed: BTreeSet::new(),
                search_allowed: false,
                calls: Mutex::new(Vec::new()),
            },
            projection(None),
        );
        assert_eq!(resolver.search(&request), SearchResponse::Neutral);
        assert_eq!(resolver.projection.search_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn tenant_and_branch_scope_are_checked_before_directory_lookup() {
        let tenant = tenant_id();
        let other_tenant = tenant_id();
        let target = record(ContextId::Inventory, "inventory.document");
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let target_branch = Uuid::new_v4();
        let sibling_branch = Uuid::new_v4();
        let target_scope = TenantScope::Branch {
            holding_id,
            company_id,
            branch_id: target_branch,
        };
        let authorizer =
            ScopeBoundDirectoryAuthorizer::try_new([(target.id(), tenant, target_scope.clone())])
                .unwrap();

        let allowed = ReferenceRequest::try_new(target.id(), tenant, target_scope, "open").unwrap();
        assert_eq!(
            authorizer.authorize_record(&allowed),
            AuthorizationDecision::Granted
        );

        let sibling = ReferenceRequest::try_new(
            target.id(),
            tenant,
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id: sibling_branch,
            },
            "open",
        )
        .unwrap();
        assert_eq!(
            authorizer.authorize_record(&sibling),
            AuthorizationDecision::Denied
        );

        let cross_tenant =
            ReferenceRequest::try_new(target.id(), other_tenant, TenantScope::Platform, "open")
                .unwrap();
        assert_eq!(
            authorizer.authorize_record(&cross_tenant),
            AuthorizationDecision::Denied
        );
    }
}
