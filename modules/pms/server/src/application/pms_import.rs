//! Native PMS cutover-import write use cases.
//!
//! The owner adapter receives one caller-owned serializable Holding transaction
//! and fresh Account/IAM authority. This layer owns command ordering: typed
//! input validation, exact idempotency replay, optimistic batch transitions and
//! the command receipt/audit/outbox ledger written in the caller's transaction.
//! It never opens another transaction and writes no other owner's rows.
use crate::domain::pms_import::{
    AppliedInput, ApproveInput, BalanceInput, BatchState, DocumentInput, ImportPocket,
    PmsImportCommandKind, PmsImportInput, PreviewInput, PropertyInput, invalid, native_command_id,
    plan_batch_transition,
};
use erpx_platform_shared_server::public::{BusinessError, CompanyId};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type PmsImportCommandFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredImportReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

/// One command receipt, audit event and outbox event written in the same
/// caller-owned transaction as the import row change.
#[derive(Clone, Debug)]
pub struct ImportLedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub state: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct PmsImportActor {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub authority_json: String,
}

impl PmsImportActor {
    pub fn new(user_id: &str, assignment_id: &str, company_id: &str, authority_json: &str) -> Self {
        Self {
            user_id: user_id.to_owned(),
            assignment_id: assignment_id.to_owned(),
            company_id: company_id.to_owned(),
            authority_json: authority_json.to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PreviewWrite<'a> {
    pub batch_id: &'a str,
    pub company_id: &'a str,
    pub property_id: &'a str,
    pub cutover_date: &'a str,
    pub expected_version: i64,
    pub definition_json: &'a str,
    pub report_json: &'a str,
    pub user_id: &'a str,
    pub now: i64,
    pub now2: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct ApproveWrite<'a> {
    pub user_id: &'a str,
    pub opening_journal_id: Option<&'a str>,
    pub now: i64,
    pub id: &'a str,
    pub version: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct BalanceWrite<'a> {
    pub id: &'a str,
    pub source_key: &'a str,
    pub property_id: &'a str,
    pub guest_account_id: &'a str,
    pub entry_id: &'a str,
    pub pocket: ImportPocket,
    pub total: i64,
    pub payment_id: Option<&'a str>,
    pub invoice_id: Option<&'a str>,
}

#[derive(Clone, Copy, Debug)]
pub struct DocumentWrite<'a> {
    pub id: &'a str,
    pub property_id: &'a str,
    pub guest_account_id: &'a str,
    pub finance_id: Option<&'a str>,
    pub snapshot_json: &'a str,
    pub now: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct PropertyWrite<'a> {
    pub now: i64,
    pub property_id: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct AppliedWrite<'a> {
    pub now: i64,
    pub id: &'a str,
    pub version: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct BatchFacts {
    pub state: BatchState,
    pub version: i64,
}

pub trait PmsImportCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    /// Real adapters return the caller-owned transaction so composed ports
    /// resolve in the same unit. Owner-local fakes may return `None`.
    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> PmsImportCommandFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> PmsImportCommandFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PmsImportCommandFuture<'a, Option<StoredImportReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a ImportLedgerEntry<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error>;
    fn batch<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> PmsImportCommandFuture<'a, Option<BatchFacts>, Self::Error>;
    fn preview<'a>(
        &'a mut self,
        command: &'a PreviewWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error>;
    fn approve<'a>(
        &'a mut self,
        command: &'a ApproveWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error>;
    fn balance<'a>(
        &'a mut self,
        command: &'a BalanceWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error>;
    fn document<'a>(
        &'a mut self,
        command: &'a DocumentWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error>;
    fn property<'a>(
        &'a mut self,
        command: &'a PropertyWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error>;
    fn applied<'a>(
        &'a mut self,
        command: &'a AppliedWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize> {
    user_id: &'a str,
    assignment_id: &'a str,
    company_id: &'a str,
    operation: &'a str,
    record_id: &'a str,
    input: &'a I,
}

async fn prepared<R: PmsImportCommandRepository, I: Serialize>(
    actor: &PmsImportActor,
    repository: &mut R,
    key: &str,
    operation: &str,
    record_id: &str,
    input: &I,
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        user_id: &actor.user_id,
        assignment_id: &actor.assignment_id,
        company_id: &actor.company_id,
        operation,
        record_id,
        input,
    })
    .map_err(|_| R::Error::from(invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
    if let Some(receipt) = repository.receipt(key).await? {
        if receipt.request_hash != request_hash {
            return Err(crate::domain::pms_import::conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different write. Start a new action.",
            )
            .into());
        }
        return match receipt.result {
            Some(result) => Ok((request_hash, Some(result))),
            None => Err(crate::domain::pms_import::conflict(
                "COMMAND_INCOMPLETE",
                "This command did not finish. Retry the same action.",
            )
            .into()),
        };
    }
    Ok((request_hash, None))
}

