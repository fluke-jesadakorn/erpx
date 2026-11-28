//! Query port and owner handler for the native Legal rule-impact read.
//!
//! No SQL lives here. The handler owns the authority gate, the rule-kind
//! invariant and the complete response projection. Purchase and Company-enabled
//! decisions arrive as a composition-supplied witness and are re-checked against
//! the owner's own `legal_settings` in the same source; they are never granted
//! by a request path, query string or broad role.

use std::{future::Future, pin::Pin};

use erpx_platform_shared_server::public::{BusinessError, FailureKind};

use super::domain::{NOTICE, RuleHead, RuleImpactItem, RuleImpactRead};

pub type RuleImpactFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition-supplied authority for one rule-impact read. `None` never
/// happens: the HTTP adapter fails closed when either header is absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuleImpactAuthority {
    /// Legal is enabled for the selected Company.
    pub company_enabled: bool,
    /// The rule's `legal.obligations` capability is purchased or retained by
    /// history for this viewer.
    pub rule_purchased: bool,
}

/// The owner-local facts behind one rule-impact read. Composition injects the
/// Pg implementation; there is no permissive or default reader.
pub trait RuleImpactSource: Send + Sync {
    /// Current, Company-scoped `legal_settings.enabled`.
    fn enabled<'a>(&'a self, company: &'a str) -> RuleImpactFuture<'a, bool>;
    /// Bounded point witness for the selected rule.
    fn rule<'a>(&'a self, company: &'a str, id: &'a str) -> RuleImpactFuture<'a, Option<RuleHead>>;
    /// Company/branch/department/member-visible linked records, ordered by id.
    fn impacts<'a>(
        &'a self,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        user: &'a str,
        id: &'a str,
    ) -> RuleImpactFuture<'a, Vec<RuleImpactItem>>;
}

