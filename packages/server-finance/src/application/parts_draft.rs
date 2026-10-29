//! Exact original parts journal derivation. All exposed inputs/outputs are
//! ordinary data; only the adapter's mandatory current owner port can write.
use super::journal::{FinanceActor, JournalDraft, JournalIdentitySeed};
use crate::domain::{journal::JournalLineInput, money::units_to_decimal};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartsOwnerKind {
    Maintenance,
    FieldService,
}
impl PartsOwnerKind {
    pub(crate) fn source(self) -> &'static str {
        match self {
            Self::Maintenance => "maintenance",
            Self::FieldService => "field_service",
        }
    }
}
/// Physical identity remains owner-specific; this data grants no authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PartsDraftSourceIdentity {
    Existing(RecordId),
    FieldServicePhysical(String),
}
impl PartsDraftSourceIdentity {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Existing(id) => id.as_str(),
            Self::FieldServicePhysical(id) => id,
        }
    }
}
/// Physical Service scope is sourced from its sealed owner witness; Maintenance
/// retains its existing typed identifiers. Neither variant grants authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PartsDraftScope {
    Maintenance {
        branch_id: RecordId,
        department_id: Option<RecordId>,
    },
    FieldServicePhysical {
        branch_id: String,
        department_id: Option<String>,
    },
}
/// Factual original OWNER command binding. No permission or context factory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartsDraftFacts {
    pub actor: FinanceActor,
    pub owner_key: CommandKey,
    pub kind: PartsOwnerKind,
    pub source_id: PartsDraftSourceIdentity,
    pub source_version: i64,
    pub scope: PartsDraftScope,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub consumption_at: TimestampMillis,
    pub reason: String,
}
/// These values must come from the original retained Finance/Inventory facts
/// for actual effects. Plain configuration data never authorizes preparation.
#[derive(Clone, Debug)]
pub struct PartsDraftConfig {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub chart_id: RecordId,
    pub period_id: RecordId,
    pub entry_date: String,
    pub currency: String,
    pub base_currency: String,
    pub inventory_account_id: RecordId,
    pub expense_account_id: RecordId,
}
/// Deterministic original identity, not a second command/receipt or authority.
pub struct PartsDraftIdentity {
    consumption_id: RecordId,
    journal_id: RecordId,
    journal_number: String,
}
impl PartsDraftIdentity {
    pub fn consumption_id(&self) -> &RecordId {
        &self.consumption_id
    }
    pub fn journal_id(&self) -> &RecordId {
        &self.journal_id
    }
    pub fn journal_number(&self) -> &str {
        &self.journal_number
    }
}
pub fn parts_draft_identity(key: &CommandKey) -> Result<PartsDraftIdentity, BusinessError> {
    let seed = JournalIdentitySeed::owner_parts(key);
    Ok(PartsDraftIdentity {
        consumption_id: RecordId::new(format!("operational-part-{}", key.as_str()))?,
        journal_id: RecordId::new(seed.journal_id())?,
        journal_number: seed.journal_number(),
    })
}
/// A derived plan is data only. There is deliberately no persistence method.
pub struct PartsDraftPlan {
    draft: JournalDraft,
    seed: JournalIdentitySeed,
    identity: PartsDraftIdentity,
    consumption_at: TimestampMillis,
}
impl PartsDraftPlan {
    pub fn draft(&self) -> &JournalDraft {
        &self.draft
    }
    pub fn identity(&self) -> &PartsDraftIdentity {
        &self.identity
    }
    pub fn consumption_at(&self) -> TimestampMillis {
        self.consumption_at
    }
    pub(crate) fn into_parts(
        self,
    ) -> (JournalDraft, JournalIdentitySeed, RecordId, TimestampMillis) {
        (
            self.draft,
            self.seed,
            self.identity.consumption_id,
            self.consumption_at,
        )
    }
}
pub fn derive_parts_draft(
    facts: &PartsDraftFacts,
    config: &PartsDraftConfig,
) -> Result<PartsDraftPlan, BusinessError> {
    const SAFE: i64 = 9_007_199_254_740_991;
    if matches!(
        &facts.source_id,
        PartsDraftSourceIdentity::FieldServicePhysical(_)
    ) && facts.kind != PartsOwnerKind::FieldService
    {
        return Err(invalid("PARTS_DRAFT_SCOPE_CHANGED"));
    }
    if !matches!(
        (&facts.kind, &facts.scope),
        (
            PartsOwnerKind::Maintenance,
            PartsDraftScope::Maintenance { .. }
        ) | (
            PartsOwnerKind::FieldService,
            PartsDraftScope::FieldServicePhysical { .. }
        )
    ) {
        return Err(invalid("PARTS_DRAFT_SCOPE_CHANGED"));
    }
    if facts.actor.holding_id != config.holding_id || facts.actor.company_id != config.company_id {
        return Err(invalid("PARTS_DRAFT_SCOPE_CHANGED"));
    }
    let reason_scalars = match facts.kind {
        PartsOwnerKind::Maintenance => 2000,
        PartsOwnerKind::FieldService => 3000,
    };
    // Original Work input permits MAX_SAFE expectedVersion; its provisional
    // expectedVersion+1 is data before Journal/Money preparation. Actual owner
    // version/output admission remains mandatory at the later apply boundary.
    if !(1..=SAFE + 1).contains(&facts.source_version)
        || !(1..=SAFE).contains(&facts.quantity_milli)
        || !(1..=SAFE).contains(&facts.value_minor)
        || !(0..=SAFE).contains(&facts.consumption_at.get())
        || facts.reason.len() > reason_scalars * 4
        || facts.reason.chars().count() > reason_scalars
        || config.currency != config.base_currency
        || config.entry_date.len() != 10
        || !time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
            .is_ok_and(|format| time::Date::parse(&config.entry_date, &format).is_ok())
    {
        return Err(invalid("PARTS_DRAFT_INPUT_INVALID"));
    }
    // Do not add account/category/active/currency mapping guards here: B0
    // prepares Journal and Money first, then evaluates those effect guards.
    let identity = parts_draft_identity(&facts.owner_key)?;
    let amount = units_to_decimal(&facts.value_minor.into(), 2)
        .map_err(|_| invalid("PARTS_DRAFT_INPUT_INVALID"))?;
    let draft = JournalDraft {
        chart_id: config.chart_id.as_str().into(),
        period_id: config.period_id.as_str().into(),
        entry_date: config.entry_date.clone(),
        currency: config.currency.clone(),
        fx_rate: None,
        is_opening: false,
        memo: format!(
            "{} spare usage {} · {}",
            facts.kind.source(),
            facts.source_id.as_str(),
            facts.reason
        ),
        source_type: format!("{}_parts", facts.kind.source()),
        source_id: Some(identity.consumption_id.as_str().into()),
        lines: vec![
            JournalLineInput {
                account_id: config.expense_account_id.as_str().into(),
                description: None,
                debit: amount.clone(),
                credit: "0".into(),
                dimensions: None,
            },
            JournalLineInput {
                account_id: config.inventory_account_id.as_str().into(),
                description: None,
                debit: "0".into(),
                credit: amount,
                dimensions: None,
            },
        ],
    };
    Ok(PartsDraftPlan {
        draft,
        seed: JournalIdentitySeed::owner_parts(&facts.owner_key),
        identity,
        consumption_at: facts.consumption_at,
    })
}
fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(
        code,
        "Use the exact bounded original parts command and Finance facts.",
    )
}
