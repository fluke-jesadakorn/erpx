//! Bind a registered Assets read plan on the caller's own transaction.
//!
//! The adapter only maps caller scope facts onto the exact registered SQL and
//! its ordered bind positions. It never opens a connection, commits, locks, or
//! changes a live TypeScript reader; authority stays with the retained caller.
use crate::public::native_read::{self, BindField, ReadPlan};
use erpx_server_shared::public::BusinessError;

/// Scope facts the live caller already resolved from its WorkContext.
#[derive(Clone, Debug, Default)]
pub struct ReadScope<'a> {
    /// Public Field Service Asset identifier (not a typed `RecordId`).
    pub asset_id: Option<&'a str>,
    pub company_id: Option<&'a str>,
    /// `packageId === 'assets'` from the retained package-obligation caller.
    pub is_base: Option<bool>,
}

/// A plan ready for the caller to run on its existing SQL snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlan {
    pub kind: &'static str,
    pub sql: &'static str,
    pub binds: Vec<String>,
}

fn scope_required(field: BindField) -> BusinessError {
    let message = match field {
        BindField::AssetId => "The Assets read needs the Field Service Asset id.",
        BindField::CompanyId => "The Assets read needs the current Company.",
        BindField::IsBase => "The Assets read needs the package obligation scope.",
    };
    BusinessError::invalid("ASSETS_READ_SCOPE_REQUIRED", message)
}

/// Bind `kind` using `scope`, preserving the exact registered SQL and order.
pub fn bind(kind: &str, scope: &ReadScope<'_>) -> Result<BoundPlan, BusinessError> {
    let plan: &'static ReadPlan = native_read::plan(kind).ok_or_else(native_read::invalid)?;
    let mut binds = Vec::with_capacity(plan.binds.len());
    for &field in plan.binds {
        let value = match field {
            BindField::AssetId => scope.asset_id.map(str::to_owned),
            BindField::CompanyId => scope.company_id.map(str::to_owned),
            BindField::IsBase => scope.is_base.map(|base| base.to_string()),
        };
        let value = value.ok_or_else(|| scope_required(field))?;
        binds.push(value);
    }
    Ok(BoundPlan {
        kind: plan.kind,
        sql: plan.sql,
        binds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public::native_read as kinds;

    fn full_scope<'a>() -> ReadScope<'a> {
        ReadScope {
            asset_id: Some("asset-1"),
            company_id: Some("company-1"),
            is_base: Some(true),
        }
    }

    #[test]
    fn every_registered_kind_binds_without_losing_its_plan() {
        for kind in kinds::kinds() {
            let bound = bind(kind, &full_scope()).expect("bindable plan");
            let plan = kinds::plan(kind).unwrap();
            assert_eq!(bound.kind, kind);
            assert_eq!(bound.sql, plan.sql);
            assert_eq!(bound.binds.len(), bound.sql.matches('?').count(), "{kind}");
        }
    }

    #[test]
    fn obligation_binds_preserve_company_and_boolean_order() {
        let bound = bind(kinds::RD_OBLIGATIONS_BASE, &full_scope()).unwrap();
        assert_eq!(bound.binds, vec!["company-1".to_owned(), "true".to_owned()]);
        let scoped = ReadScope {
            is_base: Some(false),
            ..full_scope()
        };
        assert_eq!(
            bind(kinds::RD_OBLIGATIONS_PREVENTIVE, &scoped)
                .unwrap()
                .binds,
            vec!["company-1".to_owned(), "false".to_owned()]
        );
    }

    #[test]
    fn required_scope_and_unknown_kind_are_validation_errors() {
        assert_eq!(
            bind("bogus.read", &full_scope()).unwrap_err().code,
            "ASSETS_READ_PLAN_INVALID"
        );
        assert_eq!(
            bind(kinds::RD_FS_ASSET_ATTACHMENT_IMAGE, &ReadScope::default())
                .unwrap_err()
                .code,
            "ASSETS_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(kinds::RD_OBLIGATIONS_BASE, &ReadScope::default())
                .unwrap_err()
                .code,
            "ASSETS_READ_SCOPE_REQUIRED"
        );
    }
}
