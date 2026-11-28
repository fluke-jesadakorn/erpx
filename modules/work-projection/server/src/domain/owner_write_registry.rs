//! Pure typed-kind registry for every native Work owner write. No IO, no SQL
//! execution and no authority resolution happen here: this is the owner allow
//! list and exact positional-arity fence. The owner adapter holds all SQL.
use erpx_platform_shared_server::public::BusinessError;

pub const WP_CAL_OCCURRENCE_EXCEPTION_UPSERT: &str = "wp_cal_occurrence_exception_upsert";
pub const WP_CAL_APPOINTMENT_VERSION_BUMP: &str = "wp_cal_appointment_version_bump";
pub const WP_CAL_SERIES_UNTIL: &str = "wp_cal_series_until";
pub const WP_CAL_APPOINTMENT_CLONE: &str = "wp_cal_appointment_clone";
pub const WP_CAL_SERIES_INSERT: &str = "wp_cal_series_insert";
pub const WP_CAL_ATTENDEE_CLONE: &str = "wp_cal_attendee_clone";
pub const WP_CAL_PARTICIPANT_CLONE: &str = "wp_cal_participant_clone";
pub const WP_CAL_APPOINTMENT_UPDATE: &str = "wp_cal_appointment_update";
pub const WP_CAL_ANNOUNCEMENT_DATES: &str = "wp_cal_announcement_dates";
pub const WP_CAL_APPOINTMENT_UPSERT: &str = "wp_cal_appointment_upsert";
pub const WP_CAL_ATTENDEE_DELETE: &str = "wp_cal_attendee_delete";
pub const WP_CAL_ATTENDEE_INSERT: &str = "wp_cal_attendee_insert";
pub const WP_CAL_PARTICIPANT_DELETE: &str = "wp_cal_participant_delete";
pub const WP_CAL_PARTICIPANT_INSERT: &str = "wp_cal_participant_insert";
pub const WP_CAL_SERIES_DELETE: &str = "wp_cal_series_delete";
pub const WP_CAL_SERIES_UPSERT: &str = "wp_cal_series_upsert";
pub const WP_CAL_OCCURRENCE_RESPONSE_UPSERT: &str = "wp_cal_occurrence_response_upsert";
pub const WP_CAL_ATTENDEE_RESPONSE_UPDATE: &str = "wp_cal_attendee_response_update";
pub const WP_CAL_ATTENDEE_REMINDER_UPDATE: &str = "wp_cal_attendee_reminder_update";
pub const WP_CAL_REMINDER_DELIVERY_DELETE: &str = "wp_cal_reminder_delivery_delete";
pub const WP_CAL_OCCURRENCE_EXCEPTION_CANCEL: &str = "wp_cal_occurrence_exception_cancel";
pub const WP_CAL_APPOINTMENT_STATUS: &str = "wp_cal_appointment_status";
pub const WP_CAL_ANNOUNCEMENT_UPSERT: &str = "wp_cal_announcement_upsert";
pub const WP_CAL_ANNOUNCEMENT_STATUS: &str = "wp_cal_announcement_status";
pub const WP_CAL_HISTORY_INSERT: &str = "wp_cal_history_insert";
pub const WP_REMINDER_DELIVERY_UPSERT: &str = "wp_reminder_delivery_upsert";
pub const WP_REMINDER_DELIVERY_CLAIM: &str = "wp_reminder_delivery_claim";
pub const WP_REMINDER_AUTHORIZATION_INSERT: &str = "wp_reminder_authorization_insert";
pub const WP_DAILY_PLAN_WRITE: &str = "wp_daily_plan_write";
pub const WP_DAILY_PLAN_READ: &str = "wp_daily_plan_read";
pub const WP_PAGE_LAYOUT_SAVE: &str = "wp_page_layout_save";
pub const WP_PAGE_LAYOUT_RECEIPT_SAVE: &str = "wp_page_layout_receipt_save";
pub const WP_QUICK_ACCESS_SAVE: &str = "wp_quick_access_save";
pub const WP_QUICK_ACCESS_RECEIPT_SAVE: &str = "wp_quick_access_receipt_save";
pub const WP_PROJECTION_RECEIPT: &str = "wp_projection_receipt";
pub const WP_WORK_ITEM_UPSERT: &str = "wp_work_item_upsert";
pub const WP_PROJECTION_CHECKPOINT: &str = "wp_projection_checkpoint";

