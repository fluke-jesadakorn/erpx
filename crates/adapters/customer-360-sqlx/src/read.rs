use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_customer_360::{
    Customer360SectionAvailability, Customer360View, SourceCoverage, SourceFreshness,
    ports::{PortError, ReadProjection, ReadRepository, SourceProjectionMetadata},
};
use erpx_integration_contracts::{
    CurrencyCode, DisplayName, MoneyAmount, ReferenceCode,
    commercial::{PlanRef, SubscriptionFact, SubscriptionRef, SubscriptionStatus},
    crm::{OpportunityFact, OpportunityRef, OpportunityStage},
    customer::{CustomerAccountRef, CustomerProfileFact, CustomerRef, CustomerStatus},
    organization::CompanyRef,
    sales::{SalesOrderFact, SalesOrderLineFact, SalesOrderRef, SalesOrderStatus},
    support::{SupportCaseFact, SupportCaseRef, SupportCaseStatus},
};
use erpx_kernel::AuthorityScope;
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    CONTROL_CUSTOMERS_PROJECTION_NAME, HOLDING_ACTIVITY_PROJECTION_NAME, control_partition,
};

#[derive(Debug, Clone)]
pub struct Customer360ReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
    holding_source: Option<String>,
}

impl Customer360ReadRepository {
    pub fn new(pool: ReadPool, holding_id: Option<Uuid>) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, holding_id, WaiterConfig::default())
    }

    pub fn with_waiter_config(
        pool: ReadPool,
        holding_id: Option<Uuid>,
        config: WaiterConfig,
    ) -> Result<Self, CqrsSqlxError> {
        if holding_id.is_some_and(|value| value.is_nil()) {
            return Err(CqrsSqlxError::InvalidInput("holding id is nil"));
        }
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), config)?,
            pool,
            holding_source: holding_id.map(holding_source_key),
        })
    }

    pub fn holding_source(&self) -> Option<&str> {
        self.holding_source.as_deref()
    }

    async fn metadata(
        &self,
        consistency: &QueryConsistency,
        customer: CustomerRef,
    ) -> Result<(ProjectionMetadata, SourceProjectionMetadata), PortError> {
        let waited = match consistency {
            QueryConsistency::Eventual => None,
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid = match requirement.projection.as_str() {
                    CONTROL_CUSTOMERS_PROJECTION_NAME => {
                        requirement.partition == control_partition(customer)
                    }
                    HOLDING_ACTIVITY_PROJECTION_NAME => {
                        self.holding_source.as_ref().is_none_or(|source| {
                            requirement
                                .partition
                                .strip_prefix(source)
                                .is_some_and(|suffix| suffix.starts_with('/'))
                        }) && requirement.partition.starts_with("holding:")
                    }
                    _ => false,
                };
                if !valid {
                    return Err(PortError::NotReady(ProjectionNotReady {
                        projection: requirement.projection.clone(),
                        partition: requirement.partition.clone(),
                        target: requirement.target,
                        observed: None,
                    }));
                }
                Some(self.waiter.wait_until_checked(requirement).await.map_err(
                    |error| match error {
                        WaitError::NotReady(value) => PortError::NotReady(value),
                        WaitError::Database(_) => PortError::Unavailable,
                    },
                )?)
            }
        };

        let waited_control_checkpoint = waited
            .as_ref()
            .filter(|checkpoint| checkpoint.projection == CONTROL_CUSTOMERS_PROJECTION_NAME)
            .cloned();
        let control_checkpoint = match waited_control_checkpoint {
            Some(checkpoint) => Some(checkpoint),
            None => self
                .read_side
                .checkpoint(
                    CONTROL_CUSTOMERS_PROJECTION_NAME,
                    &control_partition(customer),
                )
                .await
                .map_err(|_| PortError::Unavailable)?,
        };
        let holding_checkpoint = waited
            .as_ref()
            .filter(|checkpoint| checkpoint.projection == HOLDING_ACTIVITY_PROJECTION_NAME)
            .cloned();
        let control = self
            .projection_metadata(CONTROL_CUSTOMERS_PROJECTION_NAME, control_checkpoint, None)
            .await?;
        let holding = self
            .projection_metadata(
                HOLDING_ACTIVITY_PROJECTION_NAME,
                holding_checkpoint,
                self.holding_source.as_deref(),
            )
            .await?;
        let query = match consistency {
            QueryConsistency::ReadYourWrites(requirement)
                if requirement.projection == CONTROL_CUSTOMERS_PROJECTION_NAME =>
            {
                control.clone()
            }
            QueryConsistency::ReadYourWrites(requirement)
                if requirement.projection == HOLDING_ACTIVITY_PROJECTION_NAME =>
            {
                holding.clone()
            }
            QueryConsistency::ReadYourWrites(_) => unreachable!("validated projection name"),
            QueryConsistency::Eventual => conservative_metadata(&control, &holding),
        };
        Ok((
            query,
            SourceProjectionMetadata {
                control_customers: control,
                holding_activity: holding,
            },
        ))
    }

    async fn projection_metadata(
        &self,
        projection: &str,
        checkpoint: Option<ProjectionCheckpoint>,
        source_key: Option<&str>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name=$1 AND status IN ('active','building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(projection)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let observed_at = if let Some(checkpoint) = &checkpoint {
            Some(checkpoint.updated_at)
        } else if projection == HOLDING_ACTIVITY_PROJECTION_NAME {
            sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
                "SELECT min(checkpoint.updated_at)
                 FROM erpx_cqrs_read.projection_source_checkpoints checkpoint
                JOIN erpx_cqrs_read.projection_generations generation
                   ON generation.projection_name=checkpoint.projection_name
                  AND generation.generation=checkpoint.generation
                  AND generation.status='active'
                 WHERE checkpoint.projection_name=$1
                   AND ($2::text IS NULL OR checkpoint.source_key=$2)",
            )
            .bind(projection)
            .bind(source_key)
            .fetch_one(
                &mut *self
                    .pool
                    .begin()
                    .await
                    .map_err(|_| PortError::Unavailable)?,
            )
            .await
            .map_err(|_| PortError::Unavailable)?
        } else {
            None
        };
        let lag_millis = observed_at
            .and_then(|value| Utc::now().signed_duration_since(value).to_std().ok())
            .and_then(|value| u64::try_from(value.as_millis()).ok());
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }

    async fn profile(
        &self,
        customer: CustomerRef,
        scope: &ScopeFilter,
    ) -> Result<Option<CustomerProfileFact>, PortError> {
        let row = sqlx::query(
            "SELECT item.* FROM erpx_customer360_read.customer_profile_projection item
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=item.projection_name
              AND generation.generation=item.generation AND generation.status='active'
             WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.customer_id=$3
               AND ($4 OR EXISTS (
                   SELECT 1
                   FROM erpx_customer_authority_read.authority_link_projection authority_link
                   JOIN erpx_cqrs_read.projection_generations authority_generation
                     ON authority_generation.projection_name=authority_link.projection_name
                    AND authority_generation.generation=authority_link.generation
                    AND authority_generation.status='active'
                   WHERE authority_link.customer_id=item.customer_id
                     AND (authority_link.scope_key=ANY($5::text[])
                          OR authority_link.scope_key LIKE ANY($6::text[]))
               ))",
        )
        .bind(CONTROL_CUSTOMERS_PROJECTION_NAME)
        .bind(control_partition(customer))
        .bind(customer.as_uuid())
        .bind(scope.platform)
        .bind(&scope.exact)
        .bind(&scope.prefixes)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        row.map(decode_profile).transpose()
    }

    async fn subscriptions(
        &self,
        customer: CustomerRef,
        scope: &ScopeFilter,
    ) -> Result<Vec<SubscriptionFact>, PortError> {
        let rows = sqlx::query(
            "SELECT item.* FROM erpx_customer360_read.subscription_projection item
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=item.projection_name
              AND generation.generation=item.generation AND generation.status='active'
             WHERE item.projection_name=$1 AND item.customer_id=$2
               AND ($3::text IS NULL OR item.source_key=$3)
               AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))
             ORDER BY item.effective_at DESC,item.subscription_id DESC",
        )
        .bind(HOLDING_ACTIVITY_PROJECTION_NAME)
        .bind(customer.as_uuid())
        .bind(self.holding_source.as_deref())
        .bind(scope.platform)
        .bind(&scope.exact)
        .bind(&scope.prefixes)
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        rows.into_iter().map(decode_subscription).collect()
    }

    async fn opportunities(
        &self,
        customer: CustomerRef,
        scope: &ScopeFilter,
    ) -> Result<Vec<OpportunityFact>, PortError> {
        let rows = sqlx::query("SELECT item.* FROM erpx_customer360_read.opportunity_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.customer_id=$2 AND ($3::text IS NULL OR item.source_key=$3) AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[])) ORDER BY item.opportunity_id DESC")
            .bind(HOLDING_ACTIVITY_PROJECTION_NAME).bind(customer.as_uuid()).bind(self.holding_source.as_deref()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        rows.into_iter().map(decode_opportunity).collect()
    }

    async fn sales_orders(
        &self,
        customer: CustomerRef,
        scope: &ScopeFilter,
    ) -> Result<Vec<SalesOrderFact>, PortError> {
        let rows = sqlx::query("SELECT item.* FROM erpx_customer360_read.sales_order_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.customer_id=$2 AND ($3::text IS NULL OR item.source_key=$3) AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[])) ORDER BY item.sales_order_id DESC")
            .bind(HOLDING_ACTIVITY_PROJECTION_NAME).bind(customer.as_uuid()).bind(self.holding_source.as_deref()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        rows.into_iter().map(decode_sales_order).collect()
    }

    async fn support_cases(
        &self,
        customer: CustomerRef,
        scope: &ScopeFilter,
    ) -> Result<Vec<SupportCaseFact>, PortError> {
        let rows = sqlx::query("SELECT item.* FROM erpx_customer360_read.support_case_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.customer_id=$2 AND ($3::text IS NULL OR item.source_key=$3) AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[])) ORDER BY item.aggregate_updated_at DESC,item.support_case_id DESC")
            .bind(HOLDING_ACTIVITY_PROJECTION_NAME).bind(customer.as_uuid()).bind(self.holding_source.as_deref()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        rows.into_iter().map(decode_support_case).collect()
    }
}

fn holding_source_key(holding_id: Uuid) -> String {
    format!("holding:{holding_id}")
}

#[async_trait]
impl ReadRepository for Customer360ReadRepository {
    async fn get(
        &self,
        authority: &AuthorityScope,
        customer: CustomerRef,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<Customer360View>>, PortError> {
        authority.validate().map_err(|_| PortError::Denied)?;
        let scope = ScopeFilter::from(authority)?;
        let (query_metadata, sources) = self.metadata(consistency, customer).await?;
        let profile = self.profile(customer, &scope).await?;
        let subscriptions = self.subscriptions(customer, &scope).await?;
        let opportunities = self.opportunities(customer, &scope).await?;
        let sales_orders = self.sales_orders(customer, &scope).await?;
        let support_cases = self.support_cases(customer, &scope).await?;
        // An active projection checkpoint authoritatively covers an empty
        // collection. Row counts cannot distinguish "no facts" from "source
        // not consumed", so coverage follows source freshness instead.
        let control_observed = !matches!(
            sources.control_customers.freshness,
            FreshnessState::Unavailable
        );
        let holding_observed = !matches!(
            sources.holding_activity.freshness,
            FreshnessState::Unavailable
        );
        let coverage = SourceCoverage {
            customer_profile: control_observed,
            commercial_subscriptions: holding_observed,
            crm_opportunities: holding_observed,
            sales_orders: holding_observed,
            support_cases: holding_observed,
        };
        let availability = Customer360SectionAvailability::from_source_freshness(
            coverage,
            domain_freshness(sources.control_customers.freshness),
            domain_freshness(sources.holding_activity.freshness),
        );
        let value = if profile.is_none()
            && subscriptions.is_empty()
            && opportunities.is_empty()
            && sales_orders.is_empty()
            && support_cases.is_empty()
        {
            None
        } else {
            Some(Customer360View {
                customer,
                profile,
                subscriptions,
                opportunities,
                sales_orders,
                support_cases,
                coverage,
                availability,
            })
        };
        if value
            .as_ref()
            .is_some_and(|value| value.customer != customer)
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            query_metadata,
            sources,
        })
    }
}

const fn domain_freshness(value: FreshnessState) -> SourceFreshness {
    match value {
        FreshnessState::Current => SourceFreshness::Current,
        FreshnessState::Stale | FreshnessState::Rebuilding => SourceFreshness::Stale,
        FreshnessState::Unavailable => SourceFreshness::Unavailable,
    }
}

struct ScopeFilter {
    platform: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
}

impl ScopeFilter {
    fn from(scope: &AuthorityScope) -> Result<Self, PortError> {
        let value = match scope {
            AuthorityScope::Platform => Self {
                platform: true,
                exact: vec![],
                prefixes: vec![],
            },
            AuthorityScope::Holding { holding_id } => Self {
                platform: false,
                exact: vec![format!("h:{holding_id}")],
                prefixes: vec![format!("h:{holding_id}/%")],
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => {
                let key = format!("h:{holding_id}/c:{company_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                }
            }
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                platform: false,
                exact: vec![format!("h:{holding_id}/c:{company_id}/b:{branch_id}")],
                prefixes: vec![],
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                platform: false,
                exact: vec![format!("h:{holding_id}/c:{organization_node_id}")],
                prefixes: vec![format!("h:{holding_id}/c:{organization_node_id}/%")],
            },
        };
        Ok(value)
    }
}

fn conservative_metadata(
    left: &ProjectionMetadata,
    right: &ProjectionMetadata,
) -> ProjectionMetadata {
    ProjectionMetadata {
        checkpoint: None,
        freshness: worse_freshness(left.freshness, right.freshness),
        lag_millis: match (left.lag_millis, right.lag_millis) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(v), None) | (None, Some(v)) => Some(v),
            (None, None) => None,
        },
    }
}
fn worse_freshness(left: FreshnessState, right: FreshnessState) -> FreshnessState {
    let rank = |value| match value {
        FreshnessState::Current => 0,
        FreshnessState::Stale => 1,
        FreshnessState::Rebuilding => 2,
        FreshnessState::Unavailable => 3,
    };
    if rank(left) >= rank(right) {
        left
    } else {
        right
    }
}

