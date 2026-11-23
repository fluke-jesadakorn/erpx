//! Portable Supply package-obligation read rules for the Inventory owner.
//!
//! The caller supplies the already-authorized Company scope and the owning
//! serializable transaction; this module only validates the read scope and
//! shapes the report. All SQL lives in the adapter; authority, command ledger,
//! Finance journals and Files evidence stay at the composition boundary. It
//! never writes by itself.
use erpx_server_shared::public::BusinessError;
use sha2::{Digest, Sha256};

/// Supply packages the Inventory owner participates in.
pub fn valid_package(package: &str) -> bool {
    matches!(package, "supply" | "supply-quality" | "supply-advanced")
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Re-check the caller scope inside the owner boundary. The commercial wrapper
/// already selected the Company/package, but the owner read must not trust a
/// caller label for the module, version or clock.
pub fn require_supply_inventory_scope(
    company: &str,
    package: &str,
    version: i64,
    module: &str,
    now: i64,
) -> Result<(), BusinessError> {
    if company.is_empty() || company.len() > 200 {
        return Err(invalid(
            "SUPPLY_INVENTORY_PACKAGE_OBLIGATION_SCOPE_REQUIRED",
            "Use the selected Company scope.",
        ));
    }
    if module != "inventory"
        || !valid_package(package)
        || version != 1
        || now < 0
        || (now as u64) > (i64::MAX as u64)
    {
        return Err(invalid(
            "SUPPLY_INVENTORY_PACKAGE_OBLIGATION_SCOPE_REQUIRED",
            "Use the Supply inventory obligation scope.",
        ));
    }
    Ok(())
}

/// One outstanding-or-settled owner fact. `fact_json` is the Postgres
/// `fact::text` image; `pending` mirrors the TS `pending !== false` gate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplyObligationFact {
    pub kind: String,
    pub id: String,
    pub fact_json: String,
    pub pending: bool,
}

/// Deterministic owner revision over the observed facts. The hash covers the
/// company, package version and every fact image in `ORDER BY kind,id`, so any
/// added, removed or changed fact advances the revision.
pub fn supply_obligation_revision(
    company: &str,
    package_version: i64,
    facts: &[SupplyObligationFact],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(company.as_bytes());
    hasher.update([0u8]);
    hasher.update(package_version.to_string().as_bytes());
    for fact in facts {
        hasher.update([0u8]);
        hasher.update(fact.kind.as_bytes());
        hasher.update([0u8]);
        hasher.update(fact.id.as_bytes());
        hasher.update([0u8]);
        hasher.update(fact.fact_json.as_bytes());
        hasher.update([0u8]);
        hasher.update(if fact.pending { [1u8] } else { [0u8] });
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Count the facts that remain obligations (`pending !== false` in TS).
pub fn supply_obligation_outstanding(facts: &[SupplyObligationFact]) -> usize {
    facts.iter().filter(|fact| fact.pending).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(company: &str, package: &str, version: i64, module: &str, now: i64) -> bool {
        require_supply_inventory_scope(company, package, version, module, now).is_ok()
    }

    #[test]
    fn rejects_any_scope_mismatch() {
        assert!(scope("company", "supply", 1, "inventory", 0));
        assert!(scope("company", "supply-quality", 1, "inventory", 1));
        assert!(scope("company", "supply-advanced", 1, "inventory", 9_007_199_254_740_991));
        assert!(!scope("", "supply", 1, "inventory", 0));
        assert!(!scope("company", "sales", 1, "inventory", 0));
        assert!(!scope("company", "supply", 2, "inventory", 0));
        assert!(!scope("company", "supply", 1, "quality", 0));
        assert!(!scope("company", "supply", 1, "inventory", -1));
    }

    #[test]
    fn revision_is_deterministic_and_fact_sensitive() {
        let facts = vec![SupplyObligationFact {
            kind: "invalid-stock".into(),
            id: "w:p".into(),
            fact_json: "{\"quantity_milli\":-1}".into(),
            pending: true,
        }];
        let first = supply_obligation_revision("company", 1, &facts);
        assert_eq!(first.len(), 64);
        assert_eq!(first, supply_obligation_revision("company", 1, &facts));
        let settled = vec![SupplyObligationFact {
            pending: false,
            ..facts[0].clone()
        }];
        assert_ne!(first, supply_obligation_revision("company", 1, &settled));
        assert_ne!(
            first,
            supply_obligation_revision("other", 1, &facts)
        );
        assert_eq!(supply_obligation_outstanding(&facts), 1);
        assert_eq!(supply_obligation_outstanding(&settled), 0);
    }
}
