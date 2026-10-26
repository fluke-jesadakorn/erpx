use crate::{
    application::hierarchy::{OrganizationReadFuture, OrganizationReadPort},
    domain::hierarchy::{HierarchyLineage, OrganizationNode, WorkspaceRouteQuery},
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use sqlx::{PgConnection, Row, postgres::PgRow};
/// Borrows the caller's leased connection; never opens, commits or releases a transaction.
pub struct PgOrganizationReader<'c> {
    holding_id: HoldingId,
    connection: &'c mut PgConnection,
}
impl<'c> PgOrganizationReader<'c> {
    pub fn new(holding_id: HoldingId, connection: &'c mut PgConnection) -> Self {
        Self {
            holding_id,
            connection,
        }
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_READ_UNAVAILABLE",
        "Organization facts are unavailable.",
    )
}
fn node(value: serde_json::Value) -> Result<OrganizationNode, PgTransactionError<BusinessError>> {
    serde_json::from_value(value).map_err(|_| unavailable().into())
}
fn lineage(row: PgRow) -> Result<HierarchyLineage, PgTransactionError<BusinessError>> {
    let company = node(row.try_get("company")?)?;
    let branch: Option<serde_json::Value> = row.try_get("branch")?;
    let department: Option<serde_json::Value> = row.try_get("department")?;
    Ok(HierarchyLineage {
        company,
        branch: branch.map(node).transpose()?,
        department: department.map(node).transpose()?,
    })
}
impl OrganizationReadPort for PgOrganizationReader<'_> {
    type Error = PgTransactionError<BusinessError>;
    fn holding_id(&self) -> &HoldingId {
        &self.holding_id
    }
    fn workspace_candidates<'a>(
        &'a mut self,
        query: &'a WorkspaceRouteQuery,
    ) -> OrganizationReadFuture<'a, Vec<HierarchyLineage>, Self::Error> {
        Box::pin(async move {
            let rows=sqlx::query("SELECT to_jsonb(c) company,to_jsonb(b) branch,to_jsonb(d) department FROM organization_nodes c
          LEFT JOIN organization_nodes b ON $1::text IS NOT NULL AND b.company_id=c.id AND b.parent_id=c.id AND b.kind='branch' AND b.status='active' AND b.slug=$1
          LEFT JOIN organization_nodes d ON $2::text IS NOT NULL AND d.company_id=c.id AND d.kind='department' AND d.status='active' AND d.slug=$2
           AND (($1::text IS NOT NULL AND d.branch_id=b.id AND d.parent_id=b.id) OR ($1::text IS NULL AND d.branch_id IS NULL AND d.parent_id=c.id))
          WHERE c.kind='company' AND c.status='active' AND ($3::text IS NULL OR c.slug=$3 OR c.id=$3)
           AND ($1::text IS NULL OR b.id IS NOT NULL) AND ($2::text IS NULL OR d.id IS NOT NULL)
          ORDER BY c.id LIMIT 2")
          .bind(query.branch_slug()).bind(query.department_slug()).bind(query.company()).fetch_all(&mut *self.connection).await?;
            rows.into_iter().map(lineage).collect()
        })
    }
    fn company<'a>(
        &'a mut self,
        id: &'a CompanyId,
    ) -> OrganizationReadFuture<'a, Option<HierarchyLineage>, Self::Error> {
        Box::pin(async move {
            sqlx::query("SELECT to_jsonb(c) company,NULL::jsonb branch,NULL::jsonb department FROM organization_nodes c WHERE c.id=$1 AND c.kind='company' AND c.status='active'")
          .bind(id.as_str()).fetch_optional(&mut *self.connection).await?.map(lineage).transpose()
        })
    }
}

/// The owning request keeps this lock until all selected-grant reads and owner effects finish.
pub async fn lock_hierarchy_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    sqlx::query("LOCK TABLE organization_nodes IN SHARE MODE")
        .execute(connection)
        .await?;
    Ok(())
}