fn decode_profile(row: PgRow) -> Result<CustomerProfileFact, PortError> {
    Ok(CustomerProfileFact {
        customer: customer_ref(&row, "customer_id")?,
        account: CustomerAccountRef::try_from_uuid(uuid(&row, "customer_account_id")?)
            .map_err(|_| PortError::Internal)?,
        customer_code: code(&row, "customer_code")?,
        display_name: DisplayName::try_new(text(&row, "display_name")?)
            .map_err(|_| PortError::Internal)?,
        owning_company: CompanyRef::try_from_uuid(uuid(&row, "owning_company_id")?)
            .map_err(|_| PortError::Internal)?,
        status: customer_status(&text(&row, "customer_status")?)?,
    })
}
fn decode_subscription(row: PgRow) -> Result<SubscriptionFact, PortError> {
    Ok(SubscriptionFact {
        subscription: SubscriptionRef::try_from_uuid(uuid(&row, "subscription_id")?)
            .map_err(|_| PortError::Internal)?,
        customer: customer_ref(&row, "customer_id")?,
        plan: PlanRef::try_from_uuid(uuid(&row, "plan_id")?).map_err(|_| PortError::Internal)?,
        status: subscription_status(&text(&row, "subscription_status")?)?,
        recurring_amount: money(&row, "recurring_amount_minor", "currency")?,
        effective_at: row
            .try_get("effective_at")
            .map_err(|_| PortError::Internal)?,
    })
}
fn decode_opportunity(row: PgRow) -> Result<OpportunityFact, PortError> {
    Ok(OpportunityFact {
        opportunity: OpportunityRef::try_from_uuid(uuid(&row, "opportunity_id")?)
            .map_err(|_| PortError::Internal)?,
        customer: customer_ref(&row, "customer_id")?,
        stage: opportunity_stage(&text(&row, "opportunity_stage")?)?,
        expected_value: money(&row, "expected_value_minor", "currency")?,
    })
}
fn decode_sales_order(row: PgRow) -> Result<SalesOrderFact, PortError> {
    let lines: serde_json::Value = row.try_get("lines").map_err(|_| PortError::Internal)?;
    let lines: Vec<SalesOrderLineFact> =
        serde_json::from_value(lines).map_err(|_| PortError::Internal)?;
    if lines.is_empty() {
        return Err(PortError::Internal);
    }
    Ok(SalesOrderFact {
        sales_order: SalesOrderRef::try_from_uuid(uuid(&row, "sales_order_id")?)
            .map_err(|_| PortError::Internal)?,
        order_code: code(&row, "order_code")?,
        customer: customer_ref(&row, "customer_id")?,
        status: sales_order_status(&text(&row, "order_status")?)?,
        total: money(&row, "total_minor", "currency")?,
        lines,
    })
}
fn decode_support_case(row: PgRow) -> Result<SupportCaseFact, PortError> {
    Ok(SupportCaseFact {
        support_case: SupportCaseRef::try_from_uuid(uuid(&row, "support_case_id")?)
            .map_err(|_| PortError::Internal)?,
        case_code: code(&row, "case_code")?,
        customer: customer_ref(&row, "customer_id")?,
        status: support_case_status(&text(&row, "case_status")?)?,
        category: code(&row, "category")?,
        updated_at: row
            .try_get("aggregate_updated_at")
            .map_err(|_| PortError::Internal)?,
    })
}
fn uuid(row: &PgRow, field: &str) -> Result<Uuid, PortError> {
    let value: Uuid = row.try_get(field).map_err(|_| PortError::Internal)?;
    if value.is_nil() {
        Err(PortError::Internal)
    } else {
        Ok(value)
    }
}
fn text(row: &PgRow, field: &str) -> Result<String, PortError> {
    row.try_get(field).map_err(|_| PortError::Internal)
}
fn code(row: &PgRow, field: &str) -> Result<ReferenceCode, PortError> {
    ReferenceCode::try_new(text(row, field)?).map_err(|_| PortError::Internal)
}
fn customer_ref(row: &PgRow, field: &str) -> Result<CustomerRef, PortError> {
    CustomerRef::try_from_uuid(uuid(row, field)?).map_err(|_| PortError::Internal)
}
fn money(row: &PgRow, minor: &str, currency: &str) -> Result<MoneyAmount, PortError> {
    let units: i64 = row.try_get(minor).map_err(|_| PortError::Internal)?;
    let currency = CurrencyCode::try_new(&text(row, currency)?).map_err(|_| PortError::Internal)?;
    Ok(MoneyAmount::new(units, currency))
}
fn customer_status(v: &str) -> Result<CustomerStatus, PortError> {
    match v {
        "prospect" => Ok(CustomerStatus::Prospect),
        "active" => Ok(CustomerStatus::Active),
        "dormant" => Ok(CustomerStatus::Dormant),
        "blocked" => Ok(CustomerStatus::Blocked),
        "closed" => Ok(CustomerStatus::Closed),
        _ => Err(PortError::Internal),
    }
}
fn subscription_status(v: &str) -> Result<SubscriptionStatus, PortError> {
    match v {
        "trial" => Ok(SubscriptionStatus::Trial),
        "active" => Ok(SubscriptionStatus::Active),
        "past_due" => Ok(SubscriptionStatus::PastDue),
        "suspended" => Ok(SubscriptionStatus::Suspended),
        "ended" => Ok(SubscriptionStatus::Ended),
        _ => Err(PortError::Internal),
    }
}
fn opportunity_stage(v: &str) -> Result<OpportunityStage, PortError> {
    match v {
        "qualified" => Ok(OpportunityStage::Qualified),
        "discovery" => Ok(OpportunityStage::Discovery),
        "proposal" => Ok(OpportunityStage::Proposal),
        "negotiation" => Ok(OpportunityStage::Negotiation),
        "won" => Ok(OpportunityStage::Won),
        "lost" => Ok(OpportunityStage::Lost),
        _ => Err(PortError::Internal),
    }
}
fn sales_order_status(v: &str) -> Result<SalesOrderStatus, PortError> {
    match v {
        "confirmed" => Ok(SalesOrderStatus::Confirmed),
        "partially_fulfilled" => Ok(SalesOrderStatus::PartiallyFulfilled),
        "fulfilled" => Ok(SalesOrderStatus::Fulfilled),
        "cancelled" => Ok(SalesOrderStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}
fn support_case_status(v: &str) -> Result<SupportCaseStatus, PortError> {
    match v {
        "open" => Ok(SupportCaseStatus::Open),
        "pending_customer" => Ok(SupportCaseStatus::PendingCustomer),
        "pending_internal" => Ok(SupportCaseStatus::PendingInternal),
        "resolved" => Ok(SupportCaseStatus::Resolved),
        "closed" => Ok(SupportCaseStatus::Closed),
        _ => Err(PortError::Internal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_customer_360::SectionAvailability;
    #[test]
    fn eventual_metadata_never_fabricates_a_cross_source_checkpoint() {
        let metadata = |freshness, lag| ProjectionMetadata {
            checkpoint: None,
            freshness,
            lag_millis: lag,
        };
        let value = conservative_metadata(
            &metadata(FreshnessState::Current, Some(3)),
            &metadata(FreshnessState::Rebuilding, Some(8)),
        );
        assert_eq!(value.freshness, FreshnessState::Rebuilding);
        assert_eq!(value.lag_millis, Some(8));
        assert!(value.checkpoint.is_none());
    }
    #[test]
    fn holding_source_is_uuid_qualified() {
        assert_eq!(
            holding_source_key(Uuid::from_u128(7)),
            "holding:00000000-0000-0000-0000-000000000007"
        );
    }
    #[test]
    fn unsupported_sections_remain_unavailable() {
        let coverage = SourceCoverage::default();
        let availability = Customer360SectionAvailability::from(coverage);
        assert_eq!(availability.organization, SectionAvailability::NotProjected);
        assert_eq!(availability.notes, SectionAvailability::NotProjected);
    }

    #[test]
    fn subtree_filter_does_not_expand_selected_branches_to_whole_companies() {
        let holding = Uuid::from_u128(1);
        let company = Uuid::from_u128(2);
        let branch = Uuid::from_u128(3);
        let filter = ScopeFilter::from(&AuthorityScope::Subtree {
            holding_id: holding,
            organization_node_id: branch,
        })
        .unwrap();
        assert_eq!(filter.exact, vec![format!("h:{holding}/c:{branch}")]);
        assert_eq!(filter.prefixes, vec![format!("h:{holding}/c:{branch}/%")]);

        let company_only = ScopeFilter::from(&AuthorityScope::Subtree {
            holding_id: holding,
            organization_node_id: company,
        })
        .unwrap();
        assert_eq!(company_only.exact, vec![format!("h:{holding}/c:{company}")]);
        assert_eq!(
            company_only.prefixes,
            vec![format!("h:{holding}/c:{company}/%")]
        );
    }
}
