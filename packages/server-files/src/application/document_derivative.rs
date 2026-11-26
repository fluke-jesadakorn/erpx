//! Application for the bounded native document-page derivative command.
//!
//! The handler opens no transaction and reads no HTTP request. Composition
//! supplies the current actor, the owning expense authority and the byte store;
//! the Files adapter owns the attachment head, the replay receipt and the exact
//! derivative/storage effects. One command key fences one transaction boundary,
//! and every validation gate runs before the byte store is touched.
use crate::domain::document_derivative::{
    DERIVATIVE_OPERATION, DerivativeAuthority, DocumentDerivativeResult, DocumentPageInput,
    DocumentPageSource, validate_page, validate_type,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{CommandActor, CommandJson, command_fingerprint};
use sha2::{Digest, Sha256};
use std::future::Future;
use std::pin::Pin;

pub type Error = PgTransactionError<BusinessError>;
pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "DOCUMENT_NOT_FOUND",
        "The original document is unavailable or changed.",
    )
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "DOCUMENT_PREPARATION_FORBIDDEN",
        "Only the person preparing or reviewing the current stage can prepare document pages.",
    )
}

/// One bounded save-page command. All request-derived facts are normalized by
/// the HTTP adapter; this struct is the only input the application accepts.
#[derive(Clone, Debug)]
pub struct SaveDocumentPage<'a> {
    pub actor: &'a CommandActor,
    pub holding_id: &'a str,
    pub attachment_id: &'a str,
    pub page: u32,
    pub page_count: u32,
    pub original_checksum: &'a str,
    pub bytes: &'a [u8],
    pub key: &'a str,
    pub derivative_id: &'a str,
    pub now: i64,
}

/// Composition-owned owner-unit ports. All SQL stays in the Files adapter; the
/// cross-owner expense authority remains with the owning business owner.
pub trait DocumentDerivativePorts<C>: Send + Sync {
    fn read_only(&self) -> bool;
    fn uploads_frozen(&self) -> bool;

