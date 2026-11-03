//! Current account-only role choices and selected-session writes. The verified
//! Holding account scope is mandatory; these facts grant no business permission.
use super::holding_work_context::HoldingAccountWorkScope;
use crate::domain::work_context::{
    WorkAssignmentFact, WorkScopeKind, WorkspaceLineageFact, assignment_applies,
};
use erpx_server_organization::public::{
    CompanyContext, NodeKind, OrganizationNode, PgOrganizationReader, WorkspaceRouteQuery,
    lock_hierarchy_sources, read_company_context, resolve_workspace,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{AssignmentId, BusinessError, FailureKind, RecordId};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your access or work role changed. Review and choose again.",
    )
    .into()
}
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_ACCESS_UNAVAILABLE",
        "Unable to read current account access.",
    )
    .into()
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct HierarchyImage(
    OrganizationNode,
    Option<OrganizationNode>,
    Option<OrganizationNode>,
);
fn hierarchy(c: &CompanyContext) -> HierarchyImage {
    HierarchyImage(
        c.company().clone(),
        c.branch().cloned(),
        c.department().cloned(),
    )
}
#[derive(Clone, Deserialize)]
struct Assignment {
    id: String,
    user_id: String,
    company_id: String,
    role: String,
    modules_json: String,
    scope_kind: String,
    scope_node_id: Option<String>,
    version: i64,
    valid_from: i64,
    valid_until: i64,
    revoked_at: Option<i64>,
}
#[derive(Clone, Deserialize)]
struct Role {
    id: String,
    company_id: String,
    name: String,
    published_revision: i64,
    status: String,
    scopes: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AccountWorkSelection {
    pub assignment_id: Option<String>,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    pub assignment_version: Option<i64>,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub revision: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub expires_at: i64,
}
#[derive(Clone, Debug)]
pub struct AccountWorkRoleChoice {
    assignment: WorkAssignmentFact,
    label: String,
    branch_id: Option<RecordId>,
    branch_slug: Option<String>,
    department_slug: Option<String>,
}
impl AccountWorkRoleChoice {
    pub fn branch_id(&self) -> Option<&RecordId> {
        self.branch_id.as_ref()
    }
    pub fn branch_slug(&self) -> Option<&str> {
        self.branch_slug.as_deref()
    }
    pub fn department_slug(&self) -> Option<&str> {
        self.department_slug.as_deref()
    }
    pub fn assignment(&self) -> &WorkAssignmentFact {
        &self.assignment
    }
    pub fn label(&self) -> &str {
        &self.label
    }
}
#[derive(Clone)]
struct Image {
    iam: Value,
    organization: Vec<HierarchyImage>,
    selection: Option<Value>,
}
impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.iam == other.iam
            && self.organization == other.organization
            && self.selection == other.selection
    }
}
impl Eq for Image {}
struct Captured {
    image: Image,
    choices: Vec<AccountWorkRoleChoice>,
    workspace: WorkspaceLineageFact,
    selection: Option<AccountWorkSelection>,
}
/// Write intent precedes the first Holding snapshot. Exclusive context ordering
/// also waits for a legacy role selector before taking preference/audit locks.
pub async fn lock_account_work_roles(c: &mut PgConnection, write: bool) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    if write {
        sqlx::query("LOCK TABLE active_work_contexts IN EXCLUSIVE MODE")
            .execute(&mut *c)
            .await?;
    } else {
        sqlx::query("LOCK TABLE active_work_contexts IN SHARE MODE")
            .execute(&mut *c)
            .await?;
    }
    sqlx::query("LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_revoked_sessions,iam_access_subjects,iam_access_invitations IN SHARE MODE").execute(&mut *c).await?;
    lock_hierarchy_sources(c).await
}
async fn capture(
    c: &mut PgConnection,
    scope: &HoldingAccountWorkScope,
    now: i64,
) -> Result<Captured, Error> {
    scope.require_current(c, now).await?;
    let r = scope.request();
    let gate=sqlx::query("SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1 AND user_id=$2 AND company_id=$3) revoked,iam_principal_active($2,$3,$4) principal_active,EXISTS(SELECT 1 FROM iam_members m JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id WHERE m.user_id=$2 AND m.company_id=$3 AND m.status='active' AND m.login_policy='directory' AND c.status='active' AND i.observed_at<=$4-300000) directory_stale").bind(r.session_id.as_str()).bind(r.user_id.as_str()).bind(r.company_id.as_str()).bind(now).fetch_one(&mut *c).await?;
    if gate.try_get::<bool, _>("revoked")? {
        return Err(BusinessError::new(
            FailureKind::Unauthenticated,
            "SESSION_REVOKED",
            "Sign in again to access this company.",
        )
        .into());
    }
    if gate.try_get::<bool, _>("directory_stale")? {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "DIRECTORY_ACCESS_UNAVAILABLE",
            "Your directory access needs a successful refresh.",
        )
        .into());
    }
    if !gate.try_get::<bool, _>("principal_active")? {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ACCOUNT_ACCESS_UNAVAILABLE",
            "The Company account is unavailable.",
        )
        .into());
    }
    let rows=sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(a)::text)<=32768 THEN to_jsonb(a) END image,iam_assignment_active(a.id,$3) AND a.revoked_at IS NULL AND a.valid_from<=$3 AND a.valid_until>$3 active FROM assignments a WHERE a.user_id=$1 AND a.company_id=$2 ORDER BY a.id LIMIT 1001 FOR SHARE OF a").bind(r.user_id.as_str()).bind(r.company_id.as_str()).bind(now).fetch_all(&mut *c).await?;
    if rows.len() > 1000 {
        return Err(unavailable());
    }
    let roles:Vec<Option<Value>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(jsonb_build_object('id',role.id,'company_id',role.company_id,'name',role.name,'published_revision',role.published_revision,'status',role.status,'scopes',revision.scopes_json)::text)<=8192 THEN jsonb_build_object('id',role.id,'company_id',role.company_id,'name',role.name,'published_revision',role.published_revision,'status',role.status,'scopes',revision.scopes_json) END FROM iam_roles role JOIN iam_role_revisions revision ON revision.company_id=role.company_id AND revision.role_id=role.id AND revision.revision=role.published_revision WHERE role.company_id IN('*',$1) AND role.id IN(SELECT role FROM assignments WHERE user_id=$2 AND company_id=$1) ORDER BY role.company_id,role.id LIMIT 1001 FOR SHARE OF role,revision").bind(r.company_id.as_str()).bind(r.user_id.as_str()).fetch_all(&mut *c).await?;
    if roles.len() > 1000 || roles.iter().any(Option::is_none) {
        return Err(unavailable());
    }
    let roles = roles
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(unavailable)?;
    let definitions = roles
        .iter()
        .cloned()
        .map(serde_json::from_value::<Role>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| unavailable())?;
    let selection:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(s) FROM active_work_contexts s WHERE session_id=$1 AND company_id=$2 FOR SHARE OF s").bind(r.session_id.as_str()).bind(r.company_id.as_str()).fetch_optional(&mut *c).await?;
    let selected = selection
        .as_ref()
        .map(|s| {
            if s["user_id"] != r.user_id.as_str()
                || s["session_id"] != r.session_id.as_str()
                || s["company_id"] != r.company_id.as_str()
            {
                return Err(changed());
            }
            let optional_string = |key: &str| -> Result<Option<String>, Error> {
                if s[key].is_null() {
                    Ok(None)
                } else {
                    s[key].as_str().map(|s| Some(s.into())).ok_or_else(changed)
                }
            };
            let optional_number = |key: &str| -> Result<Option<i64>, Error> {
                if s[key].is_null() {
                    Ok(None)
                } else {
                    s[key]
                        .as_i64()
                        .filter(|v| *v >= 1 && *v <= 9_007_199_254_740_991)
                        .map(Some)
                        .ok_or_else(changed)
                }
            };
            Ok(AccountWorkSelection {
                assignment_id: optional_string("assignment_id")?,
                assignment_version: optional_number("assignment_version")?,
                revision: s["revision"]
                    .as_i64()
                    .filter(|v| *v >= 0 && *v <= 9_007_199_254_740_991)
                    .ok_or_else(changed)?,
                expires_at: s["expires_at"].as_i64().ok_or_else(changed)?,
            })
        })
        .transpose()?;
    let mut organization = PgOrganizationReader::new(r.holding_id.clone(), &mut *c);
    let company = read_company_context(&mut organization, &r.company_id).await?;
    let requested = if r.branch_slug.is_none() && r.department_slug.is_none() {
        company.clone()
    } else {
        resolve_workspace(
            &mut organization,
            &WorkspaceRouteQuery::for_company_id(
                r.branch_slug.clone(),
                r.department_slug.clone(),
                r.company_id.clone(),
            )?,
        )
        .await?
        .ok_or_else(changed)?
    };
    let workspace = WorkspaceLineageFact {
        company_id: r.company_id.clone(),
        branch_id: requested.branch().map(|n| n.id.clone()),
        department_id: requested.department().map(|n| n.id.clone()),
        branch_slug: requested.branch().map(|n| n.slug.clone()),
        department_slug: requested.department().map(|n| n.slug.clone()),
    };
    let mut organization_image = vec![hierarchy(&company), hierarchy(&requested)];
    let mut raw = Vec::new();
    let mut choices = Vec::new();
    let mut size = 0usize;
    for row in rows {
        let image: Value = row
            .try_get::<Option<Value>, _>("image")?
            .ok_or_else(unavailable)?;
        let active: bool = row.try_get("active")?;
        size = size
            .checked_add(image.to_string().len())
            .ok_or_else(unavailable)?;
        if size > 16 * 1024 * 1024 {
            return Err(unavailable());
        }
        let a: Assignment = serde_json::from_value(image.clone()).map_err(|_| unavailable())?;
        raw.push(json!([image, active]));
        if !active {
            continue;
        }
        let matching = definitions
            .iter()
            .filter(|role| {
                role.id == a.role
                    && role.status == "active"
                    && (role.company_id == "*" || role.company_id == a.company_id)
            })
            .collect::<Vec<_>>();
        if matching.len() > 1 {
            return Err(unavailable());
        }
        let Some(role) = matching.first() else {
            continue;
        };
        if role.published_revision < 1 || !role.scopes.contains(&a.scope_kind) {
            continue;
        }
        let kind = match a.scope_kind.as_str() {
            "holding" => WorkScopeKind::Holding,
            "branch" => WorkScopeKind::Branch,
            "department" => WorkScopeKind::Department,
            _ => return Err(unavailable()),
        };
        let context = match kind {
            WorkScopeKind::Holding => company.clone(),
            _ => {
                let id = RecordId::new(a.scope_node_id.clone().ok_or_else(unavailable)?)?;
                match organization
                    .scope_node_context(
                        &r.company_id,
                        if kind == WorkScopeKind::Branch {
                            NodeKind::Branch
                        } else {
                            NodeKind::Department
                        },
                        &id,
                    )
                    .await
                {
                    Ok(context) => context,
                    Err(Error::Business(e)) if e.kind == FailureKind::NotFound => continue,
                    Err(e) => return Err(e),
                }
            }
        };
        organization_image.push(hierarchy(&context));
        if a.user_id != r.user_id.as_str()
            || a.company_id != r.company_id.as_str()
            || !(1..=9_007_199_254_740_991).contains(&a.version)
        {
            return Err(unavailable());
        }
        let assignment = WorkAssignmentFact {
            id: AssignmentId::new(a.id)?,
            user_id: r.user_id.clone(),
            company_id: r.company_id.clone(),
            role: a.role,
            modules: serde_json::from_str(&a.modules_json).map_err(|_| unavailable())?,
            scope_kind: kind,
            scope_node_id: a.scope_node_id.map(RecordId::new).transpose()?,
            version: a.version,
            valid_from: a.valid_from,
            valid_until: a.valid_until,
            revoked: a.revoked_at.is_some(),
        };
        let mut label = vec![role.name.clone(), company.company().display_name.clone()];
        if let Some(branch) = context.branch() {
            label.push(branch.display_name.clone());
        }
        if let Some(department) = context.department() {
            label.push(department.display_name.clone());
        }
        choices.push(AccountWorkRoleChoice {
            assignment,
            label: label.join(" · "),
            branch_id: context.branch().map(|n| n.id.clone()),
            branch_slug: context.branch().map(|n| n.slug.clone()),
            department_slug: context.department().map(|n| n.slug.clone()),
        });
    }
    if choices.is_empty() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "WORKSPACE_UNAVAILABLE",
            "That workspace is unavailable.",
        )
        .into());
    }
    Ok(Captured {
        image: Image {
            iam: json!([raw, roles]),
            organization: organization_image,
            selection,
        },
        choices,
        workspace,
        selection: selected,
    })
}
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AccountWorkspaceChoice {
    #[cfg_attr(
        feature = "contracts",
        ts(type = r#""holding" | "branch" | "department""#)
    )]
    pub kind: &'static str,
    pub id: String,
    pub label: String,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub inherited: bool,
}
pub struct CurrentAccountWorkRoles {
    unit: (String, i32, String, String),
    captured: Captured,
}
impl CurrentAccountWorkRoles {
    pub fn company_name(&self) -> &str {
        &self.captured.image.organization[1].0.display_name
    }
    pub fn company_slug(&self) -> &str {
        &self.captured.image.organization[1].0.slug
    }
    pub fn branch_name(&self) -> Option<&str> {
        self.captured.image.organization[1]
            .1
            .as_ref()
            .map(|n| n.display_name.as_str())
    }
    pub fn department_name(&self) -> Option<&str> {
        self.captured.image.organization[1]
            .2
            .as_ref()
            .map(|n| n.display_name.as_str())
    }
    pub async fn workspace_choices(
        &self,
        c: &mut PgConnection,
        scope: &HoldingAccountWorkScope,
        now: i64,
    ) -> Result<Vec<AccountWorkspaceChoice>, Error> {
        self.require_current(c, scope, now).await?;
        let r = scope.request();
        let mut result = Vec::new();
        if self
            .choices()
            .any(|g| g.assignment.scope_kind == WorkScopeKind::Holding)
        {
            result.push(AccountWorkspaceChoice {
                kind: "holding",
                id: r.company_id.as_str().into(),
                label: "Holding".into(),
                branch_slug: None,
                department_slug: None,
                inherited: false,
            });
        }
        for context in PgOrganizationReader::new(r.holding_id.clone(), c)
            .workspace_lineages(&r.company_id)
            .await?
        {
            let w = WorkspaceLineageFact {
                company_id: r.company_id.clone(),
                branch_id: context.branch().map(|n| n.id.clone()),
                department_id: context.department().map(|n| n.id.clone()),
                branch_slug: context.branch().map(|n| n.slug.clone()),
                department_slug: context.department().map(|n| n.slug.clone()),
            };
            if !self
                .choices()
                .any(|g| assignment_applies(&g.assignment, &w))
            {
                continue;
            }
            let (kind, node) = if let Some(n) = context.department() {
                (WorkScopeKind::Department, n)
            } else {
                (WorkScopeKind::Branch, context.branch().ok_or_else(changed)?)
            };
            let exact = self.choices().any(|g| {
                g.assignment.scope_kind == kind
                    && g.assignment.scope_node_id.as_ref() == Some(&node.id)
            });
            result.push(AccountWorkspaceChoice {
                kind: if kind == WorkScopeKind::Branch {
                    "branch"
                } else {
                    "department"
                },
                id: node.id.as_str().into(),
                label: node.display_name.clone(),
                branch_slug: w.branch_slug,
                department_slug: w.department_slug,
                inherited: !exact,
            });
        }
        Ok(result)
    }
    pub fn workspace(&self) -> &WorkspaceLineageFact {
        &self.captured.workspace
    }
    pub fn choices(&self) -> impl Iterator<Item = &AccountWorkRoleChoice> {
        self.captured.choices.iter()
    }
    pub fn eligible(&self) -> impl Iterator<Item = &AccountWorkRoleChoice> {
        self.captured
            .choices
            .iter()
            .filter(|c| assignment_applies(&c.assignment, &self.captured.workspace))
    }
    pub fn selection(&self) -> Option<&AccountWorkSelection> {
        self.captured.selection.as_ref()
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        scope: &HoldingAccountWorkScope,
        now: i64,
    ) -> Result<(), Error> {
        scope.require_current(c, now).await?;
        let actual = capture(c, scope, now).await.map_err(|e| match e {
            Error::Business(_) => changed(),
            other => other,
        })?;
        if super::selected_command_authority::selected_unit(c).await? != self.unit
            || actual.image != self.captured.image
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn select(
        self,
        c: &mut PgConnection,
        scope: &HoldingAccountWorkScope,
        id: &AssignmentId,
        version: i64,
        revision: i64,
        now: i64,
    ) -> Result<RetainedAccountWorkSelection, Error> {
        self.require_current(c, scope, now).await?;
        let request = scope.request();
        if revision != self.selection().map_or(0, |s| s.revision)
            || revision >= 9_007_199_254_740_991
        {
            return Err(changed());
        }
        let choice = self
            .choices()
            .find(|a| &a.assignment.id == id && a.assignment.version == version)
            .cloned()
            .ok_or_else(changed)?;
        if self.selection().is_some_and(|s| {
            s.assignment_id.as_deref() == Some(id.as_str())
                && s.assignment_version == Some(version)
                && s.revision == revision
                && s.expires_at > now
        }) {
            return Ok(RetainedAccountWorkSelection {
                current: self,
                choice,
                changed: false,
            });
        }
        sqlx::query("SELECT set_config('erpx.work_selection_runtime','native-rust',true)")
            .execute(&mut *c)
            .await?;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,expires_at,updated_at)VALUES($1,$2,$3,$4,$5)ON CONFLICT DO NOTHING").bind(request.session_id.as_str()).bind(request.user_id.as_str()).bind(request.company_id.as_str()).bind(request.session_expires_at).bind(now).execute(&mut *c).await?;
        let updated=sqlx::query("UPDATE active_work_contexts SET assignment_id=$1,assignment_version=$2,revision=revision+1,expires_at=$3,updated_at=$4 WHERE session_id=$5 AND user_id=$6 AND company_id=$7 AND revision=$8").bind(id.as_str()).bind(version).bind(request.session_expires_at).bind(now).bind(request.session_id.as_str()).bind(request.user_id.as_str()).bind(request.company_id.as_str()).bind(revision).execute(&mut *c).await?;
        if updated.rows_affected() != 1 {
            return Err(changed());
        }
        let after = capture(c, scope, now).await?;
        let expected = json!({"session_id":request.session_id.as_str(),"user_id":request.user_id.as_str(),"company_id":request.company_id.as_str(),"assignment_id":id.as_str(),"assignment_version":version,"revision":revision+1,"expires_at":request.session_expires_at,"updated_at":now});
        if after.image.iam != self.captured.image.iam
            || after.image.organization != self.captured.image.organization
            || after.image.selection.as_ref() != Some(&expected)
        {
            return Err(changed());
        }
        let current = CurrentAccountWorkRoles {
            unit: self.unit,
            captured: after,
        };
        current.require_current(c, scope, now).await?;
        Ok(RetainedAccountWorkSelection {
            current,
            choice,
            changed: true,
        })
    }
}
pub struct RetainedAccountWorkSelection {
    changed: bool,
    current: CurrentAccountWorkRoles,
    choice: AccountWorkRoleChoice,
}
impl RetainedAccountWorkSelection {
    pub fn changed(&self) -> bool {
        self.changed
    }
    pub fn current(&self) -> &CurrentAccountWorkRoles {
        &self.current
    }
    pub fn chosen(&self) -> &AccountWorkRoleChoice {
        &self.choice
    }
    pub fn audit_authority(&self) -> Value {
        let a = &self.choice.assignment;
        let s = self
            .current
            .selection()
            .expect("a selected row was retained");
        json!({"role":a.role,"assignmentId":a.id.as_str(),"assignmentVersion":a.version,"companyId":a.company_id.as_str(),"scopeKind":match a.scope_kind{WorkScopeKind::Holding=>"holding",WorkScopeKind::Branch=>"branch",WorkScopeKind::Department=>"department"},"scopeNodeId":a.scope_node_id.as_ref().map(RecordId::as_str),"revision":s.revision})
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        scope: &HoldingAccountWorkScope,
        now: i64,
    ) -> Result<(), Error> {
        self.current.require_current(c, scope, now).await
    }
}
pub async fn read_account_work_roles(
    c: &mut PgConnection,
    scope: &HoldingAccountWorkScope,
    now: i64,
) -> Result<CurrentAccountWorkRoles, Error> {
    Ok(CurrentAccountWorkRoles {
        unit: super::selected_command_authority::selected_unit(c).await?,
        captured: capture(c, scope, now).await?,
    })
}
