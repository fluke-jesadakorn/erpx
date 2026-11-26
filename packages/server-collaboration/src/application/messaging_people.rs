//! Application port and projection for the Messaging people picker.
//!
//! The port is read-only and owns no SQL. One adapter implements it with sqlx;
//! composition binds the owner reader for each registered Holding.

use crate::domain::messaging_people::{PeopleQuery, Person, PersonRow};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type MessagingPeopleFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Active accounts in one Company, optionally narrowed to one team. The
/// Company and clock are supplied by composition from the authenticated
/// Holding selection, never by the HTTP caller.
pub trait MessagingPeopleRead: Send + Sync {
    fn active<'a>(
        &'a self,
        company_id: &'a str,
        now: i64,
        viewer_id: &'a str,
        team_id: Option<&'a str>,
    ) -> MessagingPeopleFuture<'a, Vec<PersonRow>>;
}

impl MessagingPeopleRead for std::sync::Arc<dyn MessagingPeopleRead> {
    fn active<'a>(
        &'a self,
        company_id: &'a str,
        now: i64,
        viewer_id: &'a str,
        team_id: Option<&'a str>,
    ) -> MessagingPeopleFuture<'a, Vec<PersonRow>> {
        MessagingPeopleRead::active(self.as_ref(), company_id, now, viewer_id, team_id)
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "MESSAGING_PEOPLE_READ_UNAVAILABLE",
        "The native Messaging people reader is unavailable.",
    )
}

/// Owner service: validate once, read the complete owner rowset, then apply
/// the retained case-insensitive `name email` filter and 100-item page.
pub struct PeopleService<R: MessagingPeopleRead> {
    reader: R,
}

impl<R: MessagingPeopleRead> PeopleService<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub async fn read(
        &self,
        company_id: &str,
        now: i64,
        viewer_id: &str,
        search: Option<&str>,
        team_id: Option<&str>,
    ) -> Result<Vec<Person>, BusinessError> {
        let query = PeopleQuery::new(search, team_id)?;
        let rows = self
            .reader
            .active(company_id, now, viewer_id, query.team_id.as_deref())
            .await
            .map_err(|_| unavailable())?;
        Ok(project(rows, &query))
    }
}

/// Exact retained projection. The TypeScript authority resolves account names
/// ordered by `name,id`, filters `${name} ${email}` case-insensitively and
/// slices the first 100 rows; nothing else is added or dropped.
pub fn project(rows: Vec<PersonRow>, query: &PeopleQuery) -> Vec<Person> {
    let needle = query.search.to_lowercase();
    rows.into_iter()
        .filter(|row| {
            needle.is_empty()
                || format!("{} {}", row.name, row.email)
                    .to_lowercase()
                    .contains(&needle)
        })
        .take(100)
        .map(|row| Person {
            id: row.user_id,
            name: row.name,
            email: row.email,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, name: &str, email: &str) -> PersonRow {
        PersonRow {
            user_id: id.into(),
            name: name.into(),
            email: email.into(),
        }
    }

    #[test]
    fn projection_matches_the_retained_account_name_payload() {
        let rows = vec![
            row("u2", "Bob", "bob@example.test"),
            row("u1", "Ada", "ada@example.test"),
        ];
        let all = project(rows.clone(), &PeopleQuery::new(None, None).unwrap());
        assert_eq!(all[0].id, "u2");
        assert_eq!(all[0].email, "bob@example.test");
        let filtered = project(rows, &PeopleQuery::new(Some("ADA"), None).unwrap());
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "u1");
    }

    #[test]
    fn page_is_bounded_to_one_hundred() {
        let rows = (0..120)
            .map(|i| row(&format!("u{i}"), &format!("Name {i}"), "x@example.test"))
            .collect();
        assert_eq!(project(rows, &PeopleQuery::new(None, None).unwrap()).len(), 100);
    }
}