    /// Files-owned attachment head, limited to live available expense originals.
    fn attachment<'a>(
        &'a self,
        c: &'a mut C,
        company_id: &'a str,
        attachment_id: &'a str,
    ) -> PgWorkFuture<'a, Option<DocumentPageSource>, BusinessError>;

    /// Cross-owner expense/payment authority resolved at the composition root.
    fn authority<'a>(
        &'a self,
        c: &'a mut C,
        company_id: &'a str,
        owner_id: &'a str,
        payment_id: Option<&'a str>,
        actor_id: &'a str,
    ) -> PgWorkFuture<'a, DerivativeAuthority, BusinessError>;

    /// Exact prior command receipt; a conflicting key fails closed in the adapter.
    fn replay<'a>(
        &'a self,
        c: &'a mut C,
        key: &'a str,
        fingerprint: &'a str,
    ) -> PgWorkFuture<'a, Option<DocumentDerivativeResult>, BusinessError>;

    /// Already-materialized derivative for this page/original pair.
    fn existing<'a>(
        &'a self,
        c: &'a mut C,
        attachment_id: &'a str,
        page: u32,
        original_checksum: &'a str,
    ) -> PgWorkFuture<'a, Option<DocumentDerivativeResult>, BusinessError>;

    /// Receipt, document record, derivative row and storage ledger in one unit.
    fn commit<'a, 'b>(
        &'a self,
        c: &'a mut C,
        actor: &'a CommandActor,
        record_id: &'a str,
        version: i64,
        fingerprint: &'a str,
        result: &'a DocumentDerivativeResult,
        object_key: &'a str,
        command: &'a SaveDocumentPage<'b>,
        now: i64,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// Private immutable byte store. The checksum is computed by the owner, and a
/// failed commit removes the staged object exactly as the retained route did.
pub trait DocumentDerivativeByteStore: Send + Sync {
    fn put<'a>(&'a self, key: &'a str, bytes: &'a [u8], checksum: &'a str) -> StoreFuture<'a, ()>;
    fn remove<'a>(&'a self, key: &'a str) -> StoreFuture<'a, ()>;
}

/// Prepare and commit one rendered page. Authority and every gate precede the
/// byte store; a replayed key or an existing derivative never writes bytes.
pub async fn save_document_page<C, P, S>(
    c: &mut C,
    ports: &P,
    store: &S,
    command: &SaveDocumentPage<'_>,
) -> Result<DocumentDerivativeResult, Error>
where
    C: Send,
    P: DocumentDerivativePorts<C>,
    S: DocumentDerivativeByteStore,
{
    if ports.read_only() {
        return Err(unavailable(
            "ATTACHMENTS_READ_ONLY",
            "Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.",
        )
        .into());
    }
    if ports.uploads_frozen() {
        return Err(unavailable(
            "ATTACHMENT_UPLOAD_FREEZE",
            "File intake is briefly paused for storage reconciliation.",
        )
        .into());
    }
    let input = DocumentPageInput {
        page: command.page,
        page_count: command.page_count,
        original_checksum: command.original_checksum.to_owned(),
        bytes: command.bytes.to_vec(),
    };
    validate_page(&input)?;
    let source = ports
        .attachment(
            c,
            command.actor.company_id.as_str(),
            command.attachment_id,
        )
        .await?
        .ok_or_else(not_found)?;
    if source.checksum_sha256 != command.original_checksum {
        return Err(not_found().into());
    }
    validate_type(&source, &input)?;
    let payment_id = if source.owner_type == "expense_payment" {
        Some(source.owner_id.as_str())
    } else {
        None
    };
    let authority = ports
        .authority(
            c,
            command.actor.company_id.as_str(),
            &source.owner_id,
            payment_id,
            command.actor.user_id.as_str(),
        )
        .await?;
    if !authority.allowed {
        return Err(forbidden().into());
    }
    let checksum = hex::encode(Sha256::digest(command.bytes));
    let input_json = CommandJson::from_serializable(&serde_json::json!({
        "attachmentId": command.attachment_id,
        "page": command.page,
        "pageCount": command.page_count,
        "originalChecksum": command.original_checksum,
        "checksum": checksum,
    }))
    .map_err(|_| BusinessError::invalid("ATTACHMENT_REQUEST_INVALID", "Review the page request."))?;
    let fingerprint = command_fingerprint(
        command.actor,
        DERIVATIVE_OPERATION,
        &authority.expense_id,
        &input_json,
    )?;
    if let Some(prior) = ports.replay(c, command.key, &fingerprint).await? {
        return Ok(prior);
    }
    if let Some(existing) = ports
        .existing(
            c,
            command.attachment_id,
            command.page,
            command.original_checksum,
        )
        .await?
    {
        return Ok(existing);
    }
    let object_key = format!("{}/document-pages/{}", command.holding_id, command.derivative_id);
    store.put(&object_key, command.bytes, &checksum).await?;
    let result = DocumentDerivativeResult {
        id: command.derivative_id.to_owned(),
        page: command.page,
    };
    match ports
        .commit(
            c,
            command.actor,
            &authority.expense_id,
            authority.version,
            &fingerprint,
            &result,
            &object_key,
            command,
            command.now,
        )
        .await
    {
        Ok(()) => Ok(result),
        Err(error) => {
            let _ = store.remove(&object_key).await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::{AssignmentId, CompanyId, UserId};
    use erpx_server_workflow::public::commands::{
        CommandField, WorkAuthority,
    };
    use std::sync::{Arc, Mutex};

    fn actor() -> CommandActor {
        CommandActor {
            user_id: UserId::new("user-1").unwrap(),
            assignment_id: AssignmentId::new("assignment-1").unwrap(),
            role: "employee".into(),
            company_id: CompanyId::new("co-1").unwrap(),
            branch_id: CommandField::Omitted,
            department_id: CommandField::Omitted,
            authority: CommandField::Value(WorkAuthority {
                session_id: "session-1".into(),
                revision: 1,
                assignment_version: 1,
            }),
            iam: CommandField::Value(CommandJson::parse("{}").unwrap()),
            assignment_scope_kind: "holding".into(),
            assignment_scope_node_id: None,
        }
    }

    fn command<'a>(actor: &'a CommandActor, checksum: &'a str) -> SaveDocumentPage<'a> {
        SaveDocumentPage {
            actor,
            holding_id: "acme",
            attachment_id: "attachment-1",
            page: 1,
            page_count: 1,
            original_checksum: checksum,
            bytes: &[0xff, 0xd8, 0xff, 0xe0],
            key: "key-00000001",
            derivative_id: "derivative-1",
            now: 7,
        }
    }

    fn future<T: Send + 'static>(
        value: Result<T, BusinessError>,
    ) -> Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'static>> {
        Box::pin(async move { value.map_err(PgTransactionError::Business) })
    }

    struct FakePorts {
        source: DocumentPageSource,
        authority: DerivativeAuthority,
        prior: Option<DocumentDerivativeResult>,
        existing: Option<DocumentDerivativeResult>,
        commit_error: bool,
        reads: Arc<Mutex<Vec<String>>>,
    }

    fn logs() -> Arc<Mutex<Vec<String>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    impl FakePorts {
        fn ok() -> Self {
            Self {
                source: DocumentPageSource {
                    owner_type: "expense_claim".into(),
                    owner_id: "claim-1".into(),
                    checksum_sha256: "a".repeat(64),
                    media_type: "application/pdf".into(),
                },
                authority: DerivativeAuthority {
                    expense_id: "claim-1".into(),
                    version: 1,
                    allowed: true,
                },
                prior: None,
                existing: None,
                commit_error: false,
                reads: logs(),
            }
        }
    }

    impl DocumentDerivativePorts<()> for FakePorts {
        fn read_only(&self) -> bool {
            false
        }
        fn uploads_frozen(&self) -> bool {
            false
        }
        fn attachment<'a>(
            &'a self,
            _: &'a mut (),
            _: &'a str,
            _: &'a str,
        ) -> PgWorkFuture<'a, Option<DocumentPageSource>, BusinessError> {
            future(Ok(Some(self.source.clone())))
        }
        fn authority<'a>(
            &'a self,
            _: &'a mut (),
            _: &'a str,
            _: &'a str,
            _: Option<&'a str>,
            _: &'a str,
        ) -> PgWorkFuture<'a, DerivativeAuthority, BusinessError> {
            future(Ok(self.authority.clone()))
        }
        fn replay<'a>(
            &'a self,
            _: &'a mut (),
            _: &'a str,
            _: &'a str,
        ) -> PgWorkFuture<'a, Option<DocumentDerivativeResult>, BusinessError> {
            self.reads.lock().unwrap().push("replay".into());
            future(Ok(self.prior.clone()))
        }
        fn existing<'a>(
            &'a self,
            _: &'a mut (),
            _: &'a str,
            _: u32,
            _: &'a str,
        ) -> PgWorkFuture<'a, Option<DocumentDerivativeResult>, BusinessError> {
            future(Ok(self.existing.clone()))
        }
        fn commit<'a, 'b>(
            &'a self,
            _: &'a mut (),
            _: &'a CommandActor,
            _: &'a str,
            _: i64,
            _: &'a str,
            _: &'a DocumentDerivativeResult,
            _: &'a str,
            _: &'a SaveDocumentPage<'b>,
            _: i64,
        ) -> PgWorkFuture<'a, (), BusinessError> {
            self.reads.lock().unwrap().push("commit".into());
            if self.commit_error {
                future(Err(unavailable("COMMIT_FAILED", "Commit failed.")))
            } else {
                future(Ok(()))
            }
        }
    }

    struct FakeStore {
        writes: Arc<Mutex<Vec<String>>>,
    }
    impl DocumentDerivativeByteStore for FakeStore {
        fn put<'a>(&'a self, key: &'a str, _: &'a [u8], _: &'a str) -> StoreFuture<'a, ()> {
            self.writes.lock().unwrap().push(format!("put:{key}"));
            Box::pin(async { Ok(()) })
        }
        fn remove<'a>(&'a self, key: &'a str) -> StoreFuture<'a, ()> {
            self.writes.lock().unwrap().push(format!("remove:{key}"));
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn commits_the_exact_derivative_after_authority() {
        let ports = FakePorts::ok();
        let writes = Arc::new(Mutex::new(Vec::new()));
        let store = FakeStore { writes: writes.clone() };
        let actor = actor();
        let checksum = "a".repeat(64);
        let result = save_document_page(&mut (), &ports, &store, &command(&actor, &checksum))
            .await
            .unwrap();
        assert_eq!(result.id, "derivative-1");
        assert_eq!(result.page, 1);
        assert_eq!(*ports.reads.lock().unwrap(), vec!["replay", "commit"]);
        assert_eq!(
            *writes.lock().unwrap(),
            vec!["put:acme/document-pages/derivative-1"]
        );
    }

    #[tokio::test]
    async fn replay_never_writes_bytes() {
        let mut ports = FakePorts::ok();
        ports.prior = Some(DocumentDerivativeResult {
            id: "prior".into(),
            page: 1,
        });
        let writes = Arc::new(Mutex::new(Vec::new()));
        let store = FakeStore { writes: writes.clone() };
        let actor = actor();
        let checksum = "a".repeat(64);
        let result = save_document_page(&mut (), &ports, &store, &command(&actor, &checksum))
            .await
            .unwrap();
        assert_eq!(result.id, "prior");
        assert!(writes.lock().unwrap().is_empty());
        assert_eq!(*ports.reads.lock().unwrap(), vec!["replay"]);
    }

    #[tokio::test]
    async fn denied_authority_stops_before_the_byte_store() {
        let mut ports = FakePorts::ok();
        ports.authority.allowed = false;
        let writes = Arc::new(Mutex::new(Vec::new()));
        let store = FakeStore { writes: writes.clone() };
        let actor = actor();
        let checksum = "a".repeat(64);
        assert!(
            save_document_page(&mut (), &ports, &store, &command(&actor, &checksum))
                .await
                .is_err()
        );
        assert!(writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_commit_removes_the_staged_object() {
        let mut ports = FakePorts::ok();
        ports.commit_error = true;
        let writes = Arc::new(Mutex::new(Vec::new()));
        let store = FakeStore { writes: writes.clone() };
        let actor = actor();
        let checksum = "a".repeat(64);
        assert!(
            save_document_page(&mut (), &ports, &store, &command(&actor, &checksum))
                .await
                .is_err()
        );
        assert_eq!(
            *writes.lock().unwrap(),
            vec![
                "put:acme/document-pages/derivative-1",
                "remove:acme/document-pages/derivative-1"
            ]
        );
    }
}
