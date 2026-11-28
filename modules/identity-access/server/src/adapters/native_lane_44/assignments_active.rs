//! Bounded native execution of `GET .../assignments/active`.
//!
//! The owner SQL reader (`account_work_roles::read_account_work_roles`) executes
//! the real assignment/role/selection statements inside a verified Holding
//! transaction. This module owns the exact public payload and the owner port
//! that lends that transaction; it never accepts a client-supplied actor.

use super::application::{NativeRouteBackend, RouteFuture, RouteReply, RouteRequest};
use super::domain::RouteFamily;
use crate::adapters::account_work_roles::read_account_work_roles;
use crate::adapters::holding_work_context::HoldingAccountWorkScope;
use crate::domain::work_context::{WorkScopeKind, assignment_applies};
use erpx_platform_runtime_server::public::http_support::HeaderMap;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type WorkResult<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// One option exactly as the retired TypeScript route exposed it.
#[derive(Clone, Debug)]
pub struct ActiveWorkOption {
    pub id: String,
    pub version: i64,
    pub role: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub modules: Vec<String>,
    pub scope_kind: WorkScopeKind,
    pub scope_node_id: Option<String>,
    pub label: String,
    pub valid_until: i64,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    /// Whether this assignment applies to the verified requested workspace.
    pub applies: bool,
}

/// One current selection, mapped from the owner's `active_work_contexts` read.
#[derive(Clone, Debug)]
pub struct ActiveWorkSelection {
    pub assignment_id: Option<String>,
    pub assignment_version: Option<i64>,
    pub revision: i64,
    pub expires_at: i64,
}

/// Owner facts needed for the public payload, derived inside the verified lease.
#[derive(Clone, Debug)]
pub struct ActiveWorkRoleView {
    pub company_id: String,
    pub user_id: String,
    pub options: Vec<ActiveWorkOption>,
    pub selection: Option<ActiveWorkSelection>,
}

impl ActiveWorkRoleView {
    /// Map the SQL-backed owner read into transport-neutral facts. Every field
    /// below comes from a verified Holding transaction, never the request body.
    pub fn from_owner(
        roles: &crate::adapters::account_work_roles::CurrentAccountWorkRoles,
    ) -> Self {
        let workspace = roles.workspace();
        let options = roles
            .choices()
            .map(|choice| {
                let a = choice.assignment();
                ActiveWorkOption {
                    id: a.id.as_str().into(),
                    version: a.version,
                    role: a.role.clone(),
                    company_id: a.company_id.as_str().into(),
                    branch_id: choice.branch_id().map(|id| id.as_str().into()),
                    modules: a.modules.clone(),
                    scope_kind: a.scope_kind,
                    scope_node_id: a.scope_node_id.as_ref().map(|id| id.as_str().into()),
                    label: choice.label().into(),
                    valid_until: a.valid_until,
                    branch_slug: choice.branch_slug().map(str::to_owned),
                    department_slug: choice.department_slug().map(str::to_owned),
                    applies: assignment_applies(a, workspace),
                }
            })
            .collect();
        let selection = roles.selection().map(|s| ActiveWorkSelection {
            assignment_id: s.assignment_id.as_ref().map(|id| id.as_str().into()),
            assignment_version: s.assignment_version,
            revision: s.revision,
            expires_at: s.expires_at,
        });
        Self {
            company_id: workspace.company_id.as_str().into(),
            user_id: roles.user_id().as_str().into(),
            options,
            selection,
        }
    }
}

fn encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn workspace_href(holding_slug: &str, option: &ActiveWorkOption) -> String {
    let holding = format!("/holdings/{}", encode_segment(holding_slug));
    match option.scope_kind {
        WorkScopeKind::Holding => format!("{holding}/home"),
        WorkScopeKind::Branch => format!(
            "{holding}/branches/{}/home",
            encode_segment(option.branch_slug.as_deref().unwrap_or_default())
        ),
        WorkScopeKind::Department => {
            let branch = option
                .branch_slug
                .as_deref()
                .map(|slug| format!("/branches/{}", encode_segment(slug)))
                .unwrap_or_default();
            format!(
                "{holding}{branch}/departments/{}/home",
                encode_segment(option.department_slug.as_deref().unwrap_or_default())
            )
        }
    }
}

/// The exact JSON object the retired route returned under `data`.
pub fn active_work_role_payload(
    view: &ActiveWorkRoleView,
    holding_slug: &str,
    now: i64,
) -> Result<Value, BusinessError> {
    if view.options.len() > 1000 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "ACCOUNT_ACCESS_UNAVAILABLE",
            "The active assignment set exceeds this bounded read.",
        ));
    }
    let options = view
        .options
        .iter()
        .map(|option| {
            let scope_kind = match option.scope_kind {
                WorkScopeKind::Holding => "holding",
                WorkScopeKind::Branch => "branch",
                WorkScopeKind::Department => "department",
            };
            json!({
                "id": option.id,
                "version": option.version,
                "role": option.role,
                "companyId": option.company_id,
                "branchId": option.branch_id,
                "modules": option.modules,
                "scopeKind": scope_kind,
                "scopeNodeId": option.scope_node_id,
                "label": option.label,
                "validUntil": option.valid_until,
                "branchSlug": option.branch_slug,
                "departmentSlug": option.department_slug,
                "href": workspace_href(holding_slug, option),
            })
        })
        .collect::<Vec<_>>();
    let selected = view.selection.as_ref().filter(|s| {
        s.expires_at > now
            && s.assignment_id.as_deref().is_some_and(|id| {
                view.options.iter().any(|option| {
                    option.applies
                        && option.id == id
                        && Some(option.version) == s.assignment_version
                })
            })
    });
    Ok(json!({
        "valid": selected.is_some(),
        "options": options,
        "revision": view.selection.as_ref().map_or(0, |s| s.revision),
        "assignmentVersion": selected.and_then(|s| s.assignment_version),
        "assignmentId": selected.map(|s| s.assignment_id.as_deref().unwrap_or_default()),
        "companyId": view.company_id,
        "userId": view.user_id,
    }))
}

