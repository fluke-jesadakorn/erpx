//! Source-only Maintenance purchase image. This does not grant purchase or actor authority.
//! The caller must prepare all owner locks before the first snapshot and recheck
//! this image after effects, together with Commercial and selected actor guards.
use erpx_server_assets::public::asset_purchase_source::CurrentAssetMaintenanceJoin;
use erpx_server_identity_access::public::{
    effective_authority::require_effective_authority_sources_held,
    permission_catalog_lock::require_permission_catalog_lock,
    sales_recipient::{CurrentSalesAssignmentJoin, CurrentSalesIamBoundary},
};
use erpx_server_maintenance::public::asset_purchase_record::CurrentMaintenanceAssetPurchaseRecord;
use erpx_server_manage_erpx::public::CurrentBetaPolicyAuthorityBoundary;
use erpx_server_organization::public::CurrentOrganizationSalesAuthorityBoundary;
use erpx_server_people_payroll::public::{
    identity_facts::CurrentPeopleSalesAuthorityBoundary,
    workforce_reference::CurrentSalesWorkforceJoin,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{AssertSqlSafe, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
const IMAGE: &str = r#" SELECT jsonb_build_object('record',CASE WHEN $5::bool THEN to_jsonb(r)-'version'-'updated_at' ELSE to_jsonb(r) END,'asset',to_jsonb(a),'ownerEligible',EXISTS(SELECT 1 FROM sales_recipient_assignments assignment JOIN sales_recipient_people person ON person.user_id=assignment.user_id AND person.company_id=assignment.company_id JOIN sales_recipient_engagements engagement ON engagement.person_id=person.id AND engagement.company_id=person.company_id WHERE assignment.company_id=r.company_id AND assignment.user_id=r.owner_id AND iam_assignment_active(assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(assignment.user_id,assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(assignment.role,assignment.company_id,'records.write') AND (assignment.scope_kind='holding' OR assignment.scope_kind='branch' AND assignment.scope_node_id=r.branch_id OR assignment.scope_kind='department' AND assignment.scope_node_id=r.department_id) AND person.status='active' AND person.merged_into_id IS NULL AND engagement.status='active' AND engagement.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (engagement.end_date IS NULL OR engagement.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))))::text AS image FROM maintenance_asset_purchase_source r CROSS JOIN asset_maintenance_source a"#;
pub struct MaintenanceAssetPurchaseSources<'a> {
    pub maintenance: &'a CurrentMaintenanceAssetPurchaseRecord<'a>,
    pub asset: &'a CurrentAssetMaintenanceJoin,
    pub identity: &'a CurrentSalesAssignmentJoin,
    pub iam: &'a CurrentSalesIamBoundary,
    pub people: &'a CurrentSalesWorkforceJoin,
    pub people_authority: &'a CurrentPeopleSalesAuthorityBoundary,
    pub organization: &'a CurrentOrganizationSalesAuthorityBoundary,
    pub management: &'a CurrentBetaPolicyAuthorityBoundary,
}
impl MaintenanceAssetPurchaseSources<'_> {
    async fn require_owners(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.maintenance.require_current(&mut *c).await?;
        self.asset.require_current(&mut *c).await?;
        self.identity.require_current(&mut *c).await?;
        self.iam.require_current(&mut *c).await?;
        self.people.require_current(&mut *c).await?;
        self.people_authority.require_current(&mut *c).await?;
        self.organization.require_current(&mut *c).await?;
        self.management.require_current(&mut *c).await?;
        require_effective_authority_sources_held(&mut *c).await?;
        require_permission_catalog_lock(c).await
    }
    async fn evaluate(&self, c: &mut PgConnection) -> Result<Option<String>, Error> {
        self.require_owners(&mut *c).await?;
        let sql = format!(
            "WITH {}, {}, {}, {}{}",
            self.maintenance.cte_sql(),
            self.asset.cte_sql(),
            self.identity.cte_sql(),
            self.people.cte_sql(),
            IMAGE
        );
        let row = self
            .maintenance
            .bind_source(sqlx::query(AssertSqlSafe(sql)))
            .bind(self.maintenance.file_mode())
            .fetch_optional(&mut *c)
            .await?;
        let image = row.map(|r| r.try_get("image")).transpose()?;
        self.require_owners(c).await?;
        Ok(image)
    }
    pub async fn inspect<'a>(
        &'a self,
        c: &mut PgConnection,
    ) -> Result<CurrentMaintenanceAssetPurchaseImage<'a, 'a>, Error> {
        Ok(CurrentMaintenanceAssetPurchaseImage {
            image: self.evaluate(c).await?,
            sources: self,
        })
    }
}
pub struct CurrentMaintenanceAssetPurchaseImage<'a, 's> {
    sources: &'a MaintenanceAssetPurchaseSources<'s>,
    image: Option<String>,
}
impl CurrentMaintenanceAssetPurchaseImage<'_, '_> {
    pub fn image(&self) -> Option<&str> {
        self.image.as_deref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if self.sources.evaluate(c).await? != self.image {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "MAINTENANCE_ASSET_PURCHASE_IMAGE_CHANGED",
                "The Maintenance purchase source changed.",
            )
            .into());
        }
        Ok(())
    }
}
