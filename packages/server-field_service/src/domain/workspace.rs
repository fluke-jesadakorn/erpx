//! Pure Service workspace policy; source authority stays in owner witnesses.
#[derive(Clone, Copy)]
pub struct WorkspaceRights {
    pub can_write: bool,
    pub can_review: bool,
    pub agreement_new: bool,
    pub agreement_settle: bool,
    pub order_new: bool,
    pub order_settle: bool,
    pub parts_new: bool,
    pub parts_settle: bool,
}
pub struct AgreementWorkspaceFacts<'a> {
    pub reviewer_eligible: bool,
    pub same_person_db: bool,
    pub same_person_actor: bool,
    pub has_asset: bool,
    pub status: &'a str,
    pub start_date: &'a str,
    pub end_date: &'a str,
    pub today: &'a str,
}
pub struct AgreementWorkspaceFlags {
    pub can_review: i64,
    pub can_activate: i64,
    pub can_reject: i64,
    pub can_retire: i64,
    pub can_request: i64,
}
pub fn agreement_workspace_flags(
    r: WorkspaceRights,
    f: AgreementWorkspaceFacts<'_>,
) -> AgreementWorkspaceFlags {
    let raw = f.reviewer_eligible && !f.same_person_db;
    let review = r.can_write && r.can_review && raw && !f.same_person_actor;
    let request = f.status == "active"
        && f.start_date <= f.today
        && f.end_date >= f.today
        && r.order_new
        && (!f.has_asset || r.parts_new);
    AgreementWorkspaceFlags {
        can_review: i64::from(raw),
        can_activate: i64::from(review && r.agreement_new && (!f.has_asset || r.parts_new)),
        can_reject: i64::from(review && r.agreement_settle && (!f.has_asset || r.parts_settle)),
        can_retire: i64::from(review && r.agreement_settle && (!f.has_asset || r.parts_settle)),
        can_request: i64::from(request),
    }
}
pub struct OrderWorkspaceFacts {
    pub actor_is_technician: bool,
    pub technician_eligible: bool,
    pub reviewer_eligible: bool,
    pub independent_billing: bool,
    pub actor_same_person_as_technician: bool,
    pub has_asset: bool,
}
pub struct OrderWorkspaceFlags {
    pub can_work: i64,
    pub can_release_billing: i64,
}
pub fn order_workspace_flags(r: WorkspaceRights, f: OrderWorkspaceFacts) -> OrderWorkspaceFlags {
    let work = r.can_write
        && r.order_settle
        && ((r.can_review && f.reviewer_eligible)
            || (f.actor_is_technician && f.technician_eligible));
    let asset = !f.has_asset || r.parts_settle;
    OrderWorkspaceFlags {
        can_work: i64::from(work && asset),
        can_release_billing: i64::from(
            work && f.reviewer_eligible
                && !f.actor_same_person_as_technician
                && f.independent_billing
                && asset,
        ),
    }
}
pub struct WorkspaceFlags {
    pub can_write: bool,
    pub can_create_agreement: bool,
    pub can_create_order: bool,
    pub can_use_parts: bool,
    pub can_use_asset_references: bool,
}
pub fn workspace_flags(r: WorkspaceRights) -> WorkspaceFlags {
    WorkspaceFlags {
        can_write: r.can_write && (r.agreement_settle || r.order_settle),
        can_create_agreement: r.can_write && r.agreement_new,
        can_create_order: r.can_write && r.order_new,
        can_use_parts: r.can_write && r.order_settle && r.parts_settle,
        can_use_asset_references: r.can_write && r.parts_new,
    }
}