fn denied(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

/// Enforce the same purchase/enabled decisions the retained TypeScript gate
/// applied, with the owner re-checking `legal_settings` itself.
pub async fn authorize(
    source: &dyn RuleImpactSource,
    authority: RuleImpactAuthority,
    company: &str,
) -> Result<(), BusinessError> {
    if !authority.company_enabled || !source.enabled(company).await? {
        return Err(denied(
            "LEGAL_DISABLED",
            "Legal is not enabled for this Company.",
        ));
    }
    if !authority.rule_purchased {
        return Err(denied(
            "PACKAGE_REQUIRED",
            "This Legal source requires its purchased capability or retained history.",
        ));
    }
    Ok(())
}

/// Owner handler. The query order is fixed: authority first, then the rule
/// witness, then the bounded impact set. Every fact comes from the same injected
/// source, so a reconfigured source cannot mix Company scopes.
pub async fn read_rule_impact(
    source: &dyn RuleImpactSource,
    authority: RuleImpactAuthority,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    user: &str,
    id: &str,
) -> Result<RuleImpactRead, BusinessError> {
    authorize(source, authority, company).await?;
    let rule = source
        .rule(company, id)
        .await?
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "LEGAL_NOT_FOUND", "Rule unavailable."))?;
    if rule.kind != "rule" {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "LEGAL_REQUEST_INVALID",
            "Choose a jurisdiction rule.",
        ));
    }
    let items = source.impacts(company, branch, department, user, id).await?;
    Ok(RuleImpactRead {
        rule_id: id.to_owned(),
        rule_version: rule.version,
        items,
        notice: NOTICE.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        enabled: bool,
        rule: Option<RuleHead>,
        items: Vec<RuleImpactItem>,
    }

    impl RuleImpactSource for Fake {
        fn enabled<'a>(&'a self, company: &'a str) -> RuleImpactFuture<'a, bool> {
            let calls = &self.calls;
            let enabled = self.enabled;
            Box::pin(async move {
                calls.lock().unwrap().push(format!("enabled:{company}"));
                Ok(enabled)
            })
        }
        fn rule<'a>(&'a self, company: &'a str, id: &'a str) -> RuleImpactFuture<'a, Option<RuleHead>> {
            let calls = &self.calls;
            let rule = &self.rule;
            Box::pin(async move {
                calls.lock().unwrap().push(format!("rule:{company}:{id}"));
                Ok(rule.clone())
            })
        }
        fn impacts<'a>(
            &'a self,
            company: &'a str,
            branch: Option<&'a str>,
            department: Option<&'a str>,
            user: &'a str,
            id: &'a str,
        ) -> RuleImpactFuture<'a, Vec<RuleImpactItem>> {
            let calls = &self.calls;
            let items = &self.items;
            Box::pin(async move {
                calls.lock().unwrap().push(format!(
                    "impacts:{company}:{:?}:{:?}:{user}:{id}",
                    branch, department
                ));
                Ok(items.clone())
            })
        }
    }

    fn authority() -> RuleImpactAuthority {
        RuleImpactAuthority {
            company_enabled: true,
            rule_purchased: true,
        }
    }

    #[tokio::test]
    async fn handler_binds_company_scope_and_returns_complete_payload() {
        let fake = Fake {
            enabled: true,
            rule: Some(RuleHead {
                kind: "rule".into(),
                version: 4,
            }),
            items: vec![RuleImpactItem {
                id: "item-1".into(),
                kind: "contract".into(),
                title: "Agreement".into(),
                state: "draft".into(),
                due_date: Some("2026-12-01".into()),
                version: 2,
            }],
            ..Default::default()
        };
        let read = read_rule_impact(
            &fake,
            authority(),
            "company-1",
            Some("branch-1"),
            None,
            "user-1",
            "rule-1",
        )
        .await
        .expect("read");
        assert_eq!(read.rule_id, "rule-1");
        assert_eq!(read.rule_version, 4);
        assert_eq!(read.items.len(), 1);
        assert_eq!(read.items[0].id, "item-1");
        assert_eq!(read.notice, NOTICE);
        let calls = fake.calls.lock().unwrap();
        assert!(calls.contains(&"enabled:company-1".to_owned()));
        assert!(calls.contains(&"rule:company-1:rule-1".to_owned()));
        assert!(calls.contains(&"impacts:company-1:Some(\"branch-1\"):None:user-1:rule-1".to_owned()));
    }

    #[tokio::test]
    async fn authority_failures_close_before_any_rule_read() {
        let fake = Fake {
            enabled: true,
            rule: Some(RuleHead {
                kind: "rule".into(),
                version: 1,
            }),
            ..Default::default()
        };
        let missing_purchase = RuleImpactAuthority {
            company_enabled: true,
            rule_purchased: false,
        };
        let error = read_rule_impact(&fake, missing_purchase, "company-1", None, None, "u", "r")
            .await
            .unwrap_err();
        assert_eq!(error.code, "PACKAGE_REQUIRED");
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert!(!fake.calls.lock().unwrap().iter().any(|c| c.starts_with("rule:")));

        let fake = Fake {
            enabled: false,
            ..Default::default()
        };
        let error = read_rule_impact(&fake, authority(), "company-1", None, None, "u", "r")
            .await
            .unwrap_err();
        assert_eq!(error.code, "LEGAL_DISABLED");
    }

    #[tokio::test]
    async fn non_rule_records_are_rejected() {
        let fake = Fake {
            enabled: true,
            rule: Some(RuleHead {
                kind: "contract".into(),
                version: 1,
            }),
            ..Default::default()
        };
        let error = read_rule_impact(&fake, authority(), "company-1", None, None, "u", "r")
            .await
            .unwrap_err();
        assert_eq!(error.code, "LEGAL_REQUEST_INVALID");
        assert!(!fake.calls.lock().unwrap().iter().any(|c| c.starts_with("impacts:")));
    }
}
