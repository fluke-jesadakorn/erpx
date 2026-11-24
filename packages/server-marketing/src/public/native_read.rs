//! Owner-local native read plans for the Marketing SELECT point-read tail
//! (`G4 / migrate.marketing.read-ports`).
//!
//! A plan carries the byte-exact SQL of a retained TypeScript point read, its
//! ordered binds and the authority facts inlined by that read. This is a
//! registry, not a consumer cutover: no route is mounted, no connection is
//! opened and no TypeScript reader is changed here. The caller runs a bound
//! plan on its own transaction, so the live TypeScript read stays authoritative
//! until G4 retires it. Candidate row locks, Files/Finance/Cross-owner purchase
//! witnesses and composition HTTP mounts are separate retained-authority
//! concerns and are deliberately not recreated.
use erpx_server_shared::public::BusinessError;

/// Operation id a future native owner-read transport will advertise.
pub const OWNER_READ_OPERATION: &str = "marketing.owner-read.v1";

// marketing_assets point reads.
pub const RD_ASSET_POINT: &str = "marketing-asset-point";
pub const RD_ASSET_LIST: &str = "marketing-asset-list";
pub const RD_FORM_SOURCE: &str = "active-form-source";
pub const RD_FORM_PUBLIC: &str = "active-form-public";
pub const RD_FORM_CONTENT_EXISTS: &str = "form-content-exists";
pub const RD_FORM_CONTENT_ROW: &str = "form-content-row";

// marketing_campaigns / marketing_campaign_costs fact reads.
pub const RD_CAMPAIGN_EXISTS: &str = "campaign-exists";
pub const RD_CAMPAIGN_WITNESS: &str = "campaign-witness";
pub const RD_CAMPAIGN_COST_WITNESS: &str = "campaign-cost-witness";

// sales-marketing settlement package-obligations cross-owner read.
pub const RD_PACKAGE_OBLIGATIONS: &str = "marketing-package-obligations";

/// A bind position in a planned statement, in wire order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindField {
    AssetId,
    CompanyId,
    /// Active-form / form-content version; bound as a PG integer.
    Version,
    CampaignId,
}

/// Scope facts a kind needs from its caller before its plan can be bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopeRequirement {
    pub asset: bool,
    pub company: bool,
    pub version: bool,
    pub campaign: bool,
}

/// One caller-unit read plan: exact SQL, ordered binds and the retained
/// authority facts that must survive a later cutover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadPlan {
    pub kind: &'static str,
    pub caller: &'static str,
    pub sql: &'static str,
    pub binds: &'static [BindField],
    pub scope: ScopeRequirement,
    /// Cross-owner fragments inlined verbatim by the TypeScript authority.
    /// These point reads are owner-local, so every plan has an empty set.
    pub cross_owner: &'static [&'static str],
    /// Source tables the retained TypeScript caller still reads.
    pub authority: &'static str,
}

// Verbatim statements emitted by the retained TypeScript helpers. Keeping them
// as literals lets a parity test check them against the live source and run
// both sides on one transaction.
const PLAN_ASSET_POINT: &str = "SELECT * FROM marketing_assets WHERE id=? AND company_id=?";
const PLAN_ASSET_LIST: &str =
    "SELECT * FROM marketing_assets WHERE company_id=? ORDER BY updated_at DESC";
const PLAN_FORM_SOURCE: &str = "SELECT configuration_json FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND kind='form' AND status='active'";
const PLAN_FORM_PUBLIC: &str = "SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'";
const PLAN_FORM_CONTENT_EXISTS: &str = "SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?";
const PLAN_FORM_CONTENT_ROW: &str = "SELECT id,version,preview_json FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?";
const PLAN_CAMPAIGN_EXISTS: &str = "SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?";
const PLAN_CAMPAIGN_WITNESS: &str = "SELECT campaign.id,campaign.company_id,NULL::text branch_id,NULL::text department_id,to_jsonb(campaign) witness FROM marketing_campaigns campaign";
const PLAN_CAMPAIGN_COST_WITNESS: &str = "SELECT cost.id,cost.company_id,NULL::text branch_id,NULL::text department_id,to_jsonb(cost) witness FROM marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id";

