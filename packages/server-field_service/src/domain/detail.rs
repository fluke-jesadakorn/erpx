//! Pure access flag for the existing visit-file presentation.
pub fn can_edit_visit_files(
    can_write: bool,
    visit_settle: bool,
    parts_settle: bool,
    has_asset: bool,
    status: &str,
) -> bool {
    can_write
        && visit_settle
        && (!has_asset || parts_settle)
        && !matches!(status, "billed" | "cancelled")
}
