//! Native Finance due-draft schedule validation.
//!
//! This mirrors the retained TypeScript oracle in
//! `packages/server-finance/src/domain/due-drafts.ts`
//! (`financeDueDraftSchema`, `parsedInput`) and the decision rules in
//! `packages/server-finance/src/application/due-drafts.ts`
//! (`scheduleFinanceDraft`): the four draft kinds, their source-edition
//! tables, the strict input shape, the recognition billing-run binding and
//! the expected-version resolution. No IO lives here; the adapter owns the
//! caller-transaction reads and writes.

/// Draft kinds, byte-identical to the TypeScript `kind` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DueDraftKind {
    Billing,
    Recognition,
    AssetDepreciation,
    InstrumentPayment,
}

impl DueDraftKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "billing" => Some(Self::Billing),
            "recognition" => Some(Self::Recognition),
            "asset_depreciation" => Some(Self::AssetDepreciation),
            "instrument_payment" => Some(Self::InstrumentPayment),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Billing => "billing",
            Self::Recognition => "recognition",
            Self::AssetDepreciation => "asset_depreciation",
            Self::InstrumentPayment => "instrument_payment",
        }
    }

    /// Retained source-edition table per kind, mirroring the TypeScript
    /// `sources` record in `finance-due-drafts.ts`. The table name is
    /// selected by this match, never interpolated from caller text.
    pub fn source_table(self) -> &'static str {
        match self {
            Self::Billing => "finance_business_contracts",
            Self::Recognition => "finance_business_billing_runs",
            Self::AssetDepreciation => "finance_asset_policy_editions",
            Self::InstrumentPayment => "finance_instrument_editions",
        }
    }

    /// Retained purchase capability per kind, mirroring the
    /// `withFinancePurchasedCommand` capability selection: only instrument
    /// payments require `finance.advanced`; every other kind settles under
    /// `finance.accounting`. Composition enforces the capability; this only
    /// names it.
    pub fn capability(self) -> &'static str {
        match self {
            Self::InstrumentPayment => "finance.advanced",
            _ => "finance.accounting",
        }
    }
}

/// Validated schedule input. Field names mirror the TypeScript schema so the
/// future transport can accept the retained wire shape unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueDraftScheduleInput {
    pub kind: DueDraftKind,
    pub source_id: String,
    pub run_on: String,
    pub payload_json: String,
    pub reason: String,
}

/// Decision actions, mirroring the TypeScript `retry`/`cancel` route actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DueDraftAction {
    Retry,
    Cancel,
}

impl DueDraftAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "retry" => Some(Self::Retry),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }

    /// Retained operation per action, mirroring the TypeScript op template
    /// `` `finance.due-draft.${action}` ``.
    pub fn operation(self) -> &'static str {
        match self {
            Self::Retry => "finance.due-draft.retry",
            Self::Cancel => "finance.due-draft.cancel",
        }
    }

    /// Retained status transition: retry reopens the draft as scheduled,
    /// cancel closes it as cancelled.
    pub fn status(self) -> &'static str {
        match self {
            Self::Retry => "scheduled",
            Self::Cancel => "cancelled",
        }
    }
}

/// Validated decision input, mirroring `financeDueDecisionSchema`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueDraftDecisionInput {
    pub action: DueDraftAction,
    pub expected_version: i64,
    pub reason: String,
}

pub fn schedule_changed() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 409,
        code: "FINANCE_SCHEDULE_CHANGED",
        message: "Refresh this pending Finance schedule before changing it.",
    }
}

/// Strict decision-input validation: known action, positive-integer
/// expected version, in-range reason.
pub fn validate_decision(
    action: &str,
    expected_version: i64,
    reason: &str,
) -> Result<DueDraftDecisionInput, ScheduleRefusal> {
    let action =
        DueDraftAction::parse(action).ok_or_else(|| invalid_input("Choose a draft decision."))?;
    if expected_version <= 0 {
        return Err(invalid_input("Choose the current schedule version."));
    }
    let trimmed = reason.trim();
    if trimmed.len() < 3 || trimmed.len() > 2000 {
        return Err(invalid_input("State the decision reason."));
    }
    Ok(DueDraftDecisionInput {
        action,
        expected_version,
        reason: trimmed.to_owned(),
    })
}

/// Retained decision gate: the draft must still carry the expected version
/// and be pending (`scheduled` or `blocked`). Anything else fails closed
/// exactly like the TypeScript `schedule-version` assertion.
pub fn check_decision_state(
    row_version: i64,
    row_status: &str,
    expected_version: i64,
) -> Result<(), ScheduleRefusal> {
    if row_version != expected_version
        || (row_status != "scheduled" && row_status != "blocked")
    {
        return Err(schedule_changed());
    }
    Ok(())
}
/// Failure codes are byte-identical to the TypeScript oracle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleRefusal {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}