// Verbatim cross-owner fragments from `crm-purchase-sources.ts` and
// `package-obligations.ts`. Macros (not named consts) let `concat!` compose the
// byte-exact statement the TypeScript authority executed before retirement.
macro_rules! crm_candidates {
    () => {
        concat!(
            "SELECT candidate.* FROM marketing_lead_candidates candidate\n JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id AND form.kind='form' AND form.version>=candidate.form_version\n JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version AND captured.action='activate' AND captured.status='active'\n JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'\n WHERE length(trim(candidate.consent_evidence))>=3"
        )
    };
}
macro_rules! candidate_sources {
    () => {
        concat!(
            "SELECT c.*,valid.id IS NOT NULL AND EXISTS(SELECT 1 FROM marketing_assets form JOIN marketing_asset_events activated ON activated.asset_id=form.id AND activated.version=c.form_version AND activated.action='activate' AND activated.actor_id<>form.created_by JOIN command_receipts command ON activated.id='marketing-asset-event-'||command.command_id AND command.operation='marketing.asset.activate' AND command.record_id=form.id AND command.actor_id=activated.actor_id AND command.authority_json::jsonb->>'companyId'=c.company_id WHERE form.id=c.form_id AND form.company_id=c.company_id) source_valid FROM marketing_lead_candidates c LEFT JOIN (",
            crm_candidates!(),
            ") valid ON valid.id=c.id AND valid.company_id=c.company_id"
        )
    };
}
macro_rules! package_obligations {
    () => {
        concat!(
            "WITH scope AS(SELECT ?::text company_id)\n SELECT 'asset' kind,a.id,to_jsonb(a) fact,a.status IN ('draft','active') OR NOT EXISTS(SELECT 1 FROM marketing_asset_events event WHERE event.asset_id=a.id AND event.version<=a.version AND event.status=a.status AND event.action=CASE a.status WHEN 'retired' THEN 'retire' WHEN 'rejected' THEN 'reject' END AND event.actor_id=a.reviewed_by AND (a.status<>'rejected' OR event.actor_id<>a.created_by) AND EXISTS(SELECT 1 FROM command_receipts command WHERE event.id='marketing-asset-event-'||command.command_id AND command.record_id=a.id AND command.operation='marketing.asset.'||event.action AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=a.company_id AND command.result_json::jsonb->>'status'=a.status AND command.result_json::jsonb->>'version'=event.version::text)) pending FROM marketing_assets a,scope WHERE a.company_id=scope.company_id\n UNION ALL SELECT 'provider-callback-unresolved',a.id,to_jsonb(a),true FROM marketing_assets a,scope WHERE a.company_id=scope.company_id AND a.kind IN ('provider','domain') AND EXISTS(SELECT 1 FROM marketing_asset_events event WHERE event.asset_id=a.id AND event.action='activate')\n UNION ALL SELECT 'candidate',c.id,to_jsonb(c),NOT c.source_valid OR c.status<>'handed_off' FROM (",
            candidate_sources!(),
            ") c,scope WHERE c.company_id=scope.company_id\n UNION ALL SELECT 'campaign',c.id,to_jsonb(c),false FROM marketing_campaigns c,scope WHERE c.company_id=scope.company_id\n UNION ALL SELECT 'campaign-cost-source',c.id,to_jsonb(c),NOT EXISTS(SELECT 1 FROM marketing_campaigns campaign WHERE campaign.id=c.campaign_id AND campaign.company_id=c.company_id) FROM marketing_campaign_costs c,scope WHERE c.company_id=scope.company_id\n UNION ALL SELECT 'public-receipt',receipt.key,to_jsonb(receipt),NOT c.source_valid OR receipt.request_hash!~'^[a-f0-9]{64}$' OR receipt.created_at<>c.created_at FROM marketing_public_receipts receipt JOIN (",
            candidate_sources!(),
            ") c ON c.id=receipt.candidate_id CROSS JOIN scope WHERE c.company_id=scope.company_id ORDER BY kind,id"
        )
    };
}
const PLAN_PACKAGE_OBLIGATIONS: &str = package_obligations!();

const NO_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: false,
    company: false,
    version: false,
    campaign: false,
};
const COMPANY_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: false,
    company: true,
    version: false,
    campaign: false,
};
const ASSET_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: true,
    company: true,
    version: false,
    campaign: false,
};
const FORM_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: true,
    company: true,
    version: true,
    campaign: false,
};
const PUBLIC_FORM_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: true,
    company: false,
    version: false,
    campaign: false,
};
const CAMPAIGN_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: false,
    company: true,
    version: false,
    campaign: true,
};

const ASSET_TABLES: &str = "marketing_assets";
const CAMPAIGN_TABLES: &str = "marketing_campaigns,marketing_campaign_costs";
const PKG_TABLES: &str = "marketing_assets,marketing_asset_events,marketing_lead_candidates,marketing_engagement_events,marketing_campaigns,marketing_campaign_costs,marketing_touches,marketing_public_receipts,marketing_public_limits,command_receipts";
const PKG_CROSS: &[&str] = &["command_receipts", crm_candidates!()];

