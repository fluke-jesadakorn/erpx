//! SQL projection of the versioned catalogue's purchased-capability decision.
//!
//! The command boundary guards and the post-effects decision both build their
//! predicate here, so an enforced assertion cannot silently fork from the
//! catalogue decision used by admission. Expressions only interpolate
//! code-owned aliases; company, capability and clock values stay bound.
use crate::domain::{
    packages::{
        CapabilityPurpose, CommercialCatalogue, CompanyPackageGrant,
        CompanyPackageHistoryRight, PackageDecision, PackageDefinition,
    },
    purchase_wire_revision::purchase_wire_revision,
};
use erpx_server_shared::public::TimestampMillis;
use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};

/// Checked-phase telemetry for the legacy purchase boundary. It carries no
/// grant, record or actor body and never grants work.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseShadowCheckedObservation {
    pub(super) event: &'static str,
    pub(super) phase: &'static str,
    pub(super) company_id: String,
    pub(super) capability: String,
    pub(super) purpose: CapabilityPurpose,
    pub(super) code: &'static str,
    pub(super) purchase_revision: String,
}

/// The shared history source. It matches the retained SQL exactly, including
/// the safe-integer and non-empty event guards.
pub(crate) fn history_source(q: &mut QueryBuilder<Postgres>) {
    q.push("SELECT package_id,package_version FROM public.company_package_grants WHERE company_id=s.company_id AND starts_at<=s.now_ms AND state<>'inactive' UNION SELECT package_id,package_version FROM public.company_package_history_rights WHERE company_id=s.company_id AND retained_at<=s.now_ms AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''");
}

/// `alias` is one of two code-owned constants, never an input identifier.
pub(crate) fn available(
    q: &mut QueryBuilder<Postgres>,
    alias: &'static str,
    purpose: CapabilityPurpose,
) {
    q.push(alias).push(".starts_at<=s.now_ms");
    match purpose {
        CapabilityPurpose::NewWork => {
            q.push(" AND ").push(alias).push(".state='active' AND (").push(alias).push(".ends_at IS NULL OR ").push(alias).push(".ends_at>s.now_ms)");
        }
        CapabilityPurpose::SettleExisting => {
            q.push(" AND ").push(alias).push(".state<>'inactive'");
        }
        CapabilityPurpose::History => unreachable!(),
    }
}

/// Append the boolean predicate over `s.company_id`/`s.now_ms`. The caller
/// owns the materialized `purchase_scope s` CTE and samples its clock.
pub(crate) fn push_capability_predicate(
    q: &mut QueryBuilder<Postgres>,
    catalogue: &CommercialCatalogue,
    capability: &str,
    purpose: CapabilityPurpose,
) {
    let candidates: Vec<&PackageDefinition> = catalogue
        .definitions()
        .iter()
        .filter(|definition| {
            definition
                .visible_capabilities
                .iter()
                .chain(&definition.included_capabilities)
                .any(|value| value == capability)
        })
        .collect();
    if candidates.is_empty() {
        q.push("FALSE");
    }
    for (index, definition) in candidates.iter().enumerate() {
        if index > 0 {
            q.push(" OR ");
        }
        q.push("(");
        if purpose == CapabilityPurpose::History {
            q.push("EXISTS(SELECT 1 FROM (");
            history_source(q);
            q.push(") purchase WHERE purchase.package_id=")
                .push_bind(definition.id.clone())
                .push(" AND purchase.package_version=")
                .push_bind(definition.version)
                .push(")");
        } else {
            q.push("EXISTS(SELECT 1 FROM public.company_package_grants purchase WHERE purchase.company_id=s.company_id AND purchase.package_id=")
                .push_bind(definition.id.clone())
                .push(" AND purchase.package_version=")
                .push_bind(definition.version)
                .push(" AND ");
            available(q, "purchase", purpose);
        }
        for dependency in &definition.dependencies {
            if purpose == CapabilityPurpose::History {
                q.push(" AND EXISTS(SELECT 1 FROM (");
                history_source(q);
                q.push(") parent WHERE parent.package_id=");
            } else {
                q.push(" AND EXISTS(SELECT 1 FROM public.company_package_grants parent WHERE parent.company_id=s.company_id AND parent.package_id=");
            }
            q.push_bind(dependency.clone()).push(" AND parent.package_version IN (");
            let versions: Vec<i64> = catalogue
                .definitions()
                .iter()
                .filter(|parent| &parent.id == dependency)
                .map(|parent| parent.version)
                .collect();
            for (position, version) in versions.iter().enumerate() {
                if position > 0 {
                    q.push(",");
                }
                q.push_bind(*version);
            }
            q.push(")");
            if purpose != CapabilityPurpose::History {
                q.push(" AND ");
                available(q, "parent", purpose);
            }
            q.push(")");
        }
        // Non-history dependencies are nested inside the purchase EXISTS.
        if purpose != CapabilityPurpose::History {
            q.push(")");
        }
        q.push(")");
    }
}

