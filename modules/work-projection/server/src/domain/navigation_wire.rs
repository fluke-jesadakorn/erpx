//! Owner-side projection of the destination-link normalization the retained
//! Next route applies with `canonicalDestinationLinks`.
//!
//! The browser contract is `{data: <snapshot>}`; only fields whose key matches
//! `href`, `*Href` or `*_href` are rewritten. API URLs and already-canonical
//! workspace/absolute URLs are passed through. Legacy relative source paths
//! are mapped to their registered collection/detail path. Presentation fields
//! and command payloads are never touched.

use serde_json::{Map, Value};

/// Legacy source prefixes and their registered workspace path. This is the
/// subset of `routePrefixes` needed by native work attention hrefs; the full
/// `canonicalUiHref` record routing remains a composition presentation fact.
const ROUTE_PREFIXES: &[(&str, &str)] = &[
    ("manage-erpx/billing/usage", "erpx-management/usage"),
    ("manage-erpx/iam", "erpx-management/iam"),
    ("manage-erpx", "erpx-management"),
    ("finance/group-editions", "finance/reports/group-editions"),
    ("finance/hospitality-settings", "finance/setup/hospitality-settings"),
    ("finance/book-adjustments", "finance/accounting/book-adjustments"),
    ("finance/supplier-batches", "finance/cash/supplier-batches"),
    ("finance/accounting", "finance/accounting/journals"),
    ("finance/thai-documents", "finance/setup/thai-documents"),
    ("finance/hotel-documents", "finance/accounting/hotels/documents"),
    ("finance/hotel-accounts", "finance/accounting/hotels/accounts"),
    ("finance/statement-packages", "finance/reports/statement-packages"),
    ("finance/invoices", "finance/documents/invoices"),
    ("finance/payments", "finance/cash/payments"),
    ("finance/refunds", "finance/cash/refunds"),
    ("finance/collections", "finance/cash/collections"),
    ("finance/restaurants", "finance/accounting/restaurants"),
    ("finance/hotels", "finance/accounting/hotels"),
    ("finance/engines", "finance/accounting/engines"),
    ("finance/assets", "finance/accounting/assets"),
    ("finance/reports", "finance/reports/company"),
    ("finance/sales", "finance/documents/sales"),
    ("finance/contracts", "finance/documents/contracts"),
    ("finance/due-drafts", "finance/documents/due-drafts"),
    ("finance/hospitality", "finance/setup/hospitality-settings"),
    ("finance/journal", "finance/accounting/journals"),
    ("finance", "finance/today"),
    ("people/settings", "people-organization/people/hr-settings"),
    ("people/payslips", "people-organization/people/payslips"),
    ("people/organization", "people-organization/people/organization"),
    ("people/clearance", "people-organization/people/clearance"),
    ("people/hire-handoffs", "people-organization/people/hire-handoffs"),
    ("people/evaluation", "people-organization/people/evaluation"),
    ("people/directory", "people-organization/people/directory"),
    ("people/payroll", "people-organization/people/payroll"),
    ("people/roster", "people-organization/people/roster"),
    ("people/access", "people-organization/people/users-access"),
    ("people/move", "people-organization/people/move"),
    ("people", "people-organization/people/directory"),
    ("payroll", "people-organization/people/payroll"),
    ("talent", "people-organization/talent"),
    ("benefits", "people-organization/benefits"),
    ("customer-followups", "customers-sales/sales/followups"),
    ("customer-duplicates", "customers-sales/customers/reviews/duplicates"),
    ("customer-identity", "customers-sales/customers/reviews/identity"),
    ("customer-access", "customers-sales/customers/reviews/access"),
    ("support/ticket-portal", "customers-sales/customers/service/portal"),
    ("support/incidents", "customers-sales/customers/service/incidents"),
    ("support/sessions", "customers-sales/customers/service/sessions"),
    ("support/sla", "customers-sales/customers/service/sla"),
    ("support", "customers-sales/customers/service/cases"),
    ("customers", "customers-sales/customers/directory"),
    ("marketing", "customers-sales/marketing"),
    ("ecommerce", "customers-sales/ecommerce"),
    ("crm", "customers-sales/sales/pipeline"),
    ("sales", "customers-sales/sales/pipeline"),
    ("procurement-rfqs", "purchasing/rfqs"),
    ("supplier-bills", "purchasing/supplier-bills"),
    ("landed-costs", "purchasing/landed-costs"),
    ("stock-counts", "inventory-quality/counts"),
    ("stock-transfers", "inventory-quality/transfers"),
    ("stock-transit", "inventory-quality/transit"),
    ("maintenance/plans", "services/assets/plans"),
    ("maintenance", "services/assets/maintenance"),
    ("field-service", "services/field-service"),
    ("quality", "inventory-quality/quality"),
    ("routine-templates", "projects-routines/routines"),
    ("staff-tasks", "projects-routines/tasks"),
    ("projects", "projects-routines/projects"),
    ("operations", "projects-routines/operations"),
    ("requests", "services/desk"),
    ("assets", "services/assets/register"),
    ("legal", "services/legal"),
    ("planning", "production/planning"),
    ("manufacturing", "production/manufacturing"),
    ("hospitality/setup", "pms/setup"),
    ("handbook", "knowledge/handbook/published"),
    ("knowledge", "knowledge/handbook/published"),
    ("learning", "knowledge/learning/assignments"),
    ("future-features", "knowledge/roadmap"),
    ("executive", "insights/executive"),
    ("technology", "administration/technology"),
    ("settings", "administration"),
    ("agents", "administration/agents"),
    ("usage", "administration/usage"),
    ("feed", "home"),
];