/// Exact live read of `marketingLocalPointReadSql('asset')`.
pub const ASSET_POINT_READ: ReadPlan = ReadPlan {
    kind: RD_ASSET_POINT,
    caller: "marketing.workspace.asset",
    sql: PLAN_ASSET_POINT,
    binds: &[BindField::AssetId, BindField::CompanyId],
    scope: ASSET_SCOPE,
    cross_owner: &[],
    authority: ASSET_TABLES,
};
/// Exact live read of the Marketing workspace asset list.
pub const ASSET_LIST_READ: ReadPlan = ReadPlan {
    kind: RD_ASSET_LIST,
    caller: "marketing.workspace.assets",
    sql: PLAN_ASSET_LIST,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    cross_owner: &[],
    authority: ASSET_TABLES,
};
/// Exact live read of `activeFormSql('source', true, true)`.
pub const FORM_SOURCE_READ: ReadPlan = ReadPlan {
    kind: RD_FORM_SOURCE,
    caller: "marketing.workspace.sourceForm",
    sql: PLAN_FORM_SOURCE,
    binds: &[BindField::AssetId, BindField::CompanyId, BindField::Version],
    scope: FORM_SCOPE,
    cross_owner: &[],
    authority: ASSET_TABLES,
};
/// Exact live read of `activeFormSql('public')`.
pub const FORM_PUBLIC_READ: ReadPlan = ReadPlan {
    kind: RD_FORM_PUBLIC,
    caller: "public-form.form",
    sql: PLAN_FORM_PUBLIC,
    binds: &[BindField::AssetId],
    scope: PUBLIC_FORM_SCOPE,
    cross_owner: &[],
    authority: ASSET_TABLES,
};
/// Exact live read of `marketingFormContentSql('1')`.
pub const FORM_CONTENT_EXISTS_READ: ReadPlan = ReadPlan {
    kind: RD_FORM_CONTENT_EXISTS,
    caller: "public-form.content-predicate",
    sql: PLAN_FORM_CONTENT_EXISTS,
    binds: &[BindField::AssetId, BindField::CompanyId, BindField::Version],
    scope: FORM_SCOPE,
    cross_owner: &[],
    authority: ASSET_TABLES,
};
/// Exact live read of `marketingFormContentSql('id,version,preview_json')`.
pub const FORM_CONTENT_ROW_READ: ReadPlan = ReadPlan {
    kind: RD_FORM_CONTENT_ROW,
    caller: "public-form.content",
    sql: PLAN_FORM_CONTENT_ROW,
    binds: &[BindField::AssetId, BindField::CompanyId, BindField::Version],
    scope: FORM_SCOPE,
    cross_owner: &[],
    authority: ASSET_TABLES,
};
/// Exact live read of `marketingCampaignExistsSql`.
pub const CAMPAIGN_EXISTS_READ: ReadPlan = ReadPlan {
    kind: RD_CAMPAIGN_EXISTS,
    caller: "campaigns.assertion,public-submission.campaign",
    sql: PLAN_CAMPAIGN_EXISTS,
    binds: &[BindField::CampaignId, BindField::CompanyId],
    scope: CAMPAIGN_SCOPE,
    cross_owner: &[],
    authority: CAMPAIGN_TABLES,
};
/// Exact live read of `marketingPurchaseRowsSql('campaign')`.
pub const CAMPAIGN_WITNESS_READ: ReadPlan = ReadPlan {
    kind: RD_CAMPAIGN_WITNESS,
    caller: "marketing.purchase-witness.campaign",
    sql: PLAN_CAMPAIGN_WITNESS,
    binds: &[],
    scope: NO_SCOPE,
    cross_owner: &[],
    authority: CAMPAIGN_TABLES,
};
/// Exact live read of `marketingPurchaseRowsSql('campaign-cost')`.
pub const CAMPAIGN_COST_WITNESS_READ: ReadPlan = ReadPlan {
    kind: RD_CAMPAIGN_COST_WITNESS,
    caller: "marketing.purchase-witness.campaign-cost",
    sql: PLAN_CAMPAIGN_COST_WITNESS,
    binds: &[],
    scope: NO_SCOPE,
    cross_owner: &[],
    authority: CAMPAIGN_TABLES,
};
/// Exact former live read of `marketingPackageObligations`; the TypeScript
/// statement was retired but its LOCK TABLE and source assertion remain in the
/// caller. The plan inlines the command-receipt and CRM candidate fragments.
pub const PACKAGE_OBLIGATIONS_READ: ReadPlan = ReadPlan {
    kind: RD_PACKAGE_OBLIGATIONS,
    caller: "marketing.package-obligations",
    sql: PLAN_PACKAGE_OBLIGATIONS,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    cross_owner: PKG_CROSS,
    authority: PKG_TABLES,
};

