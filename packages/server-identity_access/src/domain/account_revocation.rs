use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId, RecordId, UserId};

/// A retired child proven by the caller's locked Control session family.
/// These identifiers contain no cookie/credential and convey no authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetiredAccountChild {
    pub session_id: RecordId,
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub environment: String,
}
impl RetiredAccountChild {
    pub(crate) fn from_binding(
        session: String,
        user: String,
        audience: &str,
        environment: &str,
    ) -> Result<Self, BusinessError> {
        let parts: Vec<_> = audience.splitn(4, ':').collect();
        if environment != "development"
            || parts.len() != 4
            || parts[0] != "holding"
            || parts[1] != environment
            || parts[3].is_empty()
            || (parts[3].contains(':') && !super::account_auth::contributor_holding_host(parts[3]))
        {
            return Err(reentry_required());
        }
        Ok(Self {
            session_id: RecordId::new(session)?,
            user_id: UserId::new(user)?,
            holding_id: HoldingId::new(parts[2])?,
            environment: environment.into(),
        })
    }
}

/// Current, locked Control metadata. Only trusted composition resolves its
/// writer binding into a pool; a request never supplies a database or URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountRevocationTarget {
    pub holding_id: HoldingId,
    pub write_binding: String,
}
pub fn reentry_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DEVICE_ACCOUNT_REENTRY_REQUIRED",
        "Work context revocation could not finish. Re-enter through Account before continuing.",
    )
}