pub fn holding_required() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 403,
        code: "FINANCE_SCHEDULE_HOLDING_REQUIRED",
        message: "Schedule Company-wide Finance drafts with a Holding Finance assignment.",
    }
}

pub fn source_mismatch() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 409,
        code: "FINANCE_SCHEDULE_SOURCE_CHANGED",
        message: "Choose the current active source edition and a current Company Finance assignment.",
    }
}

pub fn recognition_binding() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 422,
        code: "FINANCE_SCHEDULE_SOURCE",
        message: "Recognition must use the selected billing run.",
    }
}

fn invalid_input(message: &'static str) -> ScheduleRefusal {
    ScheduleRefusal {
        status: 422,
        code: "FINANCE_SCHEDULE_INPUT",
        message,
    }
}

fn valid_date(value: &str) -> bool {
    // Mirrors `z.string().date()`: an ISO calendar day, not just ten
    // date-shaped characters.
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits = |from: usize, to: usize| -> Option<u32> {
        let part = value.get(from..to)?;
        if part.len() != to - from || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse::<u32>().ok()
    };
    let (Some(year), Some(month), Some(day)) =
        (digits(0, 4), digits(5, 7), digits(8, 10))
    else {
        return false;
    };
    if month < 1 || month > 12 || day < 1 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day <= max_day
}

/// Strict schedule-input validation. Unknown kinds, empty source ids,
/// non-date `run_on` values and out-of-range reasons fail closed here, before
/// any row is read.
pub fn validate_schedule(
    kind: &str,
    source_id: &str,
    run_on: &str,
    payload_json: &str,
    reason: &str,
) -> Result<DueDraftScheduleInput, ScheduleRefusal> {
    let kind = DueDraftKind::parse(kind)
        .ok_or_else(|| invalid_input("Choose a supported draft workflow."))?;
    if source_id.is_empty() {
        return Err(invalid_input("Choose a draft source."));
    }
    if !valid_date(run_on) {
        return Err(invalid_input("Choose a valid run date."));
    }
    let trimmed = reason.trim();
    if trimmed.len() < 3 || trimmed.len() > 2000 {
        return Err(invalid_input("State the draft reason."));
    }
    let payload: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_| invalid_input("The draft input must be valid JSON."))?;
    if !payload.is_object() {
        return Err(invalid_input("The draft input must be valid JSON."));
    }
    // Retained recognition binding: the payload's billing run must be the
    // selected source. The TypeScript oracle parses the full recognition run
    // schema; the binding field is the only payload fact the schedule gate
    // decides on, and a missing field fails closed exactly like a mismatch.
    if kind == DueDraftKind::Recognition {
        let bound = payload
            .get("billingRunId")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if bound != source_id {
            return Err(recognition_binding());
        }
    }
    Ok(DueDraftScheduleInput {
        kind,
        source_id: source_id.to_owned(),
        run_on: run_on.to_owned(),
        payload_json: payload.to_string(),
        reason: trimmed.to_owned(),
    })
}