/// The closed registry, in stable wire order.
pub const PLANS: &[ReadPlan] = &[
    ASSET_POINT_READ,
    ASSET_LIST_READ,
    FORM_SOURCE_READ,
    FORM_PUBLIC_READ,
    FORM_CONTENT_EXISTS_READ,
    FORM_CONTENT_ROW_READ,
    CAMPAIGN_EXISTS_READ,
    CAMPAIGN_WITNESS_READ,
    CAMPAIGN_COST_WITNESS_READ,
    PACKAGE_OBLIGATIONS_READ,
];

pub fn plan(kind: &str) -> Option<&'static ReadPlan> {
    PLANS.iter().find(|entry| entry.kind == kind)
}

pub fn is_owner_read(kind: &str) -> bool {
    plan(kind).is_some()
}

pub fn kinds() -> impl Iterator<Item = &'static str> {
    PLANS.iter().map(|entry| entry.kind)
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_READ_PLAN_INVALID",
        "Review the Marketing owner read plan.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_registry_of_ten_caller_unit_reads() {
        assert_eq!(PLANS.len(), 10);
        assert_eq!(kinds().count(), 10);
        assert_eq!(
            kinds().collect::<std::collections::BTreeSet<_>>().len(),
            10,
            "read kinds must be unique"
        );
        for kind in kinds() {
            assert!(plan(kind).is_some(), "{kind}");
            assert!(is_owner_read(kind), "{kind}");
        }
        assert_eq!(plan("bogus.read"), None);
        assert!(!is_owner_read("campaign-write"));
    }

    #[test]
    fn plans_are_select_only_and_bind_counts_match() {
        for entry in PLANS {
            let sql = entry.sql;
            assert!(
                sql.starts_with("SELECT ") || sql.starts_with("WITH "),
                "{}",
                entry.kind
            );
            assert!(!sql.contains(';'), "{}", entry.kind);
            for banned in ["INSERT", "UPDATE ", "DELETE", "FOR UPDATE", "503"] {
                assert!(!sql.contains(banned), "{} contains {banned}", entry.kind);
            }
            assert_eq!(
                sql.matches('?').count(),
                entry.binds.len(),
                "{} bind count",
                entry.kind
            );
            assert!(!entry.caller.is_empty(), "{}", entry.kind);
            assert!(!entry.authority.is_empty(), "{}", entry.kind);
            let scope = entry.scope;
            assert_eq!(scope.asset, entry.binds.contains(&BindField::AssetId));
            assert_eq!(scope.company, entry.binds.contains(&BindField::CompanyId));
            assert_eq!(scope.version, entry.binds.contains(&BindField::Version));
            assert_eq!(scope.campaign, entry.binds.contains(&BindField::CampaignId));
            for fragment in entry.cross_owner {
                assert!(sql.contains(fragment), "{} missing {fragment}", entry.kind);
            }
        }
    }

    #[test]
    fn asset_and_form_plans_keep_their_owner_and_status_predicates() {
        for kind in [
            RD_FORM_SOURCE,
            RD_FORM_PUBLIC,
            RD_FORM_CONTENT_EXISTS,
            RD_FORM_CONTENT_ROW,
        ] {
            let sql = plan(kind).unwrap().sql;
            assert!(sql.contains("kind='form'") || sql.contains("kind='content'"), "{kind}");
            assert!(sql.contains("status='active'"), "{kind}");
        }
        for kind in [RD_CAMPAIGN_EXISTS, RD_CAMPAIGN_WITNESS, RD_CAMPAIGN_COST_WITNESS] {
            assert!(plan(kind).unwrap().sql.contains("marketing_campaign"), "{kind}");
        }
    }

    #[test]
    fn package_obligations_plan_binds_company_and_keeps_cross_owner_fragments() {
        let entry = plan(RD_PACKAGE_OBLIGATIONS).unwrap();
        assert_eq!(entry.binds, &[BindField::CompanyId]);
        assert!(entry.scope.company);
        assert!(!entry.scope.asset && !entry.scope.version && !entry.scope.campaign);
        assert_eq!(entry.sql.matches('?').count(), 1);
        assert_eq!(entry.sql.matches("SELECT ").count(), 17);
        for fragment in entry.cross_owner {
            assert!(entry.sql.contains(fragment), "missing {fragment}");
        }
        assert!(entry.sql.contains("command_receipts"));
        assert!(entry.sql.contains(crm_candidates!()));
        assert!(entry.authority.contains("command_receipts"));
        assert_ne!(entry.sql, PLAN_CAMPAIGN_EXISTS);
    }
}
