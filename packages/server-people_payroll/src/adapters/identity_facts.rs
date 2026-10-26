use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

use crate::application::identity_facts::PeopleAuthorityReadPort;
use crate::domain::identity_facts::{
    EngagementAuthorityWitness, PersonAuthorityFacts, invalid_source,
};

pub struct PgPeopleAuthorityReader<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPeopleAuthorityReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    fn transaction(&self) -> Result<(), PgTransactionError<BusinessError>> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
    async fn complete(
        &mut self,
        row: PgRow,
    ) -> Result<PersonAuthorityFacts, PgTransactionError<BusinessError>> {
        let company: String = row.try_get("company_id")?;
        let person: String = row.try_get("id")?;
        let user: Option<String> = row.try_get("user_id")?;
        let merged: Option<String> = row.try_get("merged_into_id")?;
        let engagements = sqlx::query("SELECT id,version,branch_id,department_id FROM hr_engagements WHERE company_id=$1 AND person_id=$2 AND status='active' ORDER BY id")
            .bind(&company).bind(&person).fetch_all(&mut *self.connection).await?;
        let mut witnesses = Vec::with_capacity(engagements.len());
        for row in engagements {
            witnesses.push(EngagementAuthorityWitness {
                id: RecordId::new(row.try_get::<String, _>("id")?)
                    .map_err(|_| PgTransactionError::Business(invalid_source()))?,
                version: row.try_get("version")?,
                branch_id: row
                    .try_get::<Option<String>, _>("branch_id")?
                    .map(RecordId::new)
                    .transpose()
                    .map_err(|_| PgTransactionError::Business(invalid_source()))?,
                department_id: row
                    .try_get::<Option<String>, _>("department_id")?
                    .map(RecordId::new)
                    .transpose()
                    .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            });
        }
        let facts = PersonAuthorityFacts {
            company_id: CompanyId::new(company)
                .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            person_id: RecordId::new(person)
                .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            user_id: user
                .map(UserId::new)
                .transpose()
                .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            status: row.try_get("status")?,
            merged_into_id: merged
                .map(RecordId::new)
                .transpose()
                .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            version: row.try_get("version")?,
            active_engagements: witnesses,
        };
        facts.validate().map_err(PgTransactionError::Business)?;
        Ok(facts)
    }
    pub async fn person_for_user(
        &mut self,
        company: &CompanyId,
        user: &UserId,
    ) -> Result<Option<PersonAuthorityFacts>, PgTransactionError<BusinessError>> {
        self.transaction()?;
        let rows = sqlx::query("SELECT id,company_id,user_id,status,merged_into_id,version FROM hr_people WHERE company_id=$1 AND user_id=$2 AND merged_into_id IS NULL ORDER BY id LIMIT 2")
            .bind(company.as_str()).bind(user.as_str()).fetch_all(&mut *self.connection).await?;
        if rows.len() > 1 {
            return Err(PgTransactionError::Business(invalid_source()));
        }
        match rows.into_iter().next() {
            Some(row) => self.complete(row).await.map(Some),
            None => Ok(None),
        }
    }
    pub async fn person_subject(
        &mut self,
        company: &CompanyId,
        person: &RecordId,
    ) -> Result<Option<PersonAuthorityFacts>, PgTransactionError<BusinessError>> {
        self.transaction()?;
        let row = sqlx::query("SELECT id,company_id,user_id,status,merged_into_id,version FROM hr_people WHERE company_id=$1 AND id=$2")
            .bind(company.as_str()).bind(person.as_str()).fetch_optional(&mut *self.connection).await?;
        match row {
            Some(row) => self.complete(row).await.map(Some),
            None => Ok(None),
        }
    }
}

impl PeopleAuthorityReadPort for PgPeopleAuthorityReader<'_> {
    type Error = PgTransactionError<BusinessError>;
    async fn person_for_user(
        &mut self,
        company: &CompanyId,
        user: &UserId,
    ) -> Result<Option<PersonAuthorityFacts>, Self::Error> {
        PgPeopleAuthorityReader::person_for_user(self, company, user).await
    }
    async fn person_subject(
        &mut self,
        company: &CompanyId,
        person: &RecordId,
    ) -> Result<Option<PersonAuthorityFacts>, Self::Error> {
        PgPeopleAuthorityReader::person_subject(self, company, person).await
    }
}

/// Before the first authority snapshot, retain the same People source set used by queued commands.
pub async fn lock_people_authority_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE hr_people,hr_engagements,people_access_cutoffs,people_lifecycle_cases,people_lifecycle_grants,people_package_revisions,people_applied_packages,people_provision_operations,people_provision_grants IN SHARE MODE")
        .execute(connection).await?;
    Ok(())
}
