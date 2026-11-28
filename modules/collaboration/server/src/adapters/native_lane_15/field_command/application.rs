//! Application port and handler for the native field-command routing read.
//!
//! No SQL lives here. Composition supplies the owner query adapter; this
//! handler enforces the Company boundary and computes the routing answer.

use crate::adapters::native_lane_15::field_command::domain::{
    field_owned, invalid_request, valid_id, validate, CardSource, FieldCommandFacts,
    FieldCommandKind, FieldCommandRequest, RelationSource,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type FieldCommandFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner read port implemented by the sqlx adapter.
pub trait FieldCommandQuery: Send + Sync {
    fn relation<'a>(
        &'a self,
        id: &'a str,
        company: &'a str,
    ) -> FieldCommandFuture<'a, Option<RelationSource>>;
    fn card<'a>(
        &'a self,
        id: &'a str,
        company: &'a str,
    ) -> FieldCommandFuture<'a, Option<CardSource>>;
}

/// Validate the request, require the Company header for SQL-backed reads and
/// return the single boolean routing fact. The no-id branch only projects the
/// supplied source types, exactly as the original TypeScript did.
pub async fn load(
    query: &dyn FieldCommandQuery,
    request: &FieldCommandRequest,
    context: Option<&str>,
) -> Result<FieldCommandFacts, BusinessError> {
    validate(request)?;
    let company = request.company_id.as_str();
    if context != Some(company) || !valid_id(context.unwrap_or_default()) {
        return Err(invalid_request());
    }
    let kind = request.kind();
    let (source, target): (Option<String>, Option<String>) = match request.id.as_deref() {
        Some(id) => match kind {
            FieldCommandKind::Relation => match query.relation(id, company).await? {
                Some(row) => (Some(row.source_type), Some(row.target_type)),
                None => (None, None),
            },
            FieldCommandKind::Card => match query.card(id, company).await? {
                Some(row) => (Some(row.source_type), None),
                None => (None, None),
            },
        },
        None => (request.source_type.clone(), request.target_type.clone()),
    };
    Ok(FieldCommandFacts {
        field_owned: field_owned(kind, source.as_deref(), target.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_15::field_command::domain::unavailable;

    struct Stub(bool);

    impl FieldCommandQuery for Stub {
        fn relation<'a>(
            &'a self,
            id: &'a str,
            company: &'a str,
        ) -> FieldCommandFuture<'a, Option<RelationSource>> {
            let ok = !self.0 && id == "rel-1" && company == "c1";
            Box::pin(async move {
                if !ok {
                    return Err(unavailable());
                }
                Ok(Some(RelationSource {
                    source_type: "support_ticket".into(),
                    target_type: "field_service_order".into(),
                }))
            })
        }

        fn card<'a>(
            &'a self,
            id: &'a str,
            company: &'a str,
        ) -> FieldCommandFuture<'a, Option<CardSource>> {
            let ok = !self.0 && id == "card-1" && company == "c1";
            Box::pin(async move {
                if !ok {
                    return Err(unavailable());
                }
                Ok(Some(CardSource {
                    source_type: "support_ticket".into(),
                }))
            })
        }
    }

    #[tokio::test]
    async fn relation_target_owns_the_route() {
        let facts = load(
            &Stub(false),
            &FieldCommandRequest {
                entity: Some("relation".into()),
                id: Some("rel-1".into()),
                company_id: "c1".into(),
                ..FieldCommandRequest::default()
            },
            Some("c1"),
        )
        .await
        .expect("facts");
        assert!(facts.field_owned);
    }

    #[tokio::test]
    async fn card_source_decides_and_no_id_branch_needs_no_query() {
        let card = load(
            &Stub(false),
            &FieldCommandRequest {
                id: Some("card-1".into()),
                company_id: "c1".into(),
                ..FieldCommandRequest::default()
            },
            Some("c1"),
        )
        .await
        .expect("card");
        assert!(!card.field_owned);

        let supplied = load(
            &Stub(true),
            &FieldCommandRequest {
                entity: Some("relation".into()),
                company_id: "c1".into(),
                source_type: Some("staff_task".into()),
                target_type: Some("field_service_order".into()),
                ..FieldCommandRequest::default()
            },
            Some("c1"),
        )
        .await
        .expect("supplied");
        assert!(supplied.field_owned);
    }

    #[tokio::test]
    async fn fails_closed_on_company_mismatch_or_owner_failure() {
        let request = FieldCommandRequest {
            entity: Some("relation".into()),
            id: Some("rel-1".into()),
            company_id: "c1".into(),
            ..FieldCommandRequest::default()
        };
        assert_eq!(
            load(&Stub(false), &request, Some("c2")).await.unwrap_err(),
            invalid_request()
        );
        assert_eq!(
            load(&Stub(true), &request, Some("c1")).await.unwrap_err(),
            unavailable()
        );
    }
}
