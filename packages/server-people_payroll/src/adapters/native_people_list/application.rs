//! Query port and handler for the native people-directory list read. No SQL
//! lives here: the owner reader executes the statement and the domain owns the
//! row shape, the bounds and the value validation.

use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

use super::domain::{MAX_ROWS, PersonListRow, filter, invalid, unavailable, valid_id};

pub type ListFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// All five call-site values of the retired `readListPeopleRows` bind list.
/// The Company, branch and department are isolated context values; the level
/// and department filters are the bounded request query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListPeopleQuery {
    pub company: String,
    pub branch: Option<String>,
    pub department: Option<String>,
    pub level: Option<String>,
    pub filter_department: Option<String>,
}

impl ListPeopleQuery {
    pub fn new(
        company: &str,
        branch: Option<&str>,
        department: Option<&str>,
        level: Option<&str>,
        filter_department: Option<&str>,
    ) -> Self {
        Self {
            company: company.to_owned(),
            branch: branch.filter(|value| !value.is_empty()).map(str::to_owned),
            department: department.filter(|value| !value.is_empty()).map(str::to_owned),
            level: level.filter(|value| !value.is_empty()).map(str::to_owned),
            filter_department: filter_department
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company) {
            return Err(invalid());
        }
        for value in [&self.branch, &self.department] {
            if value.as_deref().is_some_and(|item| !valid_id(item)) {
                return Err(invalid());
            }
        }
        for value in [
            self.level.as_deref(),
            self.filter_department.as_deref(),
        ] {
            filter(value)?;
        }
        Ok(())
    }
}

/// Owner reader injected by composition. The axum handler transports only
/// bounded identities; no SQL and no TypeScript fallback is part of this port.
pub trait PeopleListRead: Send + Sync {
    fn list<'a>(&'a self, query: &'a ListPeopleQuery) -> ListFuture<'a, Vec<PersonListRow>>;
}

/// Read-only handler: validate once, dispatch to the owner reader and bound the
/// returned payload. No transaction, command id or idempotency key is involved.
pub async fn list_people<R: PeopleListRead + ?Sized>(
    reader: &R,
    query: &ListPeopleQuery,
) -> Result<Vec<PersonListRow>, BusinessError> {
    query.validate()?;
    let rows = reader.list(query).await?;
    if rows.len() > MAX_ROWS {
        return Err(unavailable());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_people_list::domain::fixture_row;
    use std::sync::{Arc, Mutex};

    struct Probe {
        seen: Arc<Mutex<Vec<ListPeopleQuery>>>,
        rows: Vec<PersonListRow>,
    }

    impl PeopleListRead for Probe {
        fn list<'a>(&'a self, query: &'a ListPeopleQuery) -> ListFuture<'a, Vec<PersonListRow>> {
            let seen = self.seen.clone();
            let query = query.clone();
            let rows = self.rows.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(query);
                Ok(rows)
            })
        }
    }

    #[tokio::test]
    async fn handler_validates_and_dispatches_once() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Probe {
            seen: seen.clone(),
            rows: vec![fixture_row()],
        };
        let query = ListPeopleQuery::new(
            "acme-co",
            Some("branch-1"),
            Some("dept-1"),
            Some("unset"),
            Some("dept-2"),
        );
        let rows = list_people(&reader, &query).await.unwrap();
        assert_eq!(rows, vec![fixture_row()]);
        assert_eq!(seen.lock().unwrap().as_slice(), &[query]);
    }

    #[tokio::test]
    async fn handler_fails_closed_before_reaching_the_reader() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Probe {
            seen: seen.clone(),
            rows: vec![],
        };
        for query in [
            ListPeopleQuery::new("bad company", None, None, None, None),
            ListPeopleQuery::new("acme-co", None, None, Some("drop table"), None),
            ListPeopleQuery::new("acme-co", Some("../other"), None, None, None),
        ] {
            assert!(list_people(&reader, &query).await.is_err());
        }
        assert!(seen.lock().unwrap().is_empty());
    }
}