const KINDS: &[(&str, usize)] = &[
    (WP_CAL_OCCURRENCE_EXCEPTION_UPSERT, 5),
    (WP_CAL_APPOINTMENT_VERSION_BUMP, 3),
    (WP_CAL_SERIES_UNTIL, 2),
    (WP_CAL_APPOINTMENT_CLONE, 11),
    (WP_CAL_SERIES_INSERT, 8),
    (WP_CAL_ATTENDEE_CLONE, 3),
    (WP_CAL_PARTICIPANT_CLONE, 3),
    (WP_CAL_APPOINTMENT_UPDATE, 8),
    (WP_CAL_ANNOUNCEMENT_DATES, 5),
    (WP_CAL_APPOINTMENT_UPSERT, 21),
    (WP_CAL_ATTENDEE_DELETE, 2),
    (WP_CAL_ATTENDEE_INSERT, 8),
    (WP_CAL_PARTICIPANT_DELETE, 2),
    (WP_CAL_PARTICIPANT_INSERT, 3),
    (WP_CAL_SERIES_DELETE, 2),
    (WP_CAL_SERIES_UPSERT, 8),
    (WP_CAL_OCCURRENCE_RESPONSE_UPSERT, 6),
    (WP_CAL_ATTENDEE_RESPONSE_UPDATE, 5),
    (WP_CAL_ATTENDEE_REMINDER_UPDATE, 4),
    (WP_CAL_REMINDER_DELIVERY_DELETE, 3),
    (WP_CAL_OCCURRENCE_EXCEPTION_CANCEL, 3),
    (WP_CAL_APPOINTMENT_STATUS, 4),
    (WP_CAL_ANNOUNCEMENT_UPSERT, 13),
    (WP_CAL_ANNOUNCEMENT_STATUS, 4),
    (WP_CAL_HISTORY_INSERT, 8),
    (WP_REMINDER_DELIVERY_UPSERT, 5),
    (WP_REMINDER_DELIVERY_CLAIM, 5),
    (WP_REMINDER_AUTHORIZATION_INSERT, 11),
    (WP_DAILY_PLAN_WRITE, 8),
    (WP_DAILY_PLAN_READ, 3),
    (WP_PAGE_LAYOUT_SAVE, 7),
    (WP_PAGE_LAYOUT_RECEIPT_SAVE, 9),
    (WP_QUICK_ACCESS_SAVE, 6),
    (WP_QUICK_ACCESS_RECEIPT_SAVE, 7),
    (WP_PROJECTION_RECEIPT, 4),
    (WP_WORK_ITEM_UPSERT, 11),
    (WP_PROJECTION_CHECKPOINT, 3),
];

pub fn supported(kind: &str) -> bool {
    KINDS.iter().any(|(name, _)| *name == kind)
}

/// Calendar commands write the Holding calendar tables, not the Work read
/// model. Composition uses this to select the same Holding pool as the native
/// calendar read router instead of the projection read-model writer.
pub fn is_calendar_write(kind: &str) -> bool {
    kind.starts_with("wp_cal_")
}

pub fn validate(kind: &str, value_count: usize) -> Result<(), BusinessError> {
    let expected = KINDS
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, arity)| *arity)
        .ok_or_else(|| {
            BusinessError::invalid("WORK_OWNER_WRITE_INVALID", "Unsupported owner write kind.")
        })?;
    if value_count != expected {
        return Err(BusinessError::invalid(
            "WORK_OWNER_WRITE_INVALID",
            "Unexpected owner write value count.",
        ));
    }
    Ok(())
}

/// Kinds whose SQL has a RETURNING clause; the reply carries the row.
pub fn returning(kind: &str) -> bool {
    matches!(
        kind,
        WP_CAL_ATTENDEE_REMINDER_UPDATE | WP_REMINDER_DELIVERY_CLAIM
    )
}

/// Owner-authored SELECT kinds served by the mounted owner-read route. The
/// reply carries the typed result row without a TypeScript-authored query.
pub fn reading(kind: &str) -> bool {
    kind == WP_DAILY_PLAN_READ
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_list_is_exact_and_arity_fenced() {
        assert!(supported(WP_CAL_APPOINTMENT_UPSERT));
        assert!(!supported("unknown"));
        assert!(validate(WP_CAL_APPOINTMENT_UPSERT, 21).is_ok());
        assert!(validate(WP_CAL_APPOINTMENT_UPSERT, 20).is_err());
        assert!(validate("unknown", 21).is_err());
    }

    #[test]
    fn calendar_writes_are_exact() {
        assert!(is_calendar_write(WP_CAL_APPOINTMENT_UPSERT));
        assert!(is_calendar_write(WP_CAL_HISTORY_INSERT));
        assert!(!is_calendar_write(WP_DAILY_PLAN_WRITE));
        assert!(!is_calendar_write(WP_WORK_ITEM_UPSERT));
    }

    #[test]
    fn returning_kinds_are_exact() {
        assert!(returning(WP_REMINDER_DELIVERY_CLAIM));
        assert!(!returning(WP_CAL_HISTORY_INSERT));
    }

    #[test]
    fn owner_read_kinds_are_exact_and_arity_fenced() {
        assert!(reading(WP_DAILY_PLAN_READ));
        assert!(supported(WP_DAILY_PLAN_READ));
        assert!(validate(WP_DAILY_PLAN_READ, 3).is_ok());
        assert!(validate(WP_DAILY_PLAN_READ, 2).is_err());
        assert!(!reading(WP_DAILY_PLAN_WRITE));
    }
}
