//! PostgreSQL owner read for the reserved company-onboarding slot.
//!
//! `beta_slots` is Manage ERPX control-plane data. The caller owns the retained
//! transaction; this adapter never opens, commits or retries one.
use crate::application::onboarding::{OnboardingViewReadPort, onboarding_view};
use crate::domain::onboarding::{CompanySlot, OnboardingActor, OnboardingView};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, UserId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgOnboardingViewRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgOnboardingViewRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "ONBOARDING_SOURCE_INVALID",
        "The reserved company slot is unavailable. Retry the current onboarding state.",
    )
}

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("ONBOARDING_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("ONBOARDING_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

impl OnboardingViewReadPort for PgOnboardingViewRead<'_> {
    type Error = Error;

    async fn owner_slot(&mut self, user_id: &UserId) -> Result<Option<CompanySlot>, Self::Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=$1",
        )
        .bind(user_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let slug = row
            .try_get::<String, _>("slug")
            .map_err(|_| source_invalid())?;
        let company_name = row
            .try_get::<String, _>("company_name")
            .map_err(|_| source_invalid())?;
        let branch_name = row
            .try_get::<String, _>("branch_name")
            .map_err(|_| source_invalid())?;
        let state = row
            .try_get::<String, _>("state")
            .map_err(|_| source_invalid())?;
        CompanySlot::new(slug, company_name, branch_name, &state)
            .map(Some)
            .map_err(Into::into)
    }
}

/// Read the current verified owner's onboarding slot on the caller's retained
/// serializable control-plane transaction.
pub async fn read_onboarding_view(
    connection: &mut PgConnection,
    actor: &OnboardingActor,
    enabled: bool,
) -> Result<OnboardingView, Error> {
    let mut reader = PgOnboardingViewRead::new(connection);
    onboarding_view(actor, enabled, &mut reader).await
}
