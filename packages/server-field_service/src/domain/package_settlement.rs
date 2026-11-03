//! Service v1 settlement decisions. Commercial supplies the reviewed package
//! scope; owning adapters supply current source facts without UI visibility
//! filters. These decisions confer no package-management authority.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceSettlementPackage {
    Base,
    PartsAssets,
}

pub struct AgreementSettlementFacts<'a> {
    pub id: &'a str,
    pub version: i64,
    pub state: &'a str,
    pub asset: bool,
    pub source_valid: bool,
    pub terminal_event: bool,
}

pub struct VisitSettlementFacts<'a> {
    pub id: &'a str,
    pub version: i64,
    pub state: &'a str,
    pub source_valid: bool,
    pub asset: bool,
    pub agreement_asset: bool,
    pub reservation: bool,
    pub consumption: bool,
    pub outstanding_reservation: bool,
    pub terminal_event: bool,
    pub billing_lineage: bool,
}

/// The original observation hashes these five fields in this order. Keep the
/// physical identifiers internal; consumers publish only count/revision/scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ServiceSettlementFact {
    pub kind: &'static str,
    pub id: String,
    pub version: i64,
    pub state: String,
    pub pending: bool,
}

pub fn agreement_settlement_fact(
    package: ServiceSettlementPackage,
    facts: AgreementSettlementFacts<'_>,
) -> Option<ServiceSettlementFact> {
    if package == ServiceSettlementPackage::PartsAssets && !facts.asset {
        return None;
    }
    Some(ServiceSettlementFact {
        kind: "agreement",
        id: facts.id.into(),
        version: facts.version,
        state: facts.state.into(),
        pending: !facts.source_valid
            || !matches!(facts.state, "retired" | "rejected")
            || !facts.terminal_event,
    })
}

pub fn visit_settlement_fact(
    package: ServiceSettlementPackage,
    facts: VisitSettlementFacts<'_>,
) -> Option<ServiceSettlementFact> {
    let base = package == ServiceSettlementPackage::Base;
    if !base
        && !facts.asset
        && !facts.agreement_asset
        && !facts.reservation
        && !facts.consumption
    {
        return None;
    }
    // An old, fully discharged spare does not force unrelated live base work
    // to close. Invalid lineage always remains outstanding, even after cancel.
    let live_duty = base || facts.asset || facts.agreement_asset || facts.outstanding_reservation;
    Some(ServiceSettlementFact {
        kind: "visit",
        id: facts.id.into(),
        version: facts.version,
        state: facts.state.into(),
        pending: !facts.source_valid
            || live_duty
                && (!matches!(facts.state, "cancelled" | "billed")
                    || !facts.terminal_event
                    || facts.state == "billed" && !facts.billing_lineage),
    })
}
