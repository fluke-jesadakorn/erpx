//! Native Ecommerce executive metric domain: the retired TS SELECT family.
//!
//! Three aggregate shapes (open carts, completed checkouts, return/fulfillment
//! links) plus the two branch/department organisation label fragments are
//! rendered here as the exact `?`-placeholder statement the TS reader used to
//! build. Rust owns the text, the binds and the read-only guarantee; only
//! scalar rows and the trusted statement are returned to the owner TS client.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Closed projection copied from the retained organisation source contract.
pub const ORG_NODES_SOURCE: &str =
    "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes";

/// The NULL-tolerant Company/Branch/Department scope, byte-identical to the
/// retired `scopedAlias("c")` TS fragment.
pub const SCOPED_ALIAS: &str =
    "c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)";

pub const MAX_ROWS: usize = 1000;
pub const MAX_RANGE_MS: i64 = 366 * 24 * 60 * 60 * 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Metric {
    OpenCarts,
    CompletedCheckouts,
    ReturnLinks,
    FulfillmentLinks,
}

impl Metric {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open-ecommerce-carts" => Some(Self::OpenCarts),
            "completed-ecommerce-checkouts" => Some(Self::CompletedCheckouts),
            "ecommerce-return-links" => Some(Self::ReturnLinks),
            "ecommerce-fulfillment-links" => Some(Self::FulfillmentLinks),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenCarts => "open-ecommerce-carts",
            Self::CompletedCheckouts => "completed-ecommerce-checkouts",
            Self::ReturnLinks => "ecommerce-return-links",
            Self::FulfillmentLinks => "ecommerce-fulfillment-links",
        }
    }

    fn scope_alias(self) -> &'static str {
        match self {
            Self::OpenCarts => "c",
            Self::CompletedCheckouts => "x",
            Self::ReturnLinks => "r",
            Self::FulfillmentLinks => "l",
        }
    }

    fn relation(self) -> &'static str {
        match self {
            Self::OpenCarts => "ecommerce_carts c",
            Self::CompletedCheckouts => "ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id",
            Self::ReturnLinks => "ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id",
            Self::FulfillmentLinks => "ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id",
        }
    }

    fn extra(self) -> &'static str {
        match self {
            Self::OpenCarts => "c.status='open'",
            Self::CompletedCheckouts => "x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<?",
            Self::ReturnLinks => "r.created_at>=? AND r.created_at<?",
            Self::FulfillmentLinks => "l.created_at>=? AND l.created_at<?",
        }
    }

    fn time_bounded(self) -> bool {
        !matches!(self, Self::OpenCarts)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Breakdown {
    Status,
    Branch,
    Department,
    Month,
}

impl Breakdown {
    /// The retired adapter sent every unrecognised breakdown to the month arm.
    pub fn parse(value: &str) -> Self {
        match value {
            "status" => Self::Status,
            "branch" => Self::Branch,
            "department" => Self::Department,
            _ => Self::Month,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Scope<'a> {
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindValue {
    Text(Option<String>),
    Int(i64),
}

#[derive(Clone, Debug)]
pub struct QuerySpec {
    pub sql: String,
    pub values: Vec<BindValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricRow {
    pub label: String,
    pub value: String,
}

fn invalid(message: &'static str) -> BusinessError {
    BusinessError::invalid("ECOMMERCE_EXEC_METRIC_REQUEST_INVALID", message)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn validate_scope(scope: &Scope<'_>) -> Result<(), BusinessError> {
    if !valid_id(scope.company_id)
        || scope.branch_id.is_some_and(|id| !valid_id(id))
        || scope.department_id.is_some_and(|id| !valid_id(id))
    {
        return Err(invalid("Use one bounded Ecommerce metric Company scope."));
    }
    Ok(())
}

pub fn validate_range(from_at: i64, to_exclusive: i64) -> Result<(), BusinessError> {
    if from_at <= 0 || to_exclusive <= from_at || to_exclusive - from_at > MAX_RANGE_MS {
        return Err(invalid("Use one bounded Ecommerce metric date range."));
    }
    Ok(())
}

/// The exact retired `nodeNameLabel` text for one organisation column.
pub fn node_name_label(column: &str, fallback: &str) -> String {
    format!(
        "COALESCE((SELECT n.display_name FROM ({ORG_NODES_SOURCE}) n WHERE n.id=c.{column} AND n.company_id=c.company_id),'{fallback}')"
    )
}

pub fn branch_name_label() -> String {
    node_name_label("branch_id", "Branch")
}

pub fn department_name_label() -> String {
    node_name_label("department_id", "Unassigned")
}

fn label(metric: Metric, breakdown: Breakdown) -> String {
    match breakdown {
        Breakdown::Status => format!("{}.status", metric.scope_alias()),
        Breakdown::Branch => branch_name_label(),
        Breakdown::Department => department_name_label(),
        Breakdown::Month => format!(
            "to_char(to_timestamp({}.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')",
            metric.scope_alias()
        ),
    }
}

/// Rebuild the retired `countBy` statement with positional `?` binds in the
/// original order: Company, Branch twice, Department twice, then the two
/// exclusive date bounds on the time-bounded aggregates.
pub fn build_query(
    metric: Metric,
    breakdown: Breakdown,
    scope: &Scope<'_>,
    from_at: i64,
    to_exclusive: i64,
) -> QuerySpec {
    let sql = format!(
        "SELECT {} label,count(*)::text value FROM {} WHERE {} AND {} GROUP BY 1 ORDER BY 1",
        label(metric, breakdown),
        metric.relation(),
        SCOPED_ALIAS,
        metric.extra()
    );
    let mut values = vec![
        BindValue::Text(Some(scope.company_id.to_owned())),
        BindValue::Text(scope.branch_id.map(str::to_owned)),
        BindValue::Text(scope.branch_id.map(str::to_owned)),
        BindValue::Text(scope.department_id.map(str::to_owned)),
        BindValue::Text(scope.department_id.map(str::to_owned)),
    ];
    if metric.time_bounded() {
        values.push(BindValue::Int(from_at));
        values.push(BindValue::Int(to_exclusive));
    }
    QuerySpec { sql, values }
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_EXEC_METRIC_UNAVAILABLE",
        "The native Ecommerce executive metric service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FROM: i64 = 1_767_225_600_000; // 2026-01-01T00:00:00Z
    const TO: i64 = 1_769_904_000_000; // 2026-02-01T00:00:00Z

    fn scope() -> Scope<'static> {
        Scope {
            company_id: "c",
            branch_id: None,
            department_id: None,
        }
    }

    #[test]
    fn retired_count_sql_is_byte_identical() {
        let s = scope();
        let scope_sql = SCOPED_ALIAS;
        assert_eq!(
            build_query(Metric::OpenCarts, Breakdown::Status, &s, FROM, TO).sql,
            format!("SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE {scope_sql} AND c.status='open' GROUP BY 1 ORDER BY 1")
        );
        assert_eq!(
            build_query(Metric::CompletedCheckouts, Breakdown::Status, &s, FROM, TO).sql,
            format!("SELECT x.status label,count(*)::text value FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<? GROUP BY 1 ORDER BY 1")
        );
        assert_eq!(
            build_query(Metric::ReturnLinks, Breakdown::Status, &s, FROM, TO).sql,
            format!("SELECT r.status label,count(*)::text value FROM ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND r.created_at>=? AND r.created_at<? GROUP BY 1 ORDER BY 1")
        );
        assert_eq!(
            build_query(Metric::FulfillmentLinks, Breakdown::Status, &s, FROM, TO).sql,
            format!("SELECT l.status label,count(*)::text value FROM ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND l.created_at>=? AND l.created_at<? GROUP BY 1 ORDER BY 1")
        );
    }

    #[test]
    fn retired_organisation_labels_are_byte_identical() {
        assert_eq!(
            branch_name_label(),
            format!("COALESCE((SELECT n.display_name FROM ({ORG_NODES_SOURCE}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')")
        );
        assert_eq!(
            department_name_label(),
            format!("COALESCE((SELECT n.display_name FROM ({ORG_NODES_SOURCE}) n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')")
        );
        let s = scope();
        for metric in [Metric::OpenCarts, Metric::CompletedCheckouts, Metric::ReturnLinks, Metric::FulfillmentLinks] {
            assert!(build_query(metric, Breakdown::Branch, &s, FROM, TO).sql.contains(&branch_name_label()));
            assert!(build_query(metric, Breakdown::Department, &s, FROM, TO).sql.contains(&department_name_label()));
            let month = build_query(metric, Breakdown::Month, &s, FROM, TO).sql;
            assert!(month.contains(&format!("to_char(to_timestamp({}.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')", metric.scope_alias())));
        }
    }

    #[test]
    fn binds_match_the_placeholders_and_stay_read_only() {
        let s = Scope { company_id: "c", branch_id: Some("b"), department_id: None };
        for metric in [Metric::OpenCarts, Metric::CompletedCheckouts, Metric::ReturnLinks, Metric::FulfillmentLinks] {
            for breakdown in [Breakdown::Status, Breakdown::Branch, Breakdown::Department, Breakdown::Month] {
                let spec = build_query(metric, breakdown, &s, FROM, TO);
                assert_eq!(spec.sql.matches('?').count(), spec.values.len());
                assert!(spec.sql.starts_with("SELECT "));
                assert!(!spec.sql.contains(';'));
                assert!(!spec.sql.to_uppercase().contains("INSERT")
                    && !spec.sql.to_uppercase().contains("UPDATE ")
                    && !spec.sql.to_uppercase().contains("DELETE")
                    && !spec.sql.to_uppercase().contains("FOR UPDATE")
                    && !spec.sql.to_uppercase().contains("LOCK TABLE"));
            }
        }
        let carts = build_query(Metric::OpenCarts, Breakdown::Status, &s, FROM, TO);
        assert_eq!(carts.values, vec![
            BindValue::Text(Some("c".into())),
            BindValue::Text(Some("b".into())),
            BindValue::Text(Some("b".into())),
            BindValue::Text(None),
            BindValue::Text(None),
        ]);
        let checkouts = build_query(Metric::CompletedCheckouts, Breakdown::Status, &s, FROM, TO);
        assert_eq!(checkouts.values.last(), Some(&BindValue::Int(TO)));
    }

    #[test]
    fn scope_and_range_validation_fail_closed() {
        assert_err(validate_scope(&Scope { company_id: "", branch_id: None, department_id: None }));
        assert_err(validate_scope(&Scope { company_id: "c", branch_id: Some("bad id"), department_id: None }));
        assert!(validate_scope(&scope()).is_ok());
        assert_err(validate_range(0, TO));
        assert_err(validate_range(TO, TO));
        assert_err(validate_range(FROM, FROM + MAX_RANGE_MS + 1));
        assert!(validate_range(FROM, TO).is_ok());
    }

    fn assert_err(result: Result<(), BusinessError>) {
        let error = result.unwrap_err();
        assert_eq!(error.code, "ECOMMERCE_EXEC_METRIC_REQUEST_INVALID");
    }
}