/// Retained expected-version resolution: an explicit safe-integer
/// `expectedVersion` in the payload wins, otherwise the live source version
/// applies. A non-integer payload version fails closed.
pub fn resolve_expected_version(
    payload_json: &str,
    source_version: i64,
) -> Result<i64, ScheduleRefusal> {
    let payload: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_| invalid_input("The draft input must be valid JSON."))?;
    match payload.get("expectedVersion") {
        None | Some(serde_json::Value::Null) => Ok(source_version),
        Some(serde_json::Value::Number(number)) => number
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or_else(|| invalid_input("Choose the current source version.")),
        Some(_) => Err(invalid_input("Choose the current source version.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: &str, payload: &str) -> Result<DueDraftScheduleInput, ScheduleRefusal> {
        validate_schedule(kind, "src-1", "2026-11-30", payload, "Quarter close draft")
    }

    #[test]
    fn kinds_map_to_their_retained_source_tables() {
        assert_eq!(
            DueDraftKind::parse("billing").unwrap().source_table(),
            "finance_business_contracts"
        );
        assert_eq!(
            DueDraftKind::parse("recognition").unwrap().source_table(),
            "finance_business_billing_runs"
        );
        assert_eq!(
            DueDraftKind::parse("asset_depreciation")
                .unwrap()
                .source_table(),
            "finance_asset_policy_editions"
        );
        assert_eq!(
            DueDraftKind::parse("instrument_payment")
                .unwrap()
                .source_table(),
            "finance_instrument_editions"
        );
        assert_eq!(DueDraftKind::parse("dividend"), None);
    }

    #[test]
    fn only_instrument_payments_require_the_advanced_capability() {
        assert_eq!(DueDraftKind::parse("billing").unwrap().capability(), "finance.accounting");
        assert_eq!(
            DueDraftKind::parse("recognition").unwrap().capability(),
            "finance.accounting"
        );
        assert_eq!(
            DueDraftKind::parse("instrument_payment").unwrap().capability(),
            "finance.advanced"
        );
    }

    #[test]
    fn strict_shape_rejects_unknown_kinds_and_bad_fields() {
        assert!(input("dividend", "{}").is_err());
        assert!(validate_schedule("billing", "", "2026-11-30", "{}", "Quarter close draft").is_err());
        assert!(validate_schedule("billing", "src-1", "30-11-2026", "{}", "Quarter close draft").is_err());
        assert!(validate_schedule("billing", "src-1", "2026-11-30", "{}", "no").is_err());
        assert!(validate_schedule("billing", "src-1", "2026-11-30", "[1,2]", "Quarter close draft").is_err());
        assert!(validate_schedule("billing", "src-1", "2026-11-30", "{oops", "Quarter close draft").is_err());
        let kept = input("billing", r#"{"expectedVersion":3}"#).unwrap();
        assert_eq!(kept.reason, "Quarter close draft");
    }

    #[test]
    fn recognition_binds_the_payload_to_the_selected_billing_run() {
        let bound = validate_schedule(
            "recognition",
            "run-7",
            "2026-11-30",
            r#"{"billingRunId":"run-7"}"#,
            "Recognize November revenue",
        )
        .unwrap();
        assert_eq!(bound.kind, DueDraftKind::Recognition);
        let refusal = validate_schedule(
            "recognition",
            "run-7",
            "2026-11-30",
            r#"{"billingRunId":"run-9"}"#,
            "Recognize November revenue",
        )
        .expect_err("mismatched billing run must fail");
        assert_eq!(refusal.code, "FINANCE_SCHEDULE_SOURCE");
        assert_eq!(refusal.status, 422);
        let missing = validate_schedule(
            "recognition",
            "run-7",
            "2026-11-30",
            "{}",
            "Recognize November revenue",
        )
        .expect_err("missing billing run must fail closed");
        assert_eq!(missing.code, "FINANCE_SCHEDULE_SOURCE");
    }

    #[test]
    fn expected_version_prefers_an_explicit_safe_integer() {
        assert_eq!(resolve_expected_version(r#"{"expectedVersion":4}"#, 9).unwrap(), 4);
        assert_eq!(resolve_expected_version("{}", 9).unwrap(), 9);
        assert_eq!(resolve_expected_version(r#"{"expectedVersion":null}"#, 9).unwrap(), 9);
        assert!(resolve_expected_version(r#"{"expectedVersion":1.5}"#, 9).is_err());
        assert!(resolve_expected_version(r#"{"expectedVersion":"4"}"#, 9).is_err());
    }

    #[test]
    fn decision_actions_carry_the_retained_operations_and_statuses() {
        assert_eq!(DueDraftAction::parse("retry").unwrap().operation(), "finance.due-draft.retry");
        assert_eq!(DueDraftAction::parse("cancel").unwrap().operation(), "finance.due-draft.cancel");
        assert_eq!(DueDraftAction::parse("retry").unwrap().status(), "scheduled");
        assert_eq!(DueDraftAction::parse("cancel").unwrap().status(), "cancelled");
        assert_eq!(DueDraftAction::parse("hold"), None);
    }

    #[test]
    fn decision_input_rejects_unknown_actions_and_bad_versions() {
        assert!(validate_decision("retry", 2, "Collector recovered").is_ok());
        assert!(validate_decision("hold", 2, "Collector recovered").is_err());
        assert!(validate_decision("cancel", 0, "Duplicate schedule").is_err());
        assert!(validate_decision("cancel", 2, "no").is_err());
    }

    #[test]
    fn decision_gate_requires_the_expected_pending_version() {
        assert!(check_decision_state(1, "scheduled", 1).is_ok());
        assert!(check_decision_state(3, "blocked", 3).is_ok());
        let stale = check_decision_state(1, "scheduled", 2).expect_err("stale version must fail");
        assert_eq!(stale.code, "FINANCE_SCHEDULE_CHANGED");
        assert_eq!(stale.status, 409);
        let closed = check_decision_state(2, "cancelled", 2).expect_err("closed draft must fail");
        assert_eq!(closed.code, "FINANCE_SCHEDULE_CHANGED");
        let prepared = check_decision_state(2, "prepared", 2).expect_err("prepared draft must fail");
        assert_eq!(
            prepared.message,
            "Refresh this pending Finance schedule before changing it."
        );
    }

    #[test]
    fn refusals_keep_the_retained_codes_and_messages() {
        let held = holding_required();
        assert_eq!(held.status, 403);
        assert_eq!(held.code, "FINANCE_SCHEDULE_HOLDING_REQUIRED");
        let changed = source_mismatch();
        assert_eq!(changed.status, 409);
        assert_eq!(changed.code, "FINANCE_SCHEDULE_SOURCE_CHANGED");
        assert_eq!(
            changed.message,
            "Choose the current active source edition and a current Company Finance assignment."
        );
    }
}
