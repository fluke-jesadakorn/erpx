use std::future::Future;

use crate::domain::identity_facts::PersonAuthorityFacts;
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, UserId};

pub trait PeopleAuthorityReadPort {
    type Error: From<BusinessError> + Send;
    fn person_for_user(
        &mut self,
        company: &CompanyId,
        user: &UserId,
    ) -> impl Future<Output = Result<Option<PersonAuthorityFacts>, Self::Error>> + Send;
    fn person_subject(
        &mut self,
        company: &CompanyId,
        person: &RecordId,
    ) -> impl Future<Output = Result<Option<PersonAuthorityFacts>, Self::Error>> + Send;
}
