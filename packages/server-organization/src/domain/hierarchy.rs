use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, RecordId};
use serde::Deserialize;
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Company,
    Headquarters,
    Branch,
    Department,
    Team,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Active,
    Suspended,
    Archived,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct OrganizationNode {
    pub id: RecordId,
    pub kind: NodeKind,
    pub slug: String,
    pub display_name: String,
    pub parent_id: Option<RecordId>,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub status: NodeStatus,
    pub hierarchy_revision: i64,
}
#[derive(Clone, Debug)]
pub struct HierarchyLineage {
    pub company: OrganizationNode,
    pub branch: Option<OrganizationNode>,
    pub department: Option<OrganizationNode>,
}
/// Facts do not grant account, module or work-role authority.
#[derive(Clone, Debug)]
pub struct CompanyContext {
    holding_id: HoldingId,
    lineage: HierarchyLineage,
}
impl CompanyContext {
    pub fn holding_id(&self) -> &HoldingId {
        &self.holding_id
    }
    pub fn company(&self) -> &OrganizationNode {
        &self.lineage.company
    }
    pub fn branch(&self) -> Option<&OrganizationNode> {
        self.lineage.branch.as_ref()
    }
    pub fn department(&self) -> Option<&OrganizationNode> {
        self.lineage.department.as_ref()
    }
}
pub fn workspace_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "WORKSPACE_NOT_FOUND",
        "Workspace not found.",
    )
}
fn current(node: &OrganizationNode) -> bool {
    node.status == NodeStatus::Active && node.hierarchy_revision > 0
}
pub fn company_context(
    holding_id: HoldingId,
    lineage: HierarchyLineage,
) -> Result<CompanyContext, BusinessError> {
    let company = &lineage.company;
    if company.kind != NodeKind::Company
        || !current(company)
        || company.id.as_str() != company.company_id.as_str()
        || company.parent_id.is_some()
        || company.branch_id.is_some()
    {
        return Err(workspace_missing());
    }
    if let Some(branch) = &lineage.branch
        && (!current(branch)
            || branch.kind != NodeKind::Branch
            || branch.company_id != company.company_id
            || branch.parent_id.as_ref() != Some(&company.id)
            || branch.branch_id.as_ref().is_some_and(|id| id != &branch.id))
    {
        return Err(workspace_missing());
    }
    if let Some(department) = &lineage.department {
        let parent = lineage
            .branch
            .as_ref()
            .map(|b| &b.id)
            .unwrap_or(&company.id);
        if !current(department)
            || department.kind != NodeKind::Department
            || department.company_id != company.company_id
            || department.parent_id.as_ref() != Some(parent)
            || department.branch_id.as_ref() != lineage.branch.as_ref().map(|b| &b.id)
        {
            return Err(workspace_missing());
        }
    }
    Ok(CompanyContext {
        holding_id,
        lineage,
    })
}
#[derive(Clone, Debug)]
pub struct WorkspaceRouteQuery {
    branch_slug: Option<String>,
    department_slug: Option<String>,
    company: Option<String>,
}
impl WorkspaceRouteQuery {
    pub fn new(
        branch_slug: Option<String>,
        department_slug: Option<String>,
        company: Option<String>,
    ) -> Result<Self, BusinessError> {
        for slug in [branch_slug.as_deref(), department_slug.as_deref()]
            .into_iter()
            .flatten()
        {
            if slug.is_empty()
                || slug.len() > 80
                || slug.starts_with('-')
                || slug.ends_with('-')
                || slug.contains("--")
                || !slug
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            {
                return Err(workspace_missing());
            }
        }
        if let Some(company) = &company {
            CompanyId::new(company.clone())?;
        }
        Ok(Self {
            branch_slug,
            department_slug,
            company,
        })
    }
    pub fn matches(&self, lineage: &HierarchyLineage) -> bool {
        self.company.as_ref().is_none_or(|value| {
            value == lineage.company.id.as_str() || value == &lineage.company.slug
        }) && self.branch_slug.as_deref() == lineage.branch.as_ref().map(|node| node.slug.as_str())
            && self.department_slug.as_deref()
                == lineage.department.as_ref().map(|node| node.slug.as_str())
    }
    pub fn branch_slug(&self) -> Option<&str> {
        self.branch_slug.as_deref()
    }
    pub fn department_slug(&self) -> Option<&str> {
        self.department_slug.as_deref()
    }
    pub fn company(&self) -> Option<&str> {
        self.company.as_deref()
    }
    pub fn is_empty(&self) -> bool {
        self.branch_slug.is_none() && self.department_slug.is_none() && self.company.is_none()
    }
}