/// Append the retained grant/history JSON projection over `s.company_id`.
pub(crate) fn push_shadow_facts(q: &mut QueryBuilder<Postgres>) {
    q.push(", COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',g.company_id,'packageId',g.package_id,'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,'version',g.version)) FROM public.company_package_grants g WHERE g.company_id=s.company_id),'[]'::jsonb) grants, COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',h.company_id,'packageId',h.package_id,'packageVersion',h.package_version,'sourceGrantVersion',h.source_grant_version,'sourceEventId',h.source_event_id,'retainedAt',h.retained_at)) FROM public.company_package_history_rights h WHERE h.company_id=s.company_id),'[]'::jsonb) history");
}

/// The legacy read path turns JSON BIGINTs into IEEE-754 Numbers before the
/// decision. Retain that rounding boundary so a native decision cannot differ.
pub(crate) fn js_number_grants(grants: &mut [CompanyPackageGrant]) {
    for grant in grants {
        grant.package_version = (grant.package_version as f64) as i64;
        grant.version = (grant.version as f64) as i64;
        grant.starts_at = (grant.starts_at as f64) as i64;
        grant.ends_at = grant.ends_at.map(|value| (value as f64) as i64);
    }
}
pub(crate) fn js_number_history(history: &mut [CompanyPackageHistoryRight]) {
    for right in history {
        right.package_version = (right.package_version as f64) as i64;
        right.source_grant_version = (right.source_grant_version as f64) as i64;
        right.retained_at = (right.retained_at as f64) as i64;
    }
}

/// Recompute the decision and locked-phase observation from the SQL projection.
/// `None` means the projection could not be interpreted; shadow telemetry must
/// never turn that into an owner command failure.
pub(crate) fn decision_and_observation(
    company: &str,
    capability: &str,
    purpose: CapabilityPurpose,
    now: i64,
    catalogue: &CommercialCatalogue,
    grants: serde_json::Value,
    history: serde_json::Value,
) -> Option<(PackageDecision, PurchaseShadowCheckedObservation)> {
    fn string(value: &serde_json::Value, key: &str) -> Option<String> {
        Some(value.get(key)?.as_str()?.to_owned())
    }
    fn number(value: &serde_json::Value, key: &str) -> Option<i64> {
        value.get(key)?.as_i64()
    }
    let grants: Vec<CompanyPackageGrant> = grants
        .as_array()?
        .iter()
        .map(|value| {
            Some(CompanyPackageGrant {
                company_id: string(value, "companyId")?,
                package_id: string(value, "packageId")?,
                package_version: number(value, "packageVersion")?,
                state: string(value, "state")?,
                starts_at: number(value, "startsAt")?,
                ends_at: if value.get("endsAt")?.is_null() {
                    None
                } else {
                    Some(number(value, "endsAt")?)
                },
                version: number(value, "version")?,
                reason: String::new(),
                evidence: String::new(),
            })
        })
        .collect::<Option<_>>()?;
    let history: Vec<CompanyPackageHistoryRight> = history
        .as_array()?
        .iter()
        .map(|value| {
            Some(CompanyPackageHistoryRight {
                company_id: string(value, "companyId")?,
                package_id: string(value, "packageId")?,
                package_version: number(value, "packageVersion")?,
                source_grant_version: number(value, "sourceGrantVersion")?,
                source_event_id: string(value, "sourceEventId")?,
                retained_at: number(value, "retainedAt")?,
            })
        })
        .collect::<Option<_>>()?;
    let revision = purchase_wire_revision(company, &grants, &history, TimestampMillis::new(now)).ok()?;
    let mut decision_grants = grants;
    js_number_grants(&mut decision_grants);
    let mut decision_history = history;
    js_number_history(&mut decision_history);
    let decision = catalogue.purchased_capability(
        &decision_grants,
        company,
        capability,
        purpose,
        (now as f64) as i64,
        &decision_history,
    );
    Some((
        decision,
        PurchaseShadowCheckedObservation {
            event: "commercial.purchase.shadow",
            phase: "checked",
            company_id: company.to_owned(),
            capability: capability.to_owned(),
            purpose,
            code: decision.code.as_code(),
            purchase_revision: revision,
        },
    ))
}
