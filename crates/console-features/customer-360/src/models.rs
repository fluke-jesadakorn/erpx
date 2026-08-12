//! Target-neutral Customer 360 UI models.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoneyDisplay {
    pub minor_units: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionDisplayAvailability {
    Ready,
    AwaitingSource,
    NotProjected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionFreshnessDisplay {
    Current,
    Stale,
    Rebuilding,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProjectionDisplay {
    pub freshness: ProjectionFreshnessDisplay,
    pub lag_millis: Option<u64>,
    pub checkpoint_sequence: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer360SourceProjectionsDisplay {
    pub control_customers: SourceProjectionDisplay,
    pub tenant_activity: SourceProjectionDisplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Customer360CoverageDisplay {
    pub customer_profile: bool,
    pub commercial_subscriptions: bool,
    pub crm_opportunities: bool,
    pub sales_orders: bool,
    pub support_cases: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Customer360AvailabilityDisplay {
    pub overview: SectionDisplayAvailability,
    pub commercial: SectionDisplayAvailability,
    pub relationship_activity: SectionDisplayAvailability,
    pub sales_activity: SectionDisplayAvailability,
    pub support: SectionDisplayAvailability,
    pub organization: SectionDisplayAvailability,
    pub contacts: SectionDisplayAvailability,
    pub entitlements: SectionDisplayAvailability,
    pub onboarding: SectionDisplayAvailability,
    pub health: SectionDisplayAvailability,
    pub access_audit: SectionDisplayAvailability,
    pub notes: SectionDisplayAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerProfileDisplay {
    pub account_id: String,
    pub customer_code: String,
    pub display_name: String,
    pub owning_company_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionDisplay {
    pub subscription_id: String,
    pub plan_id: String,
    pub status: String,
    pub recurring_amount: MoneyDisplay,
    pub effective_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpportunityDisplay {
    pub opportunity_id: String,
    pub stage: String,
    pub expected_value: MoneyDisplay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesOrderLineDisplay {
    pub line_id: String,
    pub item_code: String,
    pub quantity_units: i64,
    pub quantity_scale: u8,
    pub line_value: MoneyDisplay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesOrderDisplay {
    pub sales_order_id: String,
    pub order_code: String,
    pub status: String,
    pub total: MoneyDisplay,
    pub lines: Vec<SalesOrderLineDisplay>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportCaseDisplay {
    pub support_case_id: String,
    pub case_code: String,
    pub status: String,
    pub category: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer360Display {
    pub customer_id: String,
    pub profile: Option<CustomerProfileDisplay>,
    pub subscriptions: Vec<SubscriptionDisplay>,
    pub opportunities: Vec<OpportunityDisplay>,
    pub sales_orders: Vec<SalesOrderDisplay>,
    pub support_cases: Vec<SupportCaseDisplay>,
    pub coverage: Customer360CoverageDisplay,
    pub availability: Customer360AvailabilityDisplay,
    pub projections: Customer360SourceProjectionsDisplay,
}
