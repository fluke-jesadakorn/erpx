//! Legal record use cases. The owner transaction remains in the adapter.
use crate::domain::legal_records::*;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CommandKey};
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type LegalRecordsError = PgTransactionError<BusinessError>;
pub type LegalRecordsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, LegalRecordsError>> + Send + 'a>>;

/// The caller supplies one serializable Legal owner transaction. Reads apply
/// current Company/branch/department/membership visibility; `source` locks the
/// record; `commit` re-checks the version and persists receipt/audit/outbox.
pub trait LegalRecordsPorts {
    fn register<'a>(
        &'a mut self,
        actor: &'a LegalRecordsActor,
        query: &'a LegalRegisterQuery,
    ) -> LegalRecordsFuture<'a, LegalRegister>;
    fn detail<'a>(
        &'a mut self,
        actor: &'a LegalRecordsActor,
        id: &'a str,
    ) -> LegalRecordsFuture<'a, LegalDetail>;
    fn replay<'a>(
        &'a mut self,
        key: &'a str,
        request_hash: &'a str,
    ) -> LegalRecordsFuture<'a, Option<Value>>;
    fn source<'a>(
        &'a mut self,
        actor: &'a LegalRecordsActor,
        id: &'a str,
    ) -> LegalRecordsFuture<'a, LegalCommandSource>;
    fn commit<'a>(
        &'a mut self,
        actor: &'a LegalRecordsActor,
        key: &'a str,
        request_hash: &'a str,
        plan: &'a LegalCommandPlan,
    ) -> LegalRecordsFuture<'a, Value>;
}

pub async fn legal_register<P: LegalRecordsPorts>(
    ports: &mut P,
    actor: &LegalRecordsActor,
    query: LegalRegisterQuery,
) -> Result<LegalRegister, LegalRecordsError> {
    authorize_register(actor)?;
    ports.register(actor, &query).await
}

pub async fn legal_detail<P: LegalRecordsPorts>(
    ports: &mut P,
    actor: &LegalRecordsActor,
    id: &str,
) -> Result<LegalDetail, LegalRecordsError> {
    authorize_detail(actor)?;
    parse_detail_id(id)?;
    ports.detail(actor, id).await
}

