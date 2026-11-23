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

/// Exact, reviewed customer-notification outbox labels. The native projection
/// handler already owns their queue/retry side effects (`project_event`), and
/// the native `sales-notification-worker` owns delivery; only the supervisor
/// claim predicate kept them with the TypeScript `runOne` loop. Both this
/// partition and composition must use these literals so the two predicates
/// cannot drift and no event is claimed by both supervisors.
pub const CUSTOMER_NOTIFICATION_QUEUE: &str = "commerce.customer-notification.queue";
pub const CUSTOMER_NOTIFICATION_RETRY: &str = "commerce.customer-notification.retry";
const CUSTOMER_NOTIFICATION_LABELS: [&str; 2] =
    [CUSTOMER_NOTIFICATION_QUEUE, CUSTOMER_NOTIFICATION_RETRY];

/// A projection family with a native implementation ready for the supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeFamily {
    Task,
    Routine,
    Leave,
    Overtime,
    Payroll,
    PmsSocial,
    CustomerNotification,
}

impl NativeFamily {
    /// Every native-ready family, in stable predicate order.
    pub const ALL: [NativeFamily; 7] = [
        Self::Task,
        Self::Routine,
        Self::Leave,
        Self::Overtime,
        Self::Payroll,
        Self::PmsSocial,
        Self::CustomerNotification,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Routine => "routine",
            Self::Leave => "leave",
            Self::Overtime => "overtime",
            Self::Payroll => "people.payroll",
            Self::PmsSocial => "pms.social",
            Self::CustomerNotification => "commerce.customer-notification",
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
/// exact reviewed labels supplied by the workflow owner, not a prefix. The
/// customer-notification queue and retry labels are exact and static; the
/// sibling reconcile label is deliberately not native-ready and stays with
/// the TypeScript loop.
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
    if CUSTOMER_NOTIFICATION_LABELS.contains(&event_type) {
        return FamilyClass::Native(NativeFamily::CustomerNotification);
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
/// label is rejected so request data can never widen the claim set. This is the
/// canonical family clause: composition must mount it verbatim as its `owned`
/// predicate and apply the quarantine exclusion around it, so the native
/// supervisor and the TypeScript `runOne` loop never claim the same family.
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
    parts.push(format!(
        "e.event_type IN ('{}','{}')",
        CUSTOMER_NOTIFICATION_QUEUE, CUSTOMER_NOTIFICATION_RETRY
    ));
    Ok(format!("({})", parts.join(" OR ")))
}
