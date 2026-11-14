use crate::domain::identity_facts::PersonAuthorityFacts;
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis, UserId,
};

/// Trusted Identity command input; this is neither a browser DTO nor authority.
#[derive(Clone, Debug)]
pub struct InvitationPersonLinkRequest {
    pub company_id: CompanyId,
    pub person_id: RecordId,
    pub user_id: UserId,
    pub expected_person_version: i64,
    pub engagement_id: RecordId,
    pub expected_engagement_version: i64,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub now: TimestampMillis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvitationPersonLinkWitness {
    pub person_id: RecordId,
    pub user_id: UserId,
    pub person_version: i64,
    pub changed: bool,
}
pub fn invitation_person_link_plan(
    request: &InvitationPersonLinkRequest,
    current: &PersonAuthorityFacts,
) -> Result<InvitationPersonLinkWitness, BusinessError> {
    if current.company_id != request.company_id
        || current.person_id != request.person_id
        || request.expected_person_version < 1
        || request.expected_engagement_version < 1
        || !current.active_hr_subject()
        || current
            .user_id
            .as_ref()
            .is_some_and(|user| user != &request.user_id)
        || !current.active_engagements.iter().any(|engagement| {
            engagement.id == request.engagement_id
                && engagement.version == request.expected_engagement_version
                && request
                    .branch_id
                    .as_ref()
                    .is_none_or(|branch| engagement.branch_id.as_ref() == Some(branch))
                && request
                    .department_id
                    .as_ref()
                    .is_none_or(|department| engagement.department_id.as_ref() == Some(department))
        })
    {
        return Err(link_unavailable());
    }
    let changed = current.user_id.is_none();
    if changed && current.version != request.expected_person_version
        || !changed && current.version < request.expected_person_version
    {
        return Err(link_unavailable());
    }
    let version = if changed {
        current
            .version
            .checked_add(1)
            .ok_or_else(link_unavailable)?
    } else {
        current.version
    };
    Ok(InvitationPersonLinkWitness {
        person_id: current.person_id.clone(),
        user_id: request.user_id.clone(),
        person_version: version,
        changed,
    })
}
pub(crate) fn link_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVITATION_PERSON_CHANGED",
        "The invited person's active engagement or account link changed.",
    )
}