fn replay<T: serde::de::DeserializeOwned>(result: Option<Value>) -> Result<T, BusinessError> {
    match result {
        Some(value) => serde_json::from_value(value).map_err(|_| invalid()),
        None => Err(invalid()),
    }
}

async fn write_ledger<R: PmsImportCommandRepository>(
    repository: &mut R,
    key: &str,
    kind: PmsImportCommandKind,
    record_type: &'static str,
    record_id: &str,
    version: i64,
    request_hash: &str,
    result: &Value,
    state: &'static str,
    now: i64,
) -> Result<(), R::Error> {
    repository
        .ledger(&ImportLedgerEntry {
            key,
            operation: kind.operation(),
            record_type,
            record_id,
            version,
            request_hash,
            result,
            state,
            now,
        })
        .await
}

fn scope_company(actor: &PmsImportActor, company: &CompanyId) -> Result<(), BusinessError> {
    if actor.company_id != company.as_str() {
        return Err(crate::domain::pms_import::conflict(
            "PMS_IMPORT_SCOPE",
            "The import write is outside this company workspace.",
        ));
    }
    Ok(())
}

pub async fn preview<R: PmsImportCommandRepository>(
    actor: &PmsImportActor,
    raw: &Value,
    key: &str,
    repository: &mut R,
) -> Result<Value, R::Error> {
    let input = crate::domain::pms_import::parse_input(PmsImportCommandKind::Preview, raw)?;
    let PmsImportInput::Preview(PreviewInput {
        batch_id,
        company_id,
        property_id,
        cutover_date,
        expected_version,
        definition_json,
        report_json,
        user_id,
        now,
        now2,
    }) = input
    else {
        return Err(invalid().into());
    };
    let company = CompanyId::new(company_id)?;
    scope_company(actor, &company)?;
    let native_key = native_command_id(key, PmsImportCommandKind::Preview);
    let (request_hash, prior) = prepared(
        actor,
        repository,
        &native_key,
        PmsImportCommandKind::Preview.operation(),
        batch_id,
        raw,
    )
    .await?;
    if let Some(result) = prior {
        return replay(Some(result)).map_err(R::Error::from);
    }
    let result = json!({"id": batch_id, "state": "preview", "version": 1});
    write_ledger(
        repository,
        &native_key,
        PmsImportCommandKind::Preview,
        "pms_import_batches",
        batch_id,
        1,
        &request_hash,
        &result,
        "preview",
        now,
    )
    .await?;
    repository
        .preview(&PreviewWrite {
            batch_id,
            company_id: company.as_str(),
            property_id,
            cutover_date,
            expected_version,
            definition_json,
            report_json,
            user_id,
            now,
            now2,
        })
        .await?;
    Ok(result)
}

pub async fn approve<R: PmsImportCommandRepository>(
    actor: &PmsImportActor,
    raw: &Value,
    key: &str,
    repository: &mut R,
) -> Result<Value, R::Error> {
    let input = crate::domain::pms_import::parse_input(PmsImportCommandKind::Approve, raw)?;
    let PmsImportInput::Approve(ApproveInput {
        user_id,
        opening_journal_id,
        now,
        id,
    }) = input
    else {
        return Err(invalid().into());
    };
    let company = CompanyId::new(&actor.company_id)?;
    let native_key = native_command_id(key, PmsImportCommandKind::Approve);
    let (request_hash, prior) = prepared(
        actor,
        repository,
        &native_key,
        PmsImportCommandKind::Approve.operation(),
        id,
        raw,
    )
    .await?;
    if let Some(result) = prior {
        return replay(Some(result)).map_err(R::Error::from);
    }
    let batch = repository.batch(&company, id).await?.ok_or_else(|| {
        crate::domain::pms_import::conflict("PMS_IMPORT_NOT_FOUND", "Import batch not found.")
    })?;
    let version = plan_batch_transition(batch.state, batch.version, BatchState::Approved)
        .map_err(R::Error::from)?;
    let result = json!({"id": id, "state": "approved", "version": version});
    write_ledger(
        repository,
        &native_key,
        PmsImportCommandKind::Approve,
        "pms_import_batches",
        id,
        version,
        &request_hash,
        &result,
        "approved",
        now,
    )
    .await?;
    repository
        .approve(&ApproveWrite {
            user_id,
            opening_journal_id,
            now,
            id,
            version,
        })
        .await?;
    Ok(result)
}

