//! Application query for the native assistant source read. It owns validation,
//! de-duplication and the exact response envelope; every port is mandatory and
//! SQL stays behind `AssistantSourcesRead`.
use super::domain::{
    MAX_SOURCES, AssistantSourceRef, canonical, guide_reference, invalid, valid_id,
};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

pub type AssistantSourcesFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// One bounded read: an actor's exact work scope and one canonical page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssistantSourceQuery {
    pub company_id: String,
    pub user_id: String,
    pub assignment_id: String,
    pub scope_key: String,
    pub path: String,
}

impl AssistantSourceQuery {
    pub fn new(
        company_id: &str,
        user_id: &str,
        assignment_id: &str,
        scope_key: &str,
        path: &str,
    ) -> Result<Self, BusinessError> {
        if !valid_id(company_id)
            || !valid_id(user_id)
            || !valid_id(assignment_id)
            || scope_key.is_empty()
            || scope_key.len() > 2000
            || scope_key.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(Self {
            company_id: company_id.to_owned(),
            user_id: user_id.to_owned(),
            assignment_id: assignment_id.to_owned(),
            scope_key: scope_key.to_owned(),
            path: path.to_owned(),
        })
    }
}

/// The only I/O seam. The adapter executes the owner statement and returns the
/// stored `sourceReference` rows for the canonical page, newest turn first.
pub trait AssistantSourcesRead: Send + Sync {
    fn saved<'a>(
        &'a self,
        query: &'a AssistantSourceQuery,
    ) -> AssistantSourcesFuture<'a, Vec<AssistantSourceRef>>;
}

/// Complete payload parity with the retired helper: an array of source
/// references, with the manual workflow guide as the fail-closed minimum.
pub async fn assistant_sources<R: AssistantSourcesRead + ?Sized>(
    query: &AssistantSourceQuery,
    reader: &R,
) -> Result<Value, BusinessError> {
    let path = canonical(&query.path)?;
    let scoped = AssistantSourceQuery {
        path,
        ..query.clone()
    };
    let stored = reader.saved(&scoped).await?;
    let mut seen = BTreeSet::new();
    let mut sources = Vec::new();
    for reference in stored {
        if reference.path == scoped.path && seen.insert(reference.id.clone()) {
            sources.push(reference);
        }
        if sources.len() == MAX_SOURCES {
            break;
        }
    }
    if sources.is_empty() {
        sources.push(guide_reference(&scoped.path)?);
    }
    Ok(json!({"sources": sources}))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Reader {
        rows: Vec<AssistantSourceRef>,
    }

    impl AssistantSourcesRead for Reader {
        fn saved<'a>(
            &'a self,
            _: &'a AssistantSourceQuery,
        ) -> AssistantSourcesFuture<'a, Vec<AssistantSourceRef>> {
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn reference(id: &str, path: &str) -> AssistantSourceRef {
        AssistantSourceRef {
            id: id.into(),
            title: "Today · workflow guide".into(),
            path: path.into(),
            version: "1".into(),
            source: None,
        }
    }

    #[tokio::test]
    async fn stored_sources_win_and_guide_is_the_floor() {
        let query = AssistantSourceQuery::new("co", "user", "assignment", "scope", "/home?cursor=3")
            .unwrap();
        let stored = Reader {
            rows: vec![
                reference("guide:1", "/home"),
                reference("guide:1", "/home"),
                reference("record:1", "/other"),
            ],
        };
        let value = assistant_sources(&query, &stored).await.unwrap();
        assert_eq!(value["sources"].as_array().unwrap().len(), 1);
        assert_eq!(value["sources"][0]["id"], "guide:1");
        let empty = Reader { rows: vec![] };
        let value = assistant_sources(&query, &empty).await.unwrap();
        let sources = value["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["path"], "/home");
        assert!(sources[0].get("source").is_none());
    }

    #[test]
    fn query_rejects_cross_scope_identities() {
        assert!(AssistantSourceQuery::new("co", "user", "assignment", "scope", "/home").is_ok());
        assert!(AssistantSourceQuery::new("co", "user", "assignment", "scope", "/outside").is_ok());
        assert!(AssistantSourceQuery::new("", "user", "assignment", "scope", "/home").is_err());
    }
}
