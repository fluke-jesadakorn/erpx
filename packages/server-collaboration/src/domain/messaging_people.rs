//! Pure owner types for the Messaging people picker.
//!
//! No I/O and no SQL here: the adapter returns active account rows and the
//! application projects them into the exact retained payload.

use erpx_server_shared::public::BusinessError;
use serde::Serialize;

/// One active account row read by the owner SQL adapter.
#[derive(Clone, Debug)]
pub struct PersonRow {
    pub user_id: String,
    pub name: String,
    pub email: String,
}

/// The retained `{id,name,email}` picker item. `email` is always a string:
/// the TypeScript authority reads it from the Control accounts projection.
#[derive(Clone, Debug, Serialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub email: String,
}

/// Bounded, owner-validated selection. Callers cannot grow either field past
/// the limits the retained route applied before it reached the repository.
#[derive(Clone, Debug)]
pub struct PeopleQuery {
    pub search: String,
    pub team_id: Option<String>,
}

impl PeopleQuery {
    pub fn new(search: Option<&str>, team_id: Option<&str>) -> Result<Self, BusinessError> {
        let search = search.unwrap_or("").trim();
        let team_id = team_id.filter(|value| !value.is_empty());
        if search.encode_utf16().count() > 160 {
            return Err(BusinessError::invalid(
                "MESSAGING_PEOPLE_QUERY_INVALID",
                "Shorten the people search.",
            ));
        }
        if team_id.is_some_and(|value| {
            value.len() > 200 || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.:+".contains(&b))
        }) {
            return Err(BusinessError::invalid(
                "MESSAGING_PEOPLE_TEAM_INVALID",
                "Select one valid team.",
            ));
        }
        Ok(Self {
            search: search.to_owned(),
            team_id: team_id.map(str::to_owned),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_is_bounded_and_normalised() {
        let query = PeopleQuery::new(Some("  Ada  "), Some("team-1")).unwrap();
        assert_eq!(query.search, "Ada");
        assert_eq!(query.team_id.as_deref(), Some("team-1"));
        assert!(PeopleQuery::new(Some(&"x".repeat(161)), None).is_err());
        assert!(PeopleQuery::new(None, Some("bad team")).is_err());
    }
}
