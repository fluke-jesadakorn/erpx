//! Legal-owned record/readiness facts in the caller's current Holding unit.
use crate::{application::record_policy::authorize_legal_record, domain::record_policy::*};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;

/// Composition supplies actual PMS/Platform release and Commercial readers.
/// There is no production default, ambient mode or successful fallback.
pub trait PgLegalRecordExternalPolicy {
    fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn release(
        &mut self,
        connection: &mut PgConnection,
        company: &CompanyId,
        now: i64,
    ) -> impl Future<Output = Result<LegalReleaseObservation, Error>> + Send;
    fn purchases(
        &mut self,
        connection: &mut PgConnection,
        company: &CompanyId,
        requirements: &[LegalPurchaseRequirement],
        mode: LegalCommercialMode,
        now: i64,
    ) -> impl Future<Output = Result<LegalPurchaseObservation, Error>> + Send;
}

/// Utility statements only. Acquire these, external owner sources and Files
/// sources before ANY data statement of the coordinator's fresh transaction.
/// Calling this after an older snapshot never establishes phantom freshness.
pub async fn lock_legal_record_sources(connection: &mut PgConnection) -> Result<(), Error> {
    require_transaction(connection).await?;
    sqlx::query("LOCK TABLE legal_items,legal_members,legal_documents,legal_shares,legal_settings,legal_holds,legal_hold_targets,legal_disposals,legal_destructions,legal_requests,legal_matters,legal_contracts,legal_obligations,legal_compliance,legal_licenses,legal_corporate,legal_ip_assets,legal_privacy,legal_counsel,legal_templates,legal_rules,legal_executed_copies,legal_approvals,legal_approval_routes,legal_authorities,legal_retention_assignments,legal_retention_policies,legal_case_events,legal_compliance_actions,legal_privacy_cases,legal_corporate_actions,legal_ip_actions,legal_counsel_entries,legal_obligation_schedule,legal_custody_events,legal_policy_acknowledgements,legal_intake_questions,legal_trust_material,legal_external_uploads IN SHARE ROW EXCLUSIVE MODE").execute(connection).await?;
    Ok(())
}
#[derive(Clone)]
pub struct LegalRecordWitness {
    actor: LegalRecordActor,
    request: LegalRecordRequest,
    transaction: (String, i32, String),
    source: String,
    release: LegalReleaseObservation,
    purchase: LegalPurchaseObservation,
}
impl LegalRecordWitness {
    pub fn revision(&self) -> String {
        format!(
            "{}:{}:{}",
            self.source, self.release.revision, self.purchase.revision
        )
    }
}
pub struct LegalRecordAccess {
    pub item: LegalRecordSummary,
    pub document: Option<LegalRecordDocument>,
    pub held: bool,
    pub witness: LegalRecordWitness,
}
pub struct PgLegalRecordPolicy<'c, External> {
    holding_id: HoldingId,
    company_id: CompanyId,
    connection: &'c mut PgConnection,
    external: &'c mut External,
}
impl<'c, External: PgLegalRecordExternalPolicy + Send> PgLegalRecordPolicy<'c, External> {
    pub fn new(
        holding_id: HoldingId,
        company_id: CompanyId,
        connection: &'c mut PgConnection,
        external: &'c mut External,
    ) -> Self {
        Self {
            holding_id,
            company_id,
            connection,
            external,
        }
    }
    /// The owning coordinator calls this before its first data snapshot. Every
    /// later inspect/revalidate stays in that same fresh retained transaction.
    pub async fn prepare_sources(&mut self) -> Result<(), Error> {
        lock_legal_record_sources(self.connection).await?;
        self.external.lock_sources(self.connection).await
    }
    pub async fn inspect(
        &mut self,
        actor: &LegalRecordActor,
        request: &LegalRecordRequest,
        now: i64,
    ) -> Result<LegalRecordAccess, Error> {
        require_transaction(self.connection).await?;
        if actor.holding_id != self.holding_id || actor.company_id != self.company_id {
            return Err(not_found().into());
        }
        let (facts, source) = read_facts(self.connection, actor, request).await?;
        let release = self
            .external
            .release(self.connection, &actor.company_id, now)
            .await?;
        let requirements = authorize_legal_record(actor, request, &facts, &release)?;
        let purchase = self
            .external
            .purchases(
                self.connection,
                &actor.company_id,
                &requirements,
                request.mode,
                now,
            )
            .await?;
        if !purchase.allowed {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "This Legal source requires its purchased capability or retained history.",
            )
            .into());
        }
        if source.len() != 64
            || release.revision.is_empty()
            || release.revision.len() > 256
            || purchase.revision.is_empty()
            || purchase.revision.len() > 256
        {
            return Err(PersistenceError::new("LEGAL_POLICY_SOURCE_REQUIRED").into());
        }
        Ok(LegalRecordAccess {
            item: facts.item,
            document: facts.document,
            held: facts.held,
            witness: LegalRecordWitness {
                actor: actor.clone(),
                request: request.clone(),
                transaction: transaction(self.connection).await?,
                source,
                release,
                purchase,
            },
        })
    }
    pub async fn revalidate(
        &mut self,
        witness: &LegalRecordWitness,
        now: i64,
    ) -> Result<LegalRecordAccess, Error> {
        require_transaction(self.connection).await?;
        if transaction(self.connection).await? != witness.transaction {
            return Err(changed().into());
        }
        let current = self.inspect(&witness.actor, &witness.request, now).await?;
        if current.witness.source != witness.source
            || current.witness.release != witness.release
            || current.witness.purchase != witness.purchase
        {
            return Err(changed().into());
        }
        Ok(current)
    }
}
async fn require_transaction(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
async fn transaction(connection: &mut PgConnection) -> Result<(String, i32, String), Error> {
    Ok(
        sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text")
            .fetch_one(connection)
            .await?,
    )
}
async fn read_facts(
    connection: &mut PgConnection,
    actor: &LegalRecordActor,
    request: &LegalRecordRequest,
) -> Result<(LegalRecordFacts, String), Error> {
    let item = sqlx::query("SELECT * FROM legal_items WHERE id=$1 AND company_id=$2 FOR SHARE")
        .bind(request.item_id.as_str())
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(not_found)?;
    let summary = LegalRecordSummary {
        id: RecordId::new(item.try_get::<String, _>("id")?)?,
        company_id: CompanyId::new(item.try_get::<String, _>("company_id")?)?,
        branch_id: item
            .try_get::<Option<String>, _>("branch_id")?
            .map(RecordId::new)
            .transpose()?,
        department_id: item
            .try_get::<Option<String>, _>("department_id")?
            .map(RecordId::new)
            .transpose()?,
        kind: item.try_get("kind")?,
        classification: item.try_get("classification")?,
        created_by: UserId::new(item.try_get::<String, _>("created_by")?)?,
        state: item.try_get("state")?,
        version: i64::from(item.try_get::<i32, _>("version")?),
    };
    // Known tuples are fenced as well as the pre-snapshot source tables. An
    // updated/deleted membership or setting cannot survive an older snapshot.
    let members =
        sqlx::query("SELECT user_id,access FROM legal_members WHERE item_id=$1 FOR SHARE")
            .bind(request.item_id.as_str())
            .fetch_all(&mut *connection)
            .await?;
    let mut member = false;
    let mut editor = false;
    for row in members {
        if row.try_get::<String, _>("user_id")? == actor.user_id.as_str() {
            member = true;
            editor = row.try_get::<String, _>("access")? == "edit";
        }
    }
    let settings =
        sqlx::query("SELECT enabled,ai_enabled FROM legal_settings WHERE company_id=$1 FOR SHARE")
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *connection)
            .await?;
    let enabled = settings
        .as_ref()
        .map(|row| row.try_get("enabled"))
        .transpose()?
        .unwrap_or(false);
    let ai_enabled = settings
        .as_ref()
        .map(|row| row.try_get("ai_enabled"))
        .transpose()?
        .unwrap_or(false);
    let document = if let Some(id) = &request.document_id {
        sqlx::query("SELECT * FROM legal_documents WHERE id=$1 AND item_id=$2 FOR SHARE")
            .bind(id.as_str())
            .bind(request.item_id.as_str())
            .fetch_optional(&mut *connection)
            .await?
            .map(|row| -> Result<_, Error> {
                Ok(LegalRecordDocument {
                    id: RecordId::new(row.try_get::<String, _>("id")?)?,
                    item_id: RecordId::new(row.try_get::<String, _>("item_id")?)?,
                    version: i64::from(row.try_get::<i32, _>("version")?),
                    checksum_sha256: row.try_get("checksum")?,
                    media_type: row.try_get("media_type")?,
                    bytes: row.try_get("bytes")?,
                    attachment_link_id: row
                        .try_get::<Option<String>, _>("attachment_link_id")?
                        .map(RecordId::new)
                        .transpose()?,
                    attachment_source_id: row
                        .try_get::<Option<String>, _>("attachment_source_id")?
                        .map(RecordId::new)
                        .transpose()?,
                })
            })
            .transpose()?
    } else {
        None
    };
    let state=sqlx::query("SELECT EXISTS(SELECT 1 FROM legal_holds h WHERE h.state<>'released' AND(h.item_id=$1 OR EXISTS(SELECT 1 FROM legal_hold_targets t WHERE t.hold_id=h.id AND t.item_id=$1))) held,EXISTS(SELECT 1 FROM legal_disposals WHERE document_id=$2) disposed,EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=$1 AND state='executing') executing,EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=$1 AND state='completed') completed")
        .bind(request.item_id.as_str()).bind(request.document_id.as_ref().map(|id|id.as_str())).fetch_one(&mut *connection).await?;
    let source = source_revision(connection, request).await?;
    Ok((
        LegalRecordFacts {
            item: summary,
            member,
            editor,
            enabled,
            ai_enabled,
            held: state.try_get("held")?,
            disposed: state.try_get("disposed")?,
            destruction_executing: state.try_get("executing")?,
            destruction_completed: state.try_get("completed")?,
            document,
        },
        source,
    ))
}
async fn source_revision(
    connection: &mut PgConnection,
    request: &LegalRecordRequest,
) -> Result<String, Error> {
    // Match the current complete owner source, excluding inspection/access
    // append-only outputs which this operation itself is allowed to retain.
    let tables = [
        "legal_members",
        "legal_documents",
        "legal_shares",
        "legal_requests",
        "legal_matters",
        "legal_contracts",
        "legal_obligations",
        "legal_compliance",
        "legal_licenses",
        "legal_corporate",
        "legal_ip_assets",
        "legal_privacy",
        "legal_counsel",
        "legal_templates",
        "legal_rules",
        "legal_executed_copies",
        "legal_approvals",
        "legal_approval_routes",
        "legal_authorities",
        "legal_retention_assignments",
        "legal_destructions",
        "legal_case_events",
        "legal_compliance_actions",
        "legal_privacy_cases",
        "legal_corporate_actions",
        "legal_ip_actions",
        "legal_counsel_entries",
        "legal_obligation_schedule",
        "legal_custody_events",
        "legal_policy_acknowledgements",
        "legal_intake_questions",
    ];
    let fields=tables.iter().map(|table|format!("'{table}',(SELECT jsonb_agg(to_jsonb(s) ORDER BY to_jsonb(s)::text) FROM {table} s WHERE s.item_id=i.id)")).collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT encode(sha256(convert_to(jsonb_build_object('item',to_jsonb(i),'settings',(SELECT to_jsonb(s) FROM legal_settings s WHERE s.company_id=i.company_id),'holds',(SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id) FROM legal_holds h WHERE h.item_id=i.id OR EXISTS(SELECT 1 FROM legal_hold_targets t WHERE t.hold_id=h.id AND t.item_id=i.id)),'holdTargets',(SELECT jsonb_agg(to_jsonb(t) ORDER BY t.hold_id,t.item_id) FROM legal_hold_targets t WHERE t.item_id=i.id),'externalUploads',(SELECT jsonb_agg(to_jsonb(u) ORDER BY u.id) FROM legal_external_uploads u JOIN legal_shares share ON share.id=u.share_id WHERE share.item_id=i.id),'disposals',(SELECT jsonb_agg(to_jsonb(x) ORDER BY x.document_id) FROM legal_disposals x JOIN legal_documents d ON d.id=x.document_id WHERE d.item_id=i.id),'retentionPolicies',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM legal_retention_policies p JOIN legal_retention_assignments a ON a.policy_id=p.id WHERE a.item_id=i.id),{fields})::text,'UTF8')),'hex') FROM legal_items i WHERE i.id=$1"
    );
    // Audited identifiers above are compile-owned literals only; every caller
    // value remains a bind parameter. No kind/table/request text is interpolated.
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()))
        .bind(request.item_id.as_str())
        .fetch_one(connection)
        .await?)
}
