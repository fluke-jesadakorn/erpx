//! Exact Sales eligibility for a retained CRM activity recipient source.
//!
//! This is a source decision only. The coordinator must separately retain the
//! selected actor, purchase, Workflow and notification evidence, and prepare
//! every owner lock before the first transaction snapshot.
use erpx_server_crm::public::activity_source::CurrentCrmActivityRecipientSource;
use erpx_server_identity_access::public::{
    effective_authority::require_effective_authority_sources_held,
    permission_catalog_lock::require_permission_catalog_lock,
    sales_recipient::{CurrentSalesAssignmentJoin, CurrentSalesIamBoundary},
};
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

// Mirrors salesRecipientEligibility('l','l.owner_id'). Keep the joins in SQL:
// splitting them into independent Rust string comparisons changes column
// collation and can select different assignment/People rows.
const DECISION: &str = r#" SELECT EXISTS(
 SELECT 1 FROM crm_activity_recipient_source l
 JOIN sales_recipient_assignments a ON a.company_id=l.company_id AND a.user_id=l.owner_id
 JOIN sales_recipient_people p ON p.user_id=a.user_id AND p.company_id=a.company_id
 JOIN sales_recipient_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
 WHERE iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read')
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=l.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=l.department_id))
 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
 AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')
 AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
) eligible"#;

pub struct CurrentCrmSalesRecipientEligibility<'a> {
    crm: &'a CurrentCrmActivityRecipientSource,
    identity: &'a CurrentSalesAssignmentJoin,
    iam: &'a CurrentSalesIamBoundary,
    people: &'a CurrentSalesWorkforceJoin,
    people_authority: &'a CurrentPeopleSalesAuthorityBoundary,
    organization: &'a CurrentOrganizationSalesAuthorityBoundary,
    management: &'a CurrentBetaPolicyAuthorityBoundary,
    eligible: bool,
}
impl CurrentCrmSalesRecipientEligibility<'_> {
    pub fn is_eligible(&self) -> bool {
        self.eligible
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if evaluate(
            c,
            self.crm,
            self.identity,
            self.iam,
            self.people,
            self.people_authority,
            self.organization,
            self.management,
        )
        .await?
            != self.eligible
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "CRM_SALES_RECIPIENT_CHANGED",
                "The Sales recipient source changed.",
            )
            .into());
        }
        Ok(())
    }
}

pub async fn inspect_crm_sales_recipient_eligibility<'a>(
    c: &mut PgConnection,
    crm: &'a CurrentCrmActivityRecipientSource,
    identity: &'a CurrentSalesAssignmentJoin,
    iam: &'a CurrentSalesIamBoundary,
    people: &'a CurrentSalesWorkforceJoin,
    people_authority: &'a CurrentPeopleSalesAuthorityBoundary,
    organization: &'a CurrentOrganizationSalesAuthorityBoundary,
    management: &'a CurrentBetaPolicyAuthorityBoundary,
) -> Result<CurrentCrmSalesRecipientEligibility<'a>, Error> {
    let eligible = evaluate(
        c,
        crm,
        identity,
        iam,
        people,
        people_authority,
        organization,
        management,
    )
    .await?;
    Ok(CurrentCrmSalesRecipientEligibility {
        crm,
        identity,
        iam,
        people,
        people_authority,
        organization,
        management,
        eligible,
    })
}

async fn evaluate(
    c: &mut PgConnection,
    crm: &CurrentCrmActivityRecipientSource,
    identity: &CurrentSalesAssignmentJoin,
    iam: &CurrentSalesIamBoundary,
    people: &CurrentSalesWorkforceJoin,
    people_authority: &CurrentPeopleSalesAuthorityBoundary,
    organization: &CurrentOrganizationSalesAuthorityBoundary,
    management: &CurrentBetaPolicyAuthorityBoundary,
) -> Result<bool, Error> {
    crm.require_current(&mut *c).await?;
    identity.require_current(&mut *c).await?;
    iam.require_current(&mut *c).await?;
    people.require_current(&mut *c).await?;
    people_authority.require_current(&mut *c).await?;
    organization.require_current(&mut *c).await?;
    management.require_current(&mut *c).await?;
    require_effective_authority_sources_held(&mut *c).await?;
    require_permission_catalog_lock(&mut *c).await?;
    let sql = format!(
        "WITH {}, {}, {}{}",
        crm.cte_sql(),
        identity.cte_sql(),
        people.cte_sql(),
        DECISION
    );
    let row = crm
        .bind_source(sqlx::query(AssertSqlSafe(sql)))
        .fetch_one(&mut *c)
        .await?;
    let eligible: bool = row.try_get("eligible")?;
    crm.require_current(&mut *c).await?;
    identity.require_current(&mut *c).await?;
    iam.require_current(&mut *c).await?;
    people.require_current(&mut *c).await?;
    people_authority.require_current(&mut *c).await?;
    organization.require_current(&mut *c).await?;
    management.require_current(&mut *c).await?;
    require_effective_authority_sources_held(&mut *c).await?;
    require_permission_catalog_lock(c).await?;
    Ok(eligible)
}
