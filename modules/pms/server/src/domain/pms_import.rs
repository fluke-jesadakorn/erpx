//! Native PMS cutover-import write planner (pure).
//!
//! This module validates the six retained cutover-import write facts and plans
//! the pms-owned batch/document/balance state transitions. It never opens a
//! transaction or runs SQL; the owner adapter owns all rows and the caller owns
//! the serializable unit. Finance opening verification, workflow receipts and
//! the PMS snapshot simulation stay in their own owners/consumers.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const IMPORT_PREVIEW_OPERATION: &str = "pms.import.write.preview";
pub const IMPORT_APPROVE_OPERATION: &str = "pms.import.write.approve";
pub const IMPORT_BALANCE_OPERATION: &str = "pms.import.write.balance";
pub const IMPORT_DOCUMENT_OPERATION: &str = "pms.import.write.document";
pub const IMPORT_PROPERTY_OPERATION: &str = "pms.import.write.property";
pub const IMPORT_APPLIED_OPERATION: &str = "pms.import.write.applied";

pub const MAX_ID: usize = 200;
pub const MAX_KEY: usize = 200;
pub const MAX_TEXT: usize = 1_000_000;

/// Typed command kinds accepted by the native import write router. The wire
/// representation is the exact retained fact name; no SQL is transported.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PmsImportCommandKind {
    Preview,
    Approve,
    Balance,
    Document,
    Property,
    Applied,
}

impl PmsImportCommandKind {
    pub fn operation(self) -> &'static str {
        match self {
            Self::Preview => IMPORT_PREVIEW_OPERATION,
            Self::Approve => IMPORT_APPROVE_OPERATION,
            Self::Balance => IMPORT_BALANCE_OPERATION,
            Self::Document => IMPORT_DOCUMENT_OPERATION,
            Self::Property => IMPORT_PROPERTY_OPERATION,
            Self::Applied => IMPORT_APPLIED_OPERATION,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Approve => "approve",
            Self::Balance => "balance",
            Self::Document => "document",
            Self::Property => "property",
            Self::Applied => "applied",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PreviewInput<'a> {
    pub batch_id: &'a str,
    pub company_id: &'a str,
    pub property_id: &'a str,
    pub cutover_date: &'a str,
    pub expected_version: i64,
    pub definition_json: &'a str,
    pub report_json: &'a str,
    pub user_id: &'a str,
    pub now: i64,
    pub now2: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApproveInput<'a> {
    pub user_id: &'a str,
    pub opening_journal_id: Option<&'a str>,
    pub now: i64,
    pub id: &'a str,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportPocket {
    Advance,
    Deposit,
    Due,
}

#[derive(Clone, Debug, Serialize)]
pub struct BalanceInput<'a> {
    pub id: &'a str,
    pub source_key: &'a str,
    pub property_id: &'a str,
    pub guest_account_id: &'a str,
    pub entry_id: &'a str,
    pub pocket: ImportPocket,
    pub total: i64,
    pub payment_id: Option<&'a str>,
    pub invoice_id: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DocumentInput<'a> {
    pub id: &'a str,
    pub property_id: &'a str,
    pub guest_account_id: &'a str,
    pub finance_id: Option<&'a str>,
    pub snapshot_json: &'a str,
    pub now: i64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PropertyInput<'a> {
    pub now: i64,
    pub property_id: &'a str,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AppliedInput<'a> {
    pub now: i64,
    pub id: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub enum PmsImportInput<'a> {
    Preview(PreviewInput<'a>),
    Approve(ApproveInput<'a>),
    Balance(BalanceInput<'a>),
    Document(DocumentInput<'a>),
    Property(PropertyInput<'a>),
    Applied(AppliedInput<'a>),
}

impl PmsImportInput<'_> {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Preview(_) => IMPORT_PREVIEW_OPERATION,
            Self::Approve(_) => IMPORT_APPROVE_OPERATION,
            Self::Balance(_) => IMPORT_BALANCE_OPERATION,
            Self::Document(_) => IMPORT_DOCUMENT_OPERATION,
            Self::Property(_) => IMPORT_PROPERTY_OPERATION,
            Self::Applied(_) => IMPORT_APPLIED_OPERATION,
        }
    }
}

/// Deterministic native receipt namespace. The retained workflow `executeCommand`
/// receipt uses the bare key; native writes use a distinct namespace so the two
/// ledgers never collide on `command_receipts.command_id`.
pub fn native_command_id(key: &str, kind: PmsImportCommandKind) -> String {
    format!("pms-import-native:{key}:{}", kind.as_str())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchState {
    Preview,
    Approved,
    Applied,
}

impl BatchState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Approved => "approved",
            Self::Applied => "applied",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "preview" => Ok(Self::Preview),
            "approved" => Ok(Self::Approved),
            "applied" => Ok(Self::Applied),
            _ => Err(invalid()),
        }
    }
}

/// Retained batch state machine: preview -> approved -> applied. A writer may
/// only advance one step and must carry the current optimistic version.
pub fn plan_batch_transition(
    current: BatchState,
    expected_version: i64,
    target: BatchState,
) -> Result<i64, BusinessError> {
    if expected_version < 1 {
        return Err(invalid());
    }
    let allowed = matches!(
        (current, target),
        (BatchState::Preview, BatchState::Approved) | (BatchState::Approved, BatchState::Applied)
    );
    if !allowed {
        return Err(conflict(
            "PMS_IMPORT_STATE",
            "This import batch cannot take that step.",
        ));
    }
    Ok(expected_version + 1)
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_IMPORT_WRITE_INVALID",
        "Review the cutover-import write details.",
    )
}

pub fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command was already used for a different write. Start a new action.",
    )
}