fn word_chars(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// `^(?:href|\w*Href|\w*_href)$`, restricted to ASCII word characters.
fn is_destination_key(key: &str) -> bool {
    key == "href"
        || key
            .strip_suffix("Href")
            .is_some_and(|prefix| prefix.is_empty() || word_chars(prefix))
        || key
            .strip_suffix("_href")
            .is_some_and(|prefix| prefix.is_empty() || word_chars(prefix))
}

/// Mirrors `canonicalDestinationLink` for native owner hrefs.
pub fn canonical_destination_link(href: &str) -> String {
    if href.starts_with("/api/") {
        return href.to_owned();
    }
    if href.starts_with("/holdings/")
        || href.starts_with("/platform")
        || href.starts_with("http://")
        || href.starts_with("https://")
    {
        return href.to_owned();
    }
    let rest = href.strip_prefix('/').unwrap_or(href);
    let source = rest.split(['?', '#']).next().unwrap_or("");
    let matched = ROUTE_PREFIXES
        .iter()
        .filter(|(prefix, _)| source == *prefix || source.starts_with(&format!("{prefix}/")))
        .max_by_key(|(prefix, _)| prefix.len());
    let Some((prefix, path)) = matched else {
        // Workspace roots (`work`, `calendar`, ...) and unknown paths are
        // already canonical relative paths; `canonicalUiHref` leaves them.
        return href.to_owned();
    };
    let tail = &rest[prefix.len()..];
    format!("/holdings/__route_scope/{path}{tail}")
        .strip_prefix("/holdings/__route_scope")
        .unwrap_or_default()
        .to_owned()
}

/// Recursive `canonicalDestinationLinks` projection. Only destination fields
/// change; every other value keeps its exact wire representation.
pub fn canonical_destination_links(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                canonical_destination_links(item);
            }
        }
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if is_destination_key(key)
                    && let Value::String(href) = item
                {
                    *href = canonical_destination_link(href);
                } else {
                    canonical_destination_links(item);
                }
            }
        }
        _ => {}
    }
}

/// Applies the projection to a `{data: ...}` response envelope.
pub fn canonical_envelope(data: Value) -> Value {
    let mut value = Value::Object(Map::from_iter([("data".to_owned(), data)]));
    canonical_destination_links(&mut value);
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn destination_key_matches_the_ts_regex() {
        for key in ["href", "primaryHref", "details_href", "xHref", "Href", "_href"] {
            assert!(is_destination_key(key), "{key}");
        }
        for key in ["hrefValue", "referral", "other_href_value"] {
            assert!(!is_destination_key(key), "{key}");
        }
    }

    #[test]
    fn api_absolute_and_holding_links_are_untouched() {
        assert_eq!(canonical_destination_link("/api/v1/holdings/acme/navigation"), "/api/v1/holdings/acme/navigation");
        assert_eq!(canonical_destination_link("/holdings/acme/work?view=actions"), "/holdings/acme/work?view=actions");
        assert_eq!(canonical_destination_link("https://acme.jesadakorn.com/work"), "https://acme.jesadakorn.com/work");
    }

    #[test]
    fn legacy_relative_hrefs_are_mapped_and_suffix_kept() {
        assert_eq!(canonical_destination_link("/support/incidents/incident-1"), "/customers-sales/customers/service/incidents/incident-1");
        assert_eq!(canonical_destination_link("/maintenance/plans/plan-1?view=week"), "/services/assets/plans/plan-1?view=week");
        assert_eq!(canonical_destination_link("/work/routing"), "/work/routing");
    }

    #[test]
    fn only_destination_fields_change_recursively() {
        let value = json!({
            "data": {
                "attention": {"items": [{"href": "/support/ticket-1", "title": "Ticket", "secondaryHref": "/quality/q-1"}]},
                "api": "/api/v1/holdings/acme/navigation",
                "workViewAvailability": {"team": true}
            }
        });
        let mut value = value;
        canonical_destination_links(&mut value);
        assert_eq!(value["data"]["attention"]["items"][0]["href"], "/customers-sales/customers/service/cases/ticket-1");
        assert_eq!(value["data"]["attention"]["items"][0]["title"], "Ticket");
        assert_eq!(value["data"]["attention"]["items"][0]["secondaryHref"], "/inventory-quality/quality/q-1");
        assert_eq!(value["data"]["api"], "/api/v1/holdings/acme/navigation");
        assert_eq!(value["data"]["workViewAvailability"]["team"], true);
    }

    #[test]
    fn envelope_has_exactly_the_data_key() {
        assert_eq!(canonical_envelope(json!({"home": 2})), json!({"data": {"home": 2}}));
    }
}
