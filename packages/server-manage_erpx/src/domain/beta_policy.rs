use erpx_server_shared::public::CompanyId;

/// Product admission consumes this fact together with current IAM authority.
/// The presence of a Company beta policy never grants an IAM permission.
#[derive(Clone, Debug)]
pub struct CompanyBetaPolicy {
    pub company_id: CompanyId,
    pub enabled: bool,
}
