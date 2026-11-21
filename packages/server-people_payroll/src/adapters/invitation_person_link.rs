use crate::{
    adapters::identity_facts::PgPeopleAuthorityReader,
    application::invitation_person_link::InvitationPersonLinkPort,
    domain::invitation_person_link::{
        InvitationPersonLinkRequest, InvitationPersonLinkWitness, invitation_person_link_plan,
        link_unavailable,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

pub struct PgInvitationPersonLinker<'c> {
    connection: &'c mut PgConnection,
}
impl<'c> PgInvitationPersonLinker<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}
/// Acquire before the first source snapshot in the caller's write UoW. Inserts
/// and changes to the correlated engagement/legacy employee link cannot race.
pub async fn lock_invitation_person_link_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_LINK_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE hr_people,hr_engagements,employees IN SHARE ROW EXCLUSIVE MODE")
        .execute(connection)
        .await?;
    Ok(())
}
impl InvitationPersonLinkPort for PgInvitationPersonLinker<'_> {
    type Error = PgTransactionError<BusinessError>;
    async fn link_invited_person(
        &mut self,
        request: &InvitationPersonLinkRequest,
    ) -> Result<InvitationPersonLinkWitness, Self::Error> {
        lock_invitation_person_link_sources(self.connection).await?;
        // Materialize a tuple fence, so an earlier SERIALIZABLE snapshot raises
        // 40001 after a concurrent person update instead of linking stale data.
        let locked: Option<String> =
            sqlx::query_scalar("SELECT id FROM hr_people WHERE id=$1 AND company_id=$2 FOR UPDATE")
                .bind(request.person_id.as_str())
                .bind(request.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
        if locked.is_none() {
            return Err(link_unavailable().into());
        }
        let engagement: Option<String> = sqlx::query_scalar("SELECT id FROM hr_engagements WHERE id=$1 AND person_id=$2 AND company_id=$3 FOR SHARE")
            .bind(request.engagement_id.as_str()).bind(request.person_id.as_str()).bind(request.company_id.as_str()).fetch_optional(&mut *self.connection).await?;
        if engagement.is_none() {
            return Err(link_unavailable().into());
        }
        let current = PgPeopleAuthorityReader::new(&mut *self.connection)
            .person_subject(&request.company_id, &request.person_id)
            .await?
            .ok_or_else(link_unavailable)?;
        let witness = invitation_person_link_plan(request, &current)?;
        let conflicting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM hr_people WHERE user_id=$1 AND id<>$2) OR EXISTS(SELECT 1 FROM employees WHERE user_id=$1 AND (person_id IS DISTINCT FROM $2 OR company_id<>$3)) OR EXISTS(SELECT 1 FROM employees WHERE person_id=$2 AND (company_id<>$3 OR user_id IS NOT NULL AND user_id<>$1))")
            .bind(request.user_id.as_str()).bind(request.person_id.as_str()).bind(request.company_id.as_str()).fetch_one(&mut *self.connection).await?;
        if conflicting {
            return Err(link_unavailable().into());
        }
        if witness.changed {
            let affected = sqlx::query("UPDATE hr_people SET user_id=$3,version=version+1,updated_at=$4 WHERE id=$1 AND company_id=$2 AND version=$5 AND user_id IS NULL AND status='active' AND merged_into_id IS NULL")
                .bind(request.person_id.as_str()).bind(request.company_id.as_str()).bind(request.user_id.as_str()).bind(request.now.get()).bind(request.expected_person_version).execute(&mut *self.connection).await?.rows_affected();
            if affected != 1 {
                return Err(link_unavailable().into());
            }
        }
        sqlx::query("UPDATE employees SET user_id=$3 WHERE person_id=$1 AND company_id=$2 AND user_id IS DISTINCT FROM $3")
            .bind(request.person_id.as_str()).bind(request.company_id.as_str()).bind(request.user_id.as_str()).execute(&mut *self.connection).await?;
        Ok(witness)
    }
}
