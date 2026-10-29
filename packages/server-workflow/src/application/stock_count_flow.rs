//! Typed policy inputs for the native Stock Count Flow. Composition must obtain
//! and revalidate Identity, People, Organization, Inventory and purchase facts.
use crate::domain::stock_count_flow::{
    StockCountParticipant, StockCountStep, StockCountStepDecision, StockCountStepFacts,
    current_stock_count_step, resolve_stock_count_step, stock_count_terminal,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountFlowRecord {
    pub company_id: String,
    pub count_id: String,
    pub branch_id: String,
    pub status: String,
    pub version: i64,
    pub creator: String,
    pub preparer: String,
    pub approved_by: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountSavedStep {
    pub primary_user_id: Option<String>,
    pub team_name: String,
    pub default_revision: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountRoute {
    pub version: i64,
    pub defaults_revision: i64,
    pub steps: [StockCountSavedStep; 3],
    pub created_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountResolvedStep {
    pub step: StockCountStep,
    pub decision: StockCountStepDecision,
    pub team_name: String,
    pub candidates: Vec<StockCountParticipant>,
    pub actor_role_allowed: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountFlowView {
    pub record: StockCountFlowRecord,
    pub flow_version: i64,
    pub defaults_revision: i64,
    pub steps: [StockCountResolvedStep; 3],
    pub current: Option<StockCountStep>,
    pub can_act: bool,
    pub can_manage: bool,
    pub blocker: Option<&'static str>,
}
/// Candidate lists are canonical, ordered and active; selected IDs implement
/// saved -> preferred -> applicable default before calling this pure policy.
/// External owners must retain all images through completion.
pub fn resolve_stock_count_flow(
    record: StockCountFlowRecord,
    flow_version: i64,
    defaults_revision: i64,
    facts: [StockCountStepFacts; 3],
    actor: &str,
    company_admin: bool,
    purchase_allowed: bool,
) -> StockCountFlowView {
    let steps = facts.map(|f| {
        let decision = resolve_stock_count_step(&f);
        let team_name = decision.primary.as_ref().map_or_else(
            || {
                match f.step {
                    StockCountStep::Draft => "warehouse manager",
                    _ => "finance approver",
                }
                .to_owned()
            },
            |p| p.team_name.clone(),
        );
        StockCountResolvedStep {
            step: f.step,
            decision,
            team_name,
            candidates: f.candidates,
            actor_role_allowed: f.actor_role_allowed,
        }
    });
    let current = current_stock_count_step(&record.status);
    let terminal = stock_count_terminal(&record.status);
    let blocker = if terminal {
        None
    } else {
        current
            .and_then(|step| steps[step.index()].decision.blocker)
            .or_else(|| {
                current
                    .is_none()
                    .then_some("This document requires routing repair before proceeding.")
            })
    };
    let can_act = purchase_allowed
        && blocker.is_none()
        && current.is_some_and(|step| {
            let s = &steps[step.index()];
            s.actor_role_allowed
                && s.decision
                    .primary
                    .as_ref()
                    .is_some_and(|p| p.user_id == actor)
        });
    StockCountFlowView {
        record,
        flow_version,
        defaults_revision,
        steps,
        current,
        can_act,
        can_manage: purchase_allowed && company_admin,
        blocker,
    }
}
/// Scoped notification and person/role evidence are foreign-owned mandatory
/// same-unit effects; implementations are supplied by the actual owner adapters.
pub trait StockCountFlowOwnerPorts: Send {
    type Error: Send;
    fn require_participants_current(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn require_purchase_and_authority_current(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn notify_assignment(
        &mut self,
        person: &StockCountParticipant,
        record: &StockCountFlowRecord,
        step: StockCountStep,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn notify_next_action(
        &mut self,
        person: &StockCountParticipant,
        record: &StockCountFlowRecord,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}

/// Normalize the strict Service Flow assignment schema into the exact ordered
/// `{kind,...input}` object fingerprinted by the retained Workflow ledger.
/// The route accepts `saveDefault` omitted or false only. Identity and purchase
/// checks remain separate, and this function confers no authorization.
pub fn normalize_stock_count_assignment(
    input: &str,
) -> Result<crate::domain::command_json::CommandJson, erpx_server_shared::public::BusinessError> {
    use erpx_server_shared::public::BusinessError;
    let invalid =
        || BusinessError::invalid("VALIDATION_FAILED", "Use a valid Count Flow assignment.");
    use crate::domain::command_json::CommandJson;
    let object = CommandJson::parse(input)?;
    if object.object_keys()?.ok_or_else(invalid)?.iter().any(|k| {
        !matches!(
            k.as_str(),
            "stepKey" | "userId" | "expectedVersion" | "recordVersion" | "reason" | "saveDefault"
        )
    }) {
        return Err(invalid());
    }
    let property = |name: &str| -> Result<CommandJson, BusinessError> {
        object.property(name)?.ok_or_else(invalid)
    };
    let step = property("stepKey")?
        .string_utf16_units()?
        .ok_or_else(invalid)?;
    let user = property("userId")?
        .string_utf16_units()?
        .ok_or_else(invalid)?;
    let reason = property("reason")?
        .string_utf16_units()?
        .ok_or_else(invalid)?;
    let whitespace = |c: &u16| matches!(*c,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff);
    let start = reason
        .iter()
        .position(|c| !whitespace(c))
        .unwrap_or(reason.len());
    let end = reason
        .iter()
        .rposition(|c| !whitespace(c))
        .map_or(start, |i| i + 1);
    let reason = &reason[start..end];
    let number = |key: &str, minimum: f64| -> Result<u64, BusinessError> {
        let json = property(key)?;
        let n = json.as_str().parse::<f64>().map_err(|_| invalid())?;
        if !n.is_finite() || n.fract() != 0.0 || n < minimum || n > 9_007_199_254_740_991.0 {
            return Err(invalid());
        }
        Ok(n as u64)
    };
    let flow = number("expectedVersion", 0.0)?;
    let record = number("recordVersion", 1.0)?;
    if step.is_empty()
        || step.len() > 100
        || user.is_empty()
        || user.len() > 180
        || reason.len() < 3
        || reason.len() > 2000
        || object
            .property("saveDefault")?
            .is_some_and(|v| v.as_str() != "false")
    {
        return Err(invalid());
    }
    let step = CommandJson::from_utf16_string(&step)?;
    let user = CommandJson::from_utf16_string(&user)?;
    let reason = CommandJson::from_utf16_string(reason)?;
    CommandJson::parse(&format!(
        "{{\"kind\":\"stock_count\",\"stepKey\":{},\"userId\":{},\"expectedVersion\":{},\"recordVersion\":{},\"reason\":{},\"saveDefault\":false}}",
        step.as_str(),
        user.as_str(),
        flow,
        record,
        reason.as_str()
    ))
}