/// Replay is deliberately checked before payload validation and current item
/// visibility. A lost response therefore retains its original result even when
/// the action is no longer applicable.
pub async fn legal_command<P: LegalRecordsPorts>(
    ports: &mut P,
    actor: &LegalRecordsActor,
    key: &CommandKey,
    request_hash: &str,
    raw: &Value,
) -> Result<Value, LegalRecordsError> {
    let envelope = parse_envelope(raw)?;
    require(actor, command_permission(&envelope.action)?)?;
    if let Some(prior) = ports.replay(key.as_str(), request_hash).await? {
        return Ok(prior);
    }
    let command = parse_command(raw, &envelope)?;
    let source = ports.source(actor, &envelope.id).await?;
    let plan = plan_command(actor, &source, &envelope, &command, key.as_str())?;
    ports.commit(actor, key.as_str(), request_hash, &plan).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::{AssignmentId, CompanyId, HoldingId, UserId};
    use serde_json::json;

    #[derive(Default)]
    struct Fake {
        register: Option<LegalRegister>,
        detail: Option<LegalDetail>,
        replay: Option<Value>,
        source: Option<LegalCommandSource>,
        committed: Vec<LegalCommandPlan>,
        reads: Vec<&'static str>,
        fail_source: bool,
    }

    impl LegalRecordsPorts for Fake {
        fn register<'a>(
            &'a mut self,
            _: &'a LegalRecordsActor,
            _: &'a LegalRegisterQuery,
        ) -> LegalRecordsFuture<'a, LegalRegister> {
            self.reads.push("register");
            let value = self.register.take();
            Box::pin(async move {
                value.ok_or_else(|| {
                    BusinessError::new(
                        erpx_server_shared::public::FailureKind::Unavailable,
                        "LEGAL_READ_UNAVAILABLE",
                        "Legal records are unavailable.",
                    )
                    .into()
                })
            })
        }

        fn detail<'a>(
            &'a mut self,
            _: &'a LegalRecordsActor,
            _: &'a str,
        ) -> LegalRecordsFuture<'a, LegalDetail> {
            self.reads.push("detail");
            let value = self.detail.take();
            Box::pin(async move {
                value.ok_or_else(|| {
                    BusinessError::new(
                        erpx_server_shared::public::FailureKind::Unavailable,
                        "LEGAL_READ_UNAVAILABLE",
                        "Legal records are unavailable.",
                    )
                    .into()
                })
            })
        }

        fn replay<'a>(
            &'a mut self,
            _: &'a str,
            _: &'a str,
        ) -> LegalRecordsFuture<'a, Option<Value>> {
            self.reads.push("replay");
            let value = self.replay.take();
            Box::pin(async move { Ok(value) })
        }

        fn source<'a>(
            &'a mut self,
            _: &'a LegalRecordsActor,
            _: &'a str,
        ) -> LegalRecordsFuture<'a, LegalCommandSource> {
            self.reads.push("source");
            if self.fail_source {
                return Box::pin(async {
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::NotFound,
                        "LEGAL_NOT_FOUND",
                        "Legal record is unavailable in your current scope.",
                    )
                    .into())
                });
            }
            let value = self.source.take();
            Box::pin(async move {
                value.ok_or_else(|| {
                    BusinessError::new(
                        erpx_server_shared::public::FailureKind::NotFound,
                        "LEGAL_NOT_FOUND",
                        "Legal record is unavailable in your current scope.",
                    )
                    .into()
                })
            })
        }

        fn commit<'a>(
            &'a mut self,
            _: &'a LegalRecordsActor,
            _: &'a str,
            _: &'a str,
            plan: &'a LegalCommandPlan,
        ) -> LegalRecordsFuture<'a, Value> {
            self.reads.push("commit");
            self.committed.push(plan.clone());
            let result = serde_json::to_value(&plan.result).unwrap();
            Box::pin(async move { Ok(result) })
        }
    }

    fn actor(permissions: &[&str]) -> LegalRecordsActor {
        LegalRecordsActor {
            holding_id: HoldingId::new("holding").unwrap(),
            holding_slug: "acme".into(),
            company_id: CompanyId::new("company").unwrap(),
            user_id: UserId::new("user").unwrap(),
            assignment_id: AssignmentId::new("assignment").unwrap(),
            branch_id: None,
            department_id: None,
            role: "legal_manager".into(),
            permissions: permissions
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        }
    }

    fn source(state: &str, editor: bool) -> LegalCommandSource {
        LegalCommandSource {
            item: LegalRecord {
                id: "item".into(),
                company_id: "company".into(),
                branch_id: None,
                department_id: None,
                kind: "contract".into(),
                title: "Synthetic contract".into(),
                description: String::new(),
                classification: "internal".into(),
                country: "TH".into(),
                jurisdiction: "Thailand".into(),
                language: "th".into(),
                currency: "THB".into(),
                timezone: "Asia/Bangkok".into(),
                priority: "normal".into(),
                due_date: None,
                owner_id: "user".into(),
                created_by: "user".into(),
                state: state.into(),
                version: 1,
                created_at: 0,
                updated_at: 0,
                data: json!({}),
            },
            enabled: true,
            editor,
            frozen: false,
            holds: Vec::new(),
            documents: Vec::new(),
            next_document_version: 2,
        }
    }

    fn key() -> CommandKey {
        CommandKey::new("legal-command-key-0001").unwrap()
    }

    fn register() -> LegalRegister {
        LegalRegister {
            records: Vec::new(),
            total: 0,
            page: 1,
            counts: Vec::new(),
            enabled: true,
            can_edit: true,
            can_hold: true,
            can_release_hold: true,
            can_share: true,
        }
    }

    #[test]
    fn register_and_detail_require_legal_read_before_any_owner_query() {
        let mut fake = Fake {
            register: Some(register()),
            detail: None,
            ..Default::default()
        };
        let denied = actor(&[]);
        assert_eq!(
            code(
                block_on(legal_register(
                    &mut fake,
                    &denied,
                    LegalRegisterQuery::parse(None, "", "", 1, false).unwrap()
                ))
                .unwrap_err()
            ),
            Some("PERMISSION_REQUIRED")
        );
        assert!(fake.reads.is_empty());
        assert!(
            block_on(legal_register(
                &mut fake,
                &actor(&["legal.read"]),
                LegalRegisterQuery::parse(None, "", "", 1, false).unwrap()
            ))
            .is_ok()
        );
        assert_eq!(fake.reads, vec!["register"]);
    }

    #[test]
    fn a_lost_response_replays_before_current_source_and_payload_checks() {
        let mut fake = Fake {
            replay: Some(json!({"id":"item","version":2,"state":"draft"})),
            fail_source: true,
            ..Default::default()
        };
        let raw = json!({"action":"document","id":"item","expectedVersion":1,"name":"Agreement","body":"text"});
        let result = block_on(legal_command(
            &mut fake,
            &actor(&["legal.read", "legal.edit"]),
            &key(),
            "hash",
            &raw,
        ))
        .unwrap();
        assert_eq!(result["version"], 2);
        assert_eq!(fake.reads, vec!["replay"]);
        assert!(fake.committed.is_empty());
    }

    #[test]
    fn permission_is_checked_before_the_record_source() {
        let mut fake = Fake {
            source: Some(source("draft", true)),
            ..Default::default()
        };
        let raw = json!({"action":"document","id":"item","expectedVersion":1,"name":"Agreement","body":"text"});
        assert_eq!(
            code(
                block_on(legal_command(
                    &mut fake,
                    &actor(&["legal.read"]),
                    &key(),
                    "hash",
                    &raw
                ))
                .unwrap_err()
            ),
            Some("PERMISSION_REQUIRED")
        );
        assert!(fake.reads.is_empty());
    }

    #[test]
    fn a_planned_document_commits_the_expected_owner_effect() {
        let mut fake = Fake {
            source: Some(source("draft", true)),
            ..Default::default()
        };
        let raw = json!({"action":"document","id":"item","expectedVersion":1,"name":"Agreement","body":"text"});
        let result = block_on(legal_command(
            &mut fake,
            &actor(&["legal.read", "legal.edit"]),
            &key(),
            "hash",
            &raw,
        ))
        .unwrap();
        assert_eq!(result["id"], "item");
        assert_eq!(result["version"], 2);
        assert_eq!(fake.reads, vec!["replay", "source", "commit"]);
        assert_eq!(fake.committed.len(), 1);
        assert!(matches!(
            fake.committed[0].effect,
            LegalCommandEffect::AddDocument { .. }
        ));
    }

    #[test]
    fn a_changed_version_stops_before_commit() {
        let mut fake = Fake {
            source: Some(source("draft", true)),
            ..Default::default()
        };
        let mut current = fake.source.take().unwrap();
        current.item.version = 2;
        fake.source = Some(current);
        let raw = json!({"action":"document","id":"item","expectedVersion":1,"name":"Agreement","body":"text"});
        assert_eq!(
            code(
                block_on(legal_command(
                    &mut fake,
                    &actor(&["legal.read", "legal.edit"]),
                    &key(),
                    "hash",
                    &raw
                ))
                .unwrap_err()
            ),
            Some("LEGAL_VERSION_CONFLICT")
        );
        assert!(fake.committed.is_empty());
    }

    fn code(error: LegalRecordsError) -> Option<&'static str> {
        match error {
            PgTransactionError::Business(error) => Some(error.code),
            PgTransactionError::Persistence(_) => None,
        }
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }
}
