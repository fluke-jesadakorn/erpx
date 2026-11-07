//! Pure comparison for an already selected synchronous command.
use crate::{
    application::command_metadata::project, domain::effective_authority::AuthorizedWorkContext,
};
use erpx_server_shared::public::BusinessError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServicePartsAuthorityPurpose {
    Read,
    Write,
}
pub(crate) fn required_permission(
    context: &AuthorizedWorkContext,
    purpose: ServicePartsAuthorityPurpose,
) -> Result<&'static str, BusinessError> {
    context.require_permission("finance.read")?;
    match purpose {
        ServicePartsAuthorityPurpose::Write => {
            context.require_permission("finance.write")?;
            Ok("finance.write")
        }
        ServicePartsAuthorityPurpose::Read if context.has_permission("finance.write") => {
            Ok("finance.write")
        }
        ServicePartsAuthorityPurpose::Read => {
            context.require_permission("finance.approve")?;
            Ok("finance.approve")
        }
    }
}
pub(crate) fn same_selected_command(
    a: &AuthorizedWorkContext,
    b: &AuthorizedWorkContext,
) -> Result<bool, BusinessError> {
    let x = a.selection();
    let y = b.selection();
    let p = x.assignment();
    let q = y.assignment();
    let w = x.workspace();
    let z = y.workspace();
    Ok(a.holding_id() == b.holding_id()
        && a.user_id() == b.user_id()
        && a.session_id() == b.session_id()
        && w.company_id == z.company_id
        && w.branch_id == z.branch_id
        && w.department_id == z.department_id
        && w.branch_slug == z.branch_slug
        && w.department_slug == z.department_slug
        && p.id == q.id
        && p.user_id == q.user_id
        && p.company_id == q.company_id
        && p.role == q.role
        && p.modules == q.modules
        && p.scope_kind == q.scope_kind
        && p.scope_node_id == q.scope_node_id
        && p.version == q.version
        && p.valid_from == q.valid_from
        && p.valid_until == q.valid_until
        && p.revoked == q.revoked
        && x.revision() == y.revision()
        && x.role_revision() == y.role_revision()
        && x.expires_at() == y.expires_at()
        && a.expires_at() == b.expires_at()
        && a.modules() == b.modules()
        && project(a, None)? == project(b, None)?)
}
