//! Native auxiliary Legal reads. The owner transaction remains in the adapter.
//!
//! Authorization and input bounds are decided here so a Postgres adapter can
//! only bind the already-validated scope. Composition supplies the current
//! Account session, selected Company/branch/department and the serializable
//! Holding transaction; this module never opens its own.
use crate::domain::auxiliary::{
    LegalAuxiliaryActor, LegalAuxiliaryRead, authorize_read, bangkok_date, parse_as_of,
    parse_identifier,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock};
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type LegalAuxiliaryError = PgTransactionError<BusinessError>;
pub type LegalAuxiliaryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, LegalAuxiliaryError>> + Send + 'a>>;

/// Every method receives the already-authorized actor and re-applies the same
/// Company/branch/department/membership predicate in its own SQL. A broad role
/// never substitutes for confidential membership.
pub trait LegalAuxiliaryReadPorts {
    fn workspace<'a>(&'a mut self, actor: &'a LegalAuxiliaryActor)
    -> LegalAuxiliaryFuture<'a, Value>;
    fn reports<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        now: i64,
    ) -> LegalAuxiliaryFuture<'a, Value>;
    fn retention<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: Option<&'a str>,
    ) -> LegalAuxiliaryFuture<'a, Value>;
    fn governance<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: Option<&'a str>,
    ) -> LegalAuxiliaryFuture<'a, Value>;
    fn law_corpus<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        rule_id: Option<&'a str>,
    ) -> LegalAuxiliaryFuture<'a, Value>;
    fn knowledge<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        query: &'a str,
        as_of: &'a str,
    ) -> LegalAuxiliaryFuture<'a, Value>;
    fn law_citation<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        chunk_id: &'a str,
        as_of: &'a str,
    ) -> LegalAuxiliaryFuture<'a, Value>;
    fn references<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        item_id: &'a str,
    ) -> LegalAuxiliaryFuture<'a, Value>;
    fn trust<'a>(&'a mut self, actor: &'a LegalAuxiliaryActor)
    -> LegalAuxiliaryFuture<'a, Value>;
    fn saved_views<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
    ) -> LegalAuxiliaryFuture<'a, Value>;
    fn ai_history<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        item_id: &'a str,
    ) -> LegalAuxiliaryFuture<'a, Value>;
}

/// The single read entry point used by the HTTP adapter and composition.
pub async fn read_auxiliary<P: LegalAuxiliaryReadPorts>(
    ports: &mut P,
    actor: &LegalAuxiliaryActor,
    read: LegalAuxiliaryRead,
    clock: &impl Clock,
) -> Result<Value, LegalAuxiliaryError> {
    authorize_read(actor, &read)?;
    match read {
        LegalAuxiliaryRead::Workspace => ports.workspace(actor).await,
        LegalAuxiliaryRead::Reports => {
            ports.reports(actor, clock.now().get()).await
        }
        LegalAuxiliaryRead::Retention { id } => {
            let id = id.as_deref().map(parse_identifier).transpose()?;
            ports.retention(actor, id.as_deref()).await
        }
        LegalAuxiliaryRead::Governance { id } => {
            let id = id.as_deref().map(parse_identifier).transpose()?;
            ports.governance(actor, id.as_deref()).await
        }
        LegalAuxiliaryRead::LawCorpus { rule_id } => {
            let rule_id = rule_id.as_deref().map(parse_identifier).transpose()?;
            ports.law_corpus(actor, rule_id.as_deref()).await
        }
        LegalAuxiliaryRead::Knowledge { query, as_of } => {
            let query = crate::domain::auxiliary::parse_search(&query)?;
            let as_of = parse_as_of(as_of.as_deref())?
                .unwrap_or_else(|| bangkok_date(clock.now().get()));
            ports.knowledge(actor, &query, &as_of).await
        }
        LegalAuxiliaryRead::LawCitation { id, as_of } => {
            let id = parse_identifier(id.trim_start_matches("law:"))?;
            let as_of = parse_as_of(as_of.as_deref())?
                .unwrap_or_else(|| bangkok_date(clock.now().get()));
            ports.law_citation(actor, &id, &as_of).await
        }
        LegalAuxiliaryRead::References { item_id } => {
            let item_id = parse_identifier(&item_id)?;
            ports.references(actor, &item_id).await
        }
        LegalAuxiliaryRead::Trust => ports.trust(actor).await,
        LegalAuxiliaryRead::SavedViews => ports.saved_views(actor).await,
        LegalAuxiliaryRead::AiHistory { item_id } => {
            let item_id = parse_identifier(&item_id)?;
            ports.ai_history(actor, &item_id).await
        }
    }
}