pub async fn balance<R: PmsImportCommandRepository>(
    actor: &PmsImportActor,
    raw: &Value,
    key: &str,
    repository: &mut R,
) -> Result<Value, R::Error> {
    let input = crate::domain::pms_import::parse_input(PmsImportCommandKind::Balance, raw)?;
    let PmsImportInput::Balance(BalanceInput {
        id,
        source_key,
        property_id,
        guest_account_id,
        entry_id,
        pocket,
        total,
        payment_id,
        invoice_id,
    }) = input
    else {
        return Err(invalid().into());
    };
    let native_key = native_command_id(key, PmsImportCommandKind::Balance);
    let (request_hash, prior) = prepared(
        actor,
        repository,
        &native_key,
        PmsImportCommandKind::Balance.operation(),
        id,
        raw,
    )
    .await?;
    if let Some(result) = prior {
        return replay(Some(result)).map_err(R::Error::from);
    }
    let result = json!({"id": id, "state": "applied", "version": 1});
    write_ledger(
        repository,
        &native_key,
        PmsImportCommandKind::Balance,
        "pms_import_balances",
        id,
        1,
        &request_hash,
        &result,
        "applied",
        0,
    )
    .await?;
    repository
        .balance(&BalanceWrite {
            id,
            source_key,
            property_id,
            guest_account_id,
            entry_id,
            pocket,
            total,
            payment_id,
            invoice_id,
        })
        .await?;
    Ok(result)
}

pub async fn document<R: PmsImportCommandRepository>(
    actor: &PmsImportActor,
    raw: &Value,
    key: &str,
    repository: &mut R,
) -> Result<Value, R::Error> {
    let input = crate::domain::pms_import::parse_input(PmsImportCommandKind::Document, raw)?;
    let PmsImportInput::Document(DocumentInput {
        id,
        property_id,
        guest_account_id,
        finance_id,
        snapshot_json,
        now,
    }) = input
    else {
        return Err(invalid().into());
    };
    let native_key = native_command_id(key, PmsImportCommandKind::Document);
    let (request_hash, prior) = prepared(
        actor,
        repository,
        &native_key,
        PmsImportCommandKind::Document.operation(),
        id,
        raw,
    )
    .await?;
    if let Some(result) = prior {
        return replay(Some(result)).map_err(R::Error::from);
    }
    let result = json!({"id": id, "state": "applied", "version": 1});
    write_ledger(
        repository,
        &native_key,
        PmsImportCommandKind::Document,
        "pms_documents",
        id,
        1,
        &request_hash,
        &result,
        "applied",
        now,
    )
    .await?;
    repository
        .document(&DocumentWrite {
            id,
            property_id,
            guest_account_id,
            finance_id,
            snapshot_json,
            now,
        })
        .await?;
    Ok(result)
}

pub async fn property<R: PmsImportCommandRepository>(
    actor: &PmsImportActor,
    raw: &Value,
    key: &str,
    repository: &mut R,
) -> Result<Value, R::Error> {
    let input = crate::domain::pms_import::parse_input(PmsImportCommandKind::Property, raw)?;
    let PmsImportInput::Property(PropertyInput { now, property_id }) = input else {
        return Err(invalid().into());
    };
    let native_key = native_command_id(key, PmsImportCommandKind::Property);
    let (request_hash, prior) = prepared(
        actor,
        repository,
        &native_key,
        PmsImportCommandKind::Property.operation(),
        property_id,
        raw,
    )
    .await?;
    if let Some(result) = prior {
        return replay(Some(result)).map_err(R::Error::from);
    }
    let result = json!({"id": property_id, "state": "bumped", "version": 1});
    write_ledger(
        repository,
        &native_key,
        PmsImportCommandKind::Property,
        "pms_properties",
        property_id,
        1,
        &request_hash,
        &result,
        "bumped",
        now,
    )
    .await?;
    repository
        .property(&PropertyWrite { now, property_id })
        .await?;
    Ok(result)
}

pub async fn applied<R: PmsImportCommandRepository>(
    actor: &PmsImportActor,
    raw: &Value,
    key: &str,
    repository: &mut R,
) -> Result<Value, R::Error> {
    let input = crate::domain::pms_import::parse_input(PmsImportCommandKind::Applied, raw)?;
    let PmsImportInput::Applied(AppliedInput { now, id }) = input else {
        return Err(invalid().into());
    };
    let company = CompanyId::new(&actor.company_id)?;
    let native_key = native_command_id(key, PmsImportCommandKind::Applied);
    let (request_hash, prior) = prepared(
        actor,
        repository,
        &native_key,
        PmsImportCommandKind::Applied.operation(),
        id,
        raw,
    )
    .await?;
    if let Some(result) = prior {
        return replay(Some(result)).map_err(R::Error::from);
    }
    let batch = repository.batch(&company, id).await?.ok_or_else(|| {
        crate::domain::pms_import::conflict("PMS_IMPORT_NOT_FOUND", "Import batch not found.")
    })?;
    let version = plan_batch_transition(batch.state, batch.version, BatchState::Applied)
        .map_err(R::Error::from)?;
    let result = json!({"id": id, "state": "applied", "version": version});
    write_ledger(
        repository,
        &native_key,
        PmsImportCommandKind::Applied,
        "pms_import_batches",
        id,
        version,
        &request_hash,
        &result,
        "applied",
        now,
    )
    .await?;
    repository
        .applied(&AppliedWrite { now, id, version })
        .await?;
    Ok(result)
}