fn text<'a>(raw: &'a Value, field: &str, max: usize) -> Result<&'a str, BusinessError> {
    match raw.get(field) {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= max => Ok(value),
        _ => Err(invalid()),
    }
}

fn optional_text<'a>(
    raw: &'a Value,
    field: &str,
    max: usize,
) -> Result<Option<&'a str>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() && value.len() <= max => Ok(Some(value)),
        _ => Err(invalid()),
    }
}

fn integer(raw: &Value, field: &str, min: i64) -> Result<i64, BusinessError> {
    let Some(Value::Number(value)) = raw.get(field) else {
        return Err(invalid());
    };
    value
        .as_i64()
        .filter(|value| *value >= min)
        .ok_or_else(invalid)
}

pub fn parse_input<'a>(
    kind: PmsImportCommandKind,
    raw: &'a Value,
) -> Result<PmsImportInput<'a>, BusinessError> {
    if !raw.is_object() {
        return Err(invalid());
    }
    Ok(match kind {
        PmsImportCommandKind::Preview => PmsImportInput::Preview(PreviewInput {
            batch_id: text(raw, "batchId", MAX_ID)?,
            company_id: text(raw, "companyId", MAX_ID)?,
            property_id: text(raw, "propertyId", MAX_ID)?,
            cutover_date: text(raw, "cutoverDate", 10)?,
            expected_version: integer(raw, "expectedVersion", 1)?,
            definition_json: text(raw, "definitionJson", MAX_TEXT)?,
            report_json: text(raw, "reportJson", MAX_TEXT)?,
            user_id: text(raw, "userId", MAX_ID)?,
            now: integer(raw, "now", 1)?,
            now2: integer(raw, "now2", 1)?,
        }),
        PmsImportCommandKind::Approve => PmsImportInput::Approve(ApproveInput {
            user_id: text(raw, "userId", MAX_ID)?,
            opening_journal_id: optional_text(raw, "openingJournalId", MAX_ID)?,
            now: integer(raw, "now", 1)?,
            id: text(raw, "id", MAX_ID)?,
        }),
        PmsImportCommandKind::Balance => {
            let pocket = match text(raw, "pocket", 16)? {
                "advance" => ImportPocket::Advance,
                "deposit" => ImportPocket::Deposit,
                "due" => ImportPocket::Due,
                _ => return Err(invalid()),
            };
            PmsImportInput::Balance(BalanceInput {
                id: text(raw, "id", MAX_ID)?,
                source_key: text(raw, "sourceKey", MAX_KEY)?,
                property_id: text(raw, "propertyId", MAX_ID)?,
                guest_account_id: text(raw, "guestAccountId", MAX_ID)?,
                entry_id: text(raw, "entryId", MAX_ID)?,
                pocket,
                total: integer(raw, "total", 0)?,
                payment_id: optional_text(raw, "paymentId", MAX_ID)?,
                invoice_id: optional_text(raw, "invoiceId", MAX_ID)?,
            })
        }
        PmsImportCommandKind::Document => PmsImportInput::Document(DocumentInput {
            id: text(raw, "id", MAX_ID)?,
            property_id: text(raw, "propertyId", MAX_ID)?,
            guest_account_id: text(raw, "guestAccountId", MAX_ID)?,
            finance_id: optional_text(raw, "financeId", MAX_ID)?,
            snapshot_json: text(raw, "snapshotJson", MAX_TEXT)?,
            now: integer(raw, "now", 1)?,
        }),
        PmsImportCommandKind::Property => PmsImportInput::Property(PropertyInput {
            now: integer(raw, "now", 1)?,
            property_id: text(raw, "propertyId", MAX_ID)?,
        }),
        PmsImportCommandKind::Applied => PmsImportInput::Applied(AppliedInput {
            now: integer(raw, "now", 1)?,
            id: text(raw, "id", MAX_ID)?,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn command_ids_are_namespaced_and_deterministic() {
        let id = native_command_id("key-1", PmsImportCommandKind::Preview);
        assert_eq!(id, "pms-import-native:key-1:preview");
        assert_ne!(
            id,
            native_command_id("key-1", PmsImportCommandKind::Approve)
        );
    }

    #[test]
    fn batch_transition_only_advances_one_step() {
        assert_eq!(
            plan_batch_transition(BatchState::Preview, 1, BatchState::Approved).unwrap(),
            2
        );
        assert_eq!(
            plan_batch_transition(BatchState::Approved, 2, BatchState::Applied).unwrap(),
            3
        );
        assert!(plan_batch_transition(BatchState::Preview, 1, BatchState::Applied).is_err());
        assert!(plan_batch_transition(BatchState::Applied, 3, BatchState::Applied).is_err());
    }

    #[test]
    fn parses_each_typed_write_fact() {
        let preview_value = json!({
            "batchId": "b", "companyId": "c", "propertyId": "p", "cutoverDate": "2026-01-01",
            "expectedVersion": 1, "definitionJson": "{}", "reportJson": "{}",
            "userId": "u", "now": 1, "now2": 1
        });
        let preview = parse_input(PmsImportCommandKind::Preview, &preview_value).unwrap();
        assert_eq!(preview.operation(), IMPORT_PREVIEW_OPERATION);

        let balance_value = json!({
            "id": "b", "sourceKey": "s", "propertyId": "p", "guestAccountId": "g",
            "entryId": "e", "pocket": "advance", "total": 100,
            "paymentId": null, "invoiceId": null
        });
        let balance = parse_input(PmsImportCommandKind::Balance, &balance_value).unwrap();
        assert!(matches!(
            balance,
            PmsImportInput::Balance(BalanceInput {
                pocket: ImportPocket::Advance,
                ..
            })
        ));
    }

    #[test]
    fn rejects_unknown_pocket_and_short_id() {
        assert!(
            parse_input(
                PmsImportCommandKind::Balance,
                &json!({
                    "id": "", "sourceKey": "s", "propertyId": "p", "guestAccountId": "g",
                    "entryId": "e", "pocket": "advance", "total": 1,
                    "paymentId": null, "invoiceId": null
                }),
            )
            .is_err()
        );
    }
}