/// Lends an authenticated Holding transaction to an owner read. Implemented by
/// the composition root with `HoldingWorkContextInspector::with_account_work`;
/// the owner crate decides which SQL executes inside the lease.
pub type ActiveWorkFn = Arc<
    dyn for<'c> Fn(
            &'c mut PgConnection,
            &'c HoldingAccountWorkScope,
        ) -> WorkResult<'c, ActiveWorkRoleView>
        + Send
        + Sync,
>;

pub trait WorkScopeLease: Send + Sync + 'static {
    fn load_active_roles(
        &self,
        host: &str,
        headers: &HeaderMap,
        work: ActiveWorkFn,
    ) -> WorkResult<'_, ActiveWorkRoleView>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_ACCESS_UNAVAILABLE",
        "Unable to read current account access.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Axum-facing backend for the active assignment read. The SQL is executed by
/// `account_work_roles::read_account_work_roles` through `WorkScopeLease`.
pub struct PgAssignmentsActiveBackend {
    lease: Arc<dyn WorkScopeLease>,
    clock: Arc<dyn Clock + Send + Sync>,
}

impl PgAssignmentsActiveBackend {
    pub fn new(lease: Arc<dyn WorkScopeLease>, clock: Arc<dyn Clock + Send + Sync>) -> Self {
        Self { lease, clock }
    }
}

impl NativeRouteBackend for PgAssignmentsActiveBackend {
    fn execute<'a>(&'a self, request: RouteRequest<'a>) -> RouteFuture<'a, RouteReply> {
        Box::pin(async move {
            if request.family != RouteFamily::AssignmentsActive {
                return Err(unavailable());
            }
            let now = self.clock.now().get();
            let work: ActiveWorkFn = Arc::new(move |c, scope| {
                Box::pin(async move {
                    let roles = read_account_work_roles(c, scope, now, true)
                        .await
                        .map_err(business)?;
                    Ok(ActiveWorkRoleView::from_owner(&roles))
                })
            });
            let view = self
                .lease
                .load_active_roles(request.host, request.headers, work)
                .await?;
            let data = active_work_role_payload(&view, request.holding_slug, now)?;
            Ok(RouteReply::data(data))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(
        id: &str,
        version: i64,
        scope_kind: WorkScopeKind,
        applies: bool,
    ) -> ActiveWorkOption {
        ActiveWorkOption {
            id: id.into(),
            version,
            role: "ledger_reader".into(),
            company_id: "company-1".into(),
            branch_id: None,
            modules: vec!["work.get".into()],
            scope_kind,
            scope_node_id: None,
            label: "Ledger reader".into(),
            valid_until: 9_000,
            branch_slug: None,
            department_slug: None,
            applies,
        }
    }

    fn view(selection: Option<ActiveWorkSelection>) -> ActiveWorkRoleView {
        ActiveWorkRoleView {
            company_id: "company-1".into(),
            user_id: "alpha".into(),
            options: vec![
                option("assignment-1", 3, WorkScopeKind::Holding, true),
                option("assignment-2", 1, WorkScopeKind::Branch, false),
            ],
            selection,
        }
    }

    #[test]
    fn active_role_payload_matches_retired_route_fields() {
        let data = active_work_role_payload(
            &view(Some(ActiveWorkSelection {
                assignment_id: Some("assignment-1".into()),
                assignment_version: Some(3),
                revision: 7,
                expires_at: 9_500,
            })),
            "acme",
            9_000,
        )
        .unwrap();
        assert_eq!(
            data,
            json!({
                "valid": true,
                "options": [
                    {
                        "id": "assignment-1", "version": 3, "role": "ledger_reader",
                        "companyId": "company-1", "branchId": null,
                        "modules": ["work.get"], "scopeKind": "holding",
                        "scopeNodeId": null, "label": "Ledger reader",
                        "validUntil": 9000, "branchSlug": null, "departmentSlug": null,
                        "href": "/holdings/acme/home",
                    },
                    {
                        "id": "assignment-2", "version": 1, "role": "ledger_reader",
                        "companyId": "company-1", "branchId": null,
                        "modules": ["work.get"], "scopeKind": "branch",
                        "scopeNodeId": null, "label": "Ledger reader",
                        "validUntil": 9000, "branchSlug": null, "departmentSlug": null,
                        "href": "/holdings/acme/branches//home",
                    },
                ],
                "revision": 7,
                "assignmentVersion": 3,
                "assignmentId": "assignment-1",
                "companyId": "company-1",
                "userId": "alpha",
            })
        );
    }

    #[test]
    fn expired_or_inapplicable_selection_is_not_valid() {
        for selection in [
            ActiveWorkSelection {
                assignment_id: Some("assignment-1".into()),
                assignment_version: Some(3),
                revision: 7,
                expires_at: 9_000,
            },
            ActiveWorkSelection {
                assignment_id: Some("assignment-2".into()),
                assignment_version: Some(1),
                revision: 7,
                expires_at: 9_500,
            },
        ] {
            let data = active_work_role_payload(&view(Some(selection)), "acme", 9_000).unwrap();
            assert_eq!(data["valid"], false);
            assert_eq!(data["assignmentVersion"], Value::Null);
            assert_eq!(data["assignmentId"], Value::Null);
        }
        let empty = active_work_role_payload(&view(None), "acme", 9_000).unwrap();
        assert_eq!(empty["revision"], 0);
        assert_eq!(empty["valid"], false);
    }
}
