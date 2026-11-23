//! Canonical partition of durable outbox event families.
//!
//! Reconciles `inventory.runtime`/`audit.runtime`: the native supervisor owns
//! the families below; every other family stays with the retained TypeScript
//! `runOne` loop until its own native port lands. The predicate builder emits
//! the static SQL fragment used by the supervisor's claim - no request, user
//! or provider data is ever interpolated, and lease/retry/failure mechanics
//! are untouched.
use std::fmt;

const MAX_EVENT_TYPE_BYTES: usize = 256;

/// A projection family with a native implementation ready for the supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeFamily {
    Task,
    Routine,
    Leave,
    Overtime,
    Payroll,
    PmsSocial,
}

impl NativeFamily {
    /// Every native-ready family, in stable predicate order.
    pub const ALL: [NativeFamily; 6] = [
        Self::Task,
        Self::Routine,
        Self::Leave,
        Self::Overtime,
        Self::Payroll,
        Self::PmsSocial,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Routine => "routine",
            Self::Leave => "leave",
            Self::Overtime => "overtime",
            Self::Payroll => "people.payroll",
            Self::PmsSocial => "pms.social",
        }
    }
}

impl fmt::Display for NativeFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Classification of one durable event type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FamilyClass {
    Native(NativeFamily),
    /// Retained by the TypeScript runOne loop until a native port exists, or a
    /// quarantined family owned by a different native worker.
    RetainedTs,
}

impl FamilyClass {
    pub const fn is_native(self) -> bool {
        matches!(self, Self::Native(_))
    }
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_EVENT_TYPE_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Classify a durable `event_type`. Prefix families match the reviewed claim
/// predicate exactly, including the leave-policy and overtime-eligibility
/// exclusions retained by the TypeScript projector. PMS social events are
/// exact reviewed labels supplied by the workflow owner, not a prefix.
pub fn classify(event_type: &str, pms_social: &[&str]) -> FamilyClass {
    if !valid_label(event_type) {
        return FamilyClass::RetainedTs;
    }
    if event_type.starts_with("task.") {
        return FamilyClass::Native(NativeFamily::Task);
    }
    if event_type == "routine.run" {
        return FamilyClass::Native(NativeFamily::Routine);
    }
    if event_type.starts_with("leave.") && !event_type.starts_with("leave.policy.") {
        return FamilyClass::Native(NativeFamily::Leave);
    }
    if event_type.starts_with("overtime.") && event_type != "overtime.eligibility" {
        return FamilyClass::Native(NativeFamily::Overtime);
    }
    if event_type.starts_with("people.payroll.") {
        return FamilyClass::Native(NativeFamily::Payroll);
    }
    if pms_social.iter().any(|known| *known == event_type) {
        return FamilyClass::Native(NativeFamily::PmsSocial);
    }
    FamilyClass::RetainedTs
}

/// A rejected predicate input. It never carries SQL, labels or provider text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FamilyPredicateError {
    pub code: &'static str,
}

impl fmt::Display for FamilyPredicateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for FamilyPredicateError {}

/// Build the static `owned` claim predicate for the native supervisor.
///
/// `pms_social` are the exact, reviewed event labels from the workflow owner.
/// Each is validated as a safe SQL literal; an empty, oversized or punctuated
/// label is rejected so request data can never widen the claim set. The result
/// is byte-for-byte the family clause of the composition-owned predicate, with
/// the quarantine exclusion applied by the caller.
pub fn owned_predicate(pms_social: &[&str]) -> Result<String, FamilyPredicateError> {
    for label in pms_social {
        if !valid_label(label) {
            return Err(FamilyPredicateError {
                code: "OUTBOX_FAMILY_PREDICATE_INVALID",
            });
        }
    }
    let mut parts = vec![
        "e.event_type LIKE 'task.%'".to_owned(),
        "e.event_type = 'routine.run'".to_owned(),
        "(e.event_type LIKE 'leave.%' AND e.event_type NOT LIKE 'leave.policy.%')".to_owned(),
        "(e.event_type LIKE 'overtime.%' AND e.event_type <> 'overtime.eligibility')".to_owned(),
        "e.event_type LIKE 'people.payroll.%'".to_owned(),
    ];
    if !pms_social.is_empty() {
        let labels = pms_social
            .iter()
            .map(|label| format!("'{label}'"))
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("e.event_type IN ({labels})"));
    }
    Ok(format!("({})", parts.join(" OR ")))
}