/// Parse the retained GET path into one bounded auxiliary read. Returns `None`
/// for paths that another native owner route or the retained route serves.
pub fn parse_auxiliary_path(path: &[String], query: &[(String, String)]) -> Option<LegalAuxiliaryRead> {
    let get = |name: &str| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    Some(match path {
        [one] if one == "workspace" => LegalAuxiliaryRead::Workspace,
        [one] if one == "reports" => LegalAuxiliaryRead::Reports,
        [one] if one == "trust" => LegalAuxiliaryRead::Trust,
        [one] if one == "views" => LegalAuxiliaryRead::SavedViews,
        [one] if one == "laws" => LegalAuxiliaryRead::LawCorpus {
            rule_id: get("ruleId"),
        },
        [one] if one == "knowledge" => LegalAuxiliaryRead::Knowledge {
            query: get("q").unwrap_or_default(),
            as_of: get("asOf"),
        },
        [one] if one == "retention" => LegalAuxiliaryRead::Retention { id: None },
        [one] if one == "governance" => LegalAuxiliaryRead::Governance { id: None },
        [one, id] if one == "retention" => LegalAuxiliaryRead::Retention { id: Some(id.clone()) },
        [one, id] if one == "governance" => LegalAuxiliaryRead::Governance { id: Some(id.clone()) },
        [one, id] if one == "law-citations" => LegalAuxiliaryRead::LawCitation {
            id: id.clone(),
            as_of: get("asOf"),
        },
        [one, id] if one == "references" => LegalAuxiliaryRead::References { item_id: id.clone() },
        [one, id] if one == "ai" => LegalAuxiliaryRead::AiHistory { item_id: id.clone() },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::auxiliary::*;
    use erpx_server_shared::public::{
        AssignmentId, CompanyId, FailureKind, HoldingId, TimestampMillis, UserId,
    };

    struct Fake {
        calls: Vec<&'static str>,
        seen: Vec<String>,
    }

    impl LegalAuxiliaryReadPorts for Fake {
        fn workspace<'a>(&'a mut self, _: &'a LegalAuxiliaryActor) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("workspace");
            Box::pin(async { Ok(serde_json::json!({"deadlines": [], "assigned": []})) })
        }
        fn reports<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, now: i64) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("reports");
            self.seen.push(now.to_string());
            Box::pin(async { Ok(serde_json::json!({"asOf": 0})) })
        }
        fn retention<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, id: Option<&'a str>) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("retention");
            self.seen.push(id.unwrap_or("").to_owned());
            Box::pin(async { Ok(serde_json::json!({"policies": []})) })
        }
        fn governance<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, id: Option<&'a str>) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("governance");
            self.seen.push(id.unwrap_or("").to_owned());
            Box::pin(async { Ok(serde_json::json!({"forms": []})) })
        }
        fn law_corpus<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, rule_id: Option<&'a str>) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("law_corpus");
            self.seen.push(rule_id.unwrap_or("").to_owned());
            Box::pin(async { Ok(serde_json::json!({"sources": []})) })
        }
        fn knowledge<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, query: &'a str, as_of: &'a str) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("knowledge");
            self.seen.push(format!("{query}|{as_of}"));
            Box::pin(async move { Ok(serde_json::json!({"query": query})) })
        }
        fn law_citation<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, chunk_id: &'a str, as_of: &'a str) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("law_citation");
            self.seen.push(format!("{chunk_id}|{as_of}"));
            Box::pin(async move { Ok(serde_json::json!({"id": chunk_id})) })
        }
        fn references<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, item_id: &'a str) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("references");
            self.seen.push(item_id.to_owned());
            Box::pin(async { Ok(serde_json::json!([])) })
        }
        fn trust<'a>(&'a mut self, _: &'a LegalAuxiliaryActor) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("trust");
            Box::pin(async { Ok(serde_json::json!([])) })
        }
        fn saved_views<'a>(&'a mut self, _: &'a LegalAuxiliaryActor) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("saved_views");
            Box::pin(async { Ok(serde_json::json!([])) })
        }
        fn ai_history<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, item_id: &'a str) -> LegalAuxiliaryFuture<'a, Value> {
            self.calls.push("ai_history");
            self.seen.push(item_id.to_owned());
            Box::pin(async { Ok(serde_json::json!([])) })
        }
    }

    struct Fixed(i64);
    impl Clock for Fixed {
        fn now(&self) -> TimestampMillis {
            TimestampMillis::new(self.0)
        }
    }

    fn actor(permissions: &[&str]) -> LegalAuxiliaryActor {
        LegalAuxiliaryActor {
            user_id: UserId::new("user-one").unwrap(),
            assignment_id: AssignmentId::new("assignment-one").unwrap(),
            holding_id: HoldingId::new("holding-one").unwrap(),
            company_id: CompanyId::new("company-one").unwrap(),
            branch_id: None,
            department_id: None,
            role: "legal_manager".into(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    fn ports() -> Fake {
        Fake { calls: vec![], seen: vec![] }
    }

    fn business_kind(error: &LegalAuxiliaryError) -> Option<FailureKind> {
        match error {
            PgTransactionError::Business(error) => Some(error.kind),
            PgTransactionError::Persistence(_) => None,
        }
    }

    #[tokio::test]
    async fn dispatch_defaults_as_of_to_the_bangkok_date() {
        let mut ports = ports();
        let actor = actor(&["legal.read"]);
        read_auxiliary(
            &mut ports,
            &actor,
            LegalAuxiliaryRead::Knowledge {
                query: "  contract  ".into(),
                as_of: None,
            },
            &Fixed(1_794_330_000_000),
        )
        .await
        .unwrap();
        assert_eq!(ports.seen, vec!["contract|2026-11-11".to_owned()]);
    }

    #[tokio::test]
    async fn explicit_as_of_is_validated_and_stripped_from_citation_ids() {
        let mut ports = ports();
        let actor = actor(&["legal.read"]);
        read_auxiliary(
            &mut ports,
            &actor,
            LegalAuxiliaryRead::LawCitation {
                id: "law:chunk-one".into(),
                as_of: Some("2024-02-29".into()),
            },
            &Fixed(0),
        )
        .await
        .unwrap();
        assert_eq!(ports.seen, vec!["chunk-one|2024-02-29".to_owned()]);
        let failure = read_auxiliary(
            &mut ports,
            &actor,
            LegalAuxiliaryRead::LawCitation {
                id: "chunk-one".into(),
                as_of: Some("2024-02-30".into()),
            },
            &Fixed(0),
        )
        .await
        .unwrap_err();
        assert_eq!(business_kind(&failure), Some(FailureKind::InvalidInput));
    }

    #[tokio::test]
    async fn unauthorized_reads_never_reach_the_port() {
        let mut ports = ports();
        let failure = read_auxiliary(
            &mut ports,
            &actor(&[]),
            LegalAuxiliaryRead::Reports,
            &Fixed(0),
        )
        .await
        .unwrap_err();
        assert_eq!(business_kind(&failure), Some(FailureKind::Forbidden));
        assert!(ports.calls.is_empty());
    }

    #[test]
    fn paths_map_to_bounded_reads_only() {
        let query = vec![("ruleId".to_owned(), "rule-one".to_owned())];
        assert_eq!(
            parse_auxiliary_path(&["laws".into()], &query),
            Some(LegalAuxiliaryRead::LawCorpus { rule_id: Some("rule-one".into()) })
        );
        assert_eq!(
            parse_auxiliary_path(&["retention".into(), "item-one".into()], &[]),
            Some(LegalAuxiliaryRead::Retention { id: Some("item-one".into()) })
        );
        assert_eq!(parse_auxiliary_path(&["documents".into(), "one".into()], &[]), None);
        assert_eq!(parse_auxiliary_path(&["laws".into(), "extra".into()], &query), None);
    }
}
