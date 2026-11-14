//! PostgreSQL adapter for the native auxiliary Legal reads.
//!
//! The caller supplies one serializable Holding transaction and the current
//! Account session scope. Every statement re-applies the retained Company,
//! branch, department and confidential-membership visibility; no broad role or
//! package claim substitutes for it. Purchase history admission stays with the
//! composition transport exactly as it does for the native record reads.
use crate::{
    application::auxiliary::{
        LegalAuxiliaryError, LegalAuxiliaryFuture, LegalAuxiliaryReadPorts,
    },
    domain::auxiliary::*,
    domain::legal_records::sha256_hex,
};
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

/// Feature categories retained by the current reports view. These are
/// compile-owned table names; no request value selects a relation.
const FEATURE_TABLES: [(&str, &str); 10] = [
    ("case_event", "legal_case_events"),
    ("compliance_action", "legal_compliance_actions"),
    ("privacy_case", "legal_privacy_cases"),
    ("corporate_action", "legal_corporate_actions"),
    ("ip_action", "legal_ip_actions"),
    ("counsel_entry", "legal_counsel_entries"),
    ("schedule", "legal_obligation_schedule"),
    ("custody", "legal_custody_events"),
    ("policy_ack", "legal_policy_acknowledgements"),
    ("intake", "legal_intake_questions"),
];

const CLOSED_STATES: &str =
    "('closed','completed','expired','terminated','archived','cancelled')";

const CLEAN_DOCUMENT: &str = "(d.object_key IS NULL OR EXISTS(\
 SELECT 1 FROM attachment_links l JOIN attachments f ON f.id=l.attachment_id \
 WHERE l.id=d.attachment_link_id AND l.state='bound' AND f.company_id=i.company_id \
 AND f.source_state='available' AND f.revoked_at IS NULL AND f.checksum_sha256=d.checksum)) \
 AND NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=d.id) \
 AND NOT EXISTS(SELECT 1 FROM legal_destructions destruction \
 WHERE destruction.item_id=i.id AND destruction.state IN ('executing','completed'))";

const LAW_AVAILABILITY: &str = "r.state='approved' AND r.effective_from<=$7 \
 AND (r.effective_until IS NULL OR r.effective_until>$8) AND s.state='active' \
 AND i.state NOT IN ('archived','cancelled','rejected') \
 AND (d.object_key IS NULL OR EXISTS(SELECT 1 FROM attachment_links l JOIN attachments f ON f.id=l.attachment_id \
  WHERE l.id=d.attachment_link_id AND l.state='bound' \
  AND f.company_id=i.company_id AND f.source_state='available' AND f.revoked_at IS NULL \
  AND f.checksum_sha256=d.checksum)) \
 AND NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=d.id) \
 AND NOT EXISTS(SELECT 1 FROM legal_destructions destruction \
  WHERE destruction.item_id=i.id AND destruction.state IN ('executing','completed')) \
 AND (SELECT ap.applies FROM legal_law_applicability ap \
  WHERE ap.company_id=i.company_id AND ap.revision_id=r.id AND ap.state='approved' \
  ORDER BY ap.created_at DESC,ap.id DESC LIMIT 1)=true";

const UNAMBIGUOUS_REVISION: &str = "(SELECT count(*) FROM legal_law_revisions conflicting \
 WHERE conflicting.source_id=r.source_id AND conflicting.state='approved' \
 AND conflicting.effective_from<=$9 \
 AND (conflicting.effective_until IS NULL OR conflicting.effective_until>$10))=1";

const LAW_FIELDS: &str = "'id',c.id,'sourceId','law:'||c.id,'kind','law','item_id',i.id,\
 'title',i.title,'document_id',d.id,'version',d.version,'checksum',d.checksum,\
 'text',c.text,'excerpt',c.text,\
 'page',CASE WHEN c.page_mapping THEN c.page ELSE NULL END,\
 'page_mapping',c.page_mapping,'section',c.section,'revision_id',r.id,\
 'revision_label',r.revision_label,'source_url',s.source_url,\
 'published_on',r.published_on,'effective_from',r.effective_from,\
 'effective_until',r.effective_until";

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

pub struct PgLegalAuxiliaryReads<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgLegalAuxiliaryReads<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    /// Composition may use this to keep one read and its access audit in the
    /// caller's fresh unit; it never commits or opens a pool checkout.
    pub fn connection(&mut self) -> &mut PgConnection {
        self.connection
    }

    fn visibility(&self, actor: &LegalAuxiliaryActor) -> String {
        visibility_for(actor, "i")
    }

    async fn ready(&mut self) -> Result<(), LegalAuxiliaryError> {
        require_transaction(self.connection).await
    }

    async fn require_item(
        &mut self,
        actor: &LegalAuxiliaryActor,
        item_id: &str,
    ) -> Result<(), LegalAuxiliaryError> {
        self.require_item_alias(actor, item_id, "i").await
    }

    async fn require_item_alias(
        &mut self,
        actor: &LegalAuxiliaryActor,
        item_id: &str,
        alias: &str,
    ) -> Result<(), LegalAuxiliaryError> {
        let sql = format!(
            "SELECT 1 FROM legal_items {alias} WHERE {alias}.id=$5 AND {}",
            visibility_for(actor, alias)
        );
        let found = sqlx::query_scalar::<_, i32>(AssertSqlSafe(sql))
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(actor.user_id.as_str())
            .bind(item_id)
            .fetch_optional(&mut *self.connection)
            .await?;
        if found.is_none() {
            return Err(not_found());
        }
        Ok(())
    }

    async fn rows(
        &mut self,
        sql: &str,
        company: &str,
        branch: Option<&str>,
        department: Option<&str>,
        user: &str,
    ) -> Result<Vec<Value>, LegalAuxiliaryError> {
        let rows = sqlx::query(AssertSqlSafe(sql))
            .bind(company)
            .bind(branch)
            .bind(department)
            .bind(user)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| Ok(row.try_get::<Value, _>("x")?))
            .collect()
    }

    /// Read audit for law/knowledge evidence, matching `legal_access_events`.
    /// Identifiers are derived from the exact source and operation so a replay
    /// of the same read in the same millisecond cannot collide silently.
    async fn record_reads(
        &mut self,
        actor: &LegalAuxiliaryActor,
        sources: &[Value],
        action: &str,
        now: i64,
    ) -> Result<(), LegalAuxiliaryError> {
        let mut seen = std::collections::BTreeSet::new();
        for source in sources {
            let Some(document_id) = source.get("document_id").and_then(Value::as_str) else {
                continue;
            };
            if !seen.insert(document_id.to_owned()) {
                continue;
            }
            let Some(item_id) = source.get("item_id").and_then(Value::as_str) else {
                continue;
            };
            let identifier = format!(
                "legal-access-{}",
                &sha256_hex(format!("{action}:{document_id}:{}:{now}", actor.user_id.as_str()).as_bytes())
                    [..40]
            );
            sqlx::query(
                "INSERT INTO legal_access_events(id,item_id,document_id,actor_id,action,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
            )
            .bind(identifier)
            .bind(item_id)
            .bind(document_id)
            .bind(actor.user_id.as_str())
            .bind(action)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
        }
        Ok(())
    }
}

fn not_found() -> LegalAuxiliaryError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.").into()
}

async fn require_transaction(connection: &mut PgConnection) -> Result<(), LegalAuxiliaryError> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

impl LegalAuxiliaryReadPorts for PgLegalAuxiliaryReads<'_> {
    fn workspace<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let visibility = self.visibility(actor);
            let deadlines = format!(
                "SELECT to_jsonb(x) x FROM (SELECT i.id,i.kind,i.title,i.state,i.due_date,i.owner_id,i.version::bigint version \
                 FROM legal_items i WHERE {visibility} AND i.due_date IS NOT NULL \
                 AND i.state NOT IN ('archived','cancelled','closed','completed','terminated') \
                 ORDER BY i.due_date,i.id LIMIT 100) x"
            );
            let assigned = format!(
                "SELECT to_jsonb(x) x FROM (SELECT i.id,i.kind,i.title,i.state,i.due_date,i.owner_id,i.version::bigint version \
                 FROM legal_items i WHERE {visibility} AND i.owner_id=$5 \
                 AND i.state NOT IN ('archived','cancelled','closed','completed','terminated') \
                 ORDER BY i.due_date NULLS LAST,i.updated_at DESC LIMIT 100) x"
            );
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let department = actor.department_id.as_ref().map(|value| value.as_str());
            let deadlines = self
                .rows(&deadlines, actor.company_id.as_str(), branch, department, actor.user_id.as_str())
                .await?;
            let assigned = self.query_owner(&assigned, actor).await?;
            Ok(json!({"deadlines": deadlines, "assigned": assigned}))
        })
    }

    fn reports<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        now: i64,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let visibility = self.visibility(actor);
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let department = actor.department_id.as_ref().map(|value| value.as_str());
            let company = actor.company_id.as_str().to_owned();
            let user = actor.user_id.as_str().to_owned();
            let registers = format!(
                "SELECT to_jsonb(x) x FROM (SELECT i.kind,i.state,count(*) count,\
                 count(*) FILTER(WHERE i.due_date::date<(clock_timestamp() AT TIME ZONE i.timezone)::date \
                 AND i.state NOT IN ('closed','completed','expired','terminated','archived','cancelled')) overdue,\
                 count(*) FILTER(WHERE i.sla_due_at<$5 AND i.state NOT IN ('closed','completed','archived','cancelled')) sla_breached \
                 FROM legal_items i WHERE {visibility} GROUP BY i.kind,i.state ORDER BY i.kind,i.state) x"
            );
            let registers = sqlx::query(AssertSqlSafe(registers.as_str()))
                .bind(&company)
                .bind(branch)
                .bind(department)
                .bind(&user)
                .bind(now)
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(|row| row.try_get::<Value, _>("x"))
                .collect::<Result<Vec<_>, _>>()?;
            let upcoming = format!(
                "SELECT to_jsonb(x) x FROM (SELECT i.id,i.title,i.kind,i.due_date,i.owner_id,i.state \
                 FROM legal_items i WHERE {visibility} \
                 AND i.due_date::date BETWEEN (clock_timestamp() AT TIME ZONE i.timezone)::date \
                 AND (clock_timestamp() AT TIME ZONE i.timezone)::date+90 \
                 AND i.state NOT IN {CLOSED_STATES} ORDER BY i.due_date,i.id LIMIT 100) x"
            );
            let upcoming = self
                .rows(&upcoming, &company, branch, department, &user)
                .await?;
            let mut workflows = Vec::with_capacity(FEATURE_TABLES.len());
            for (category, table) in FEATURE_TABLES {
                let sql = format!(
                    "SELECT to_jsonb(x) x FROM (SELECT f.state,count(*) count,\
                     count(*) FILTER(WHERE f.due_date::date<(clock_timestamp() AT TIME ZONE i.timezone)::date) overdue \
                     FROM {table} f JOIN legal_items i ON i.id=f.item_id WHERE {visibility} GROUP BY f.state) x"
                );
                let rows = self.rows(&sql, &company, branch, department, &user).await?;
                workflows.push(json!({"category": category, "rows": rows}));
            }
            let costs = format!(
                "SELECT to_jsonb(x) x FROM (SELECT i.currency,\
                 sum(COALESCE(NULLIF(c.data->>'budget',''),'0')::numeric) budget_minor,\
                 sum(COALESCE(NULLIF(c.data->>'fees',''),'0')::numeric) fees_minor \
                 FROM legal_counsel c JOIN legal_items i ON i.id=c.item_id WHERE {visibility} GROUP BY i.currency) x"
            );
            let costs = self.rows(&costs, &company, branch, department, &user).await?;
            let access = format!(
                "SELECT to_jsonb(x) x FROM (SELECT i.id,i.title,i.classification,\
                 count(DISTINCT m.user_id) members,\
                 count(DISTINCT s.id) FILTER(WHERE s.revoked_at IS NULL AND s.expires_at>$5) external_shares \
                 FROM legal_items i LEFT JOIN legal_members m ON m.item_id=i.id \
                 LEFT JOIN legal_shares s ON s.item_id=i.id WHERE {visibility} \
                 GROUP BY i.id ORDER BY i.title LIMIT 200) x"
            );
            let access = sqlx::query(AssertSqlSafe(access.as_str()))
                .bind(&company)
                .bind(branch)
                .bind(department)
                .bind(&user)
                .bind(now)
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(|row| row.try_get::<Value, _>("x"))
                .collect::<Result<Vec<_>, _>>()?;
            let jobs = format!(
                "SELECT to_jsonb(x) x FROM (SELECT p.id,p.item_id,p.kind,p.state,p.error,p.attempts,p.created_at \
                 FROM legal_processing_jobs p JOIN legal_items i ON i.id=p.item_id WHERE {visibility} \
                 ORDER BY p.created_at DESC LIMIT 100) x"
            );
            let jobs = self.rows(&jobs, &company, branch, department, &user).await?;
            let invitations = format!(
                "SELECT to_jsonb(x) x FROM (SELECT j.id,s.item_id,j.state,j.error,j.attempts,j.created_at \
                 FROM legal_invitation_jobs j JOIN legal_shares s ON s.id=j.share_id \
                 JOIN legal_items i ON i.id=s.item_id WHERE {visibility} \
                 ORDER BY j.created_at DESC LIMIT 100) x"
            );
            let invitations = self.rows(&invitations, &company, branch, department, &user).await?;
            Ok(json!({
                "asOf": now,
                "registers": registers,
                "upcoming": upcoming,
                "workflows": workflows,
                "costs": costs,
                "access": access,
                "jobs": jobs,
                "invitations": invitations,
            }))
        })
    }

    fn retention<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: Option<&'a str>,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let policies = sqlx::query(
                "SELECT to_jsonb(p) x FROM legal_retention_policies p \
                 WHERE p.company_id=$1 ORDER BY p.created_at DESC,p.id",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| row.try_get::<Value, _>("x"))
            .collect::<Result<Vec<_>, _>>()?;
            let (assignment, requests) = if let Some(id) = id {
                self.require_item(actor, id).await?;
                let assignment = sqlx::query("SELECT to_jsonb(a) x FROM legal_retention_assignments a WHERE a.item_id=$1")
                    .bind(id)
                    .fetch_optional(&mut *self.connection)
                    .await?
                    .map(|row| row.try_get::<Value, _>("x"))
                    .transpose()?;
                let requests = sqlx::query(
                    "SELECT to_jsonb(d) x FROM legal_destructions d \
                     WHERE d.item_id=$1 ORDER BY d.created_at DESC,d.id",
                )
                .bind(id)
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(|row| row.try_get::<Value, _>("x"))
                .collect::<Result<Vec<_>, _>>()?;
                (assignment, requests)
            } else {
                (None, Vec::new())
            };
            Ok(json!({"policies": policies, "assignment": assignment, "requests": requests}))
        })
    }

    fn governance<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: Option<&'a str>,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let forms = sqlx::query(
                "SELECT to_jsonb(f) x FROM legal_intake_forms f \
                 WHERE f.company_id=$1 ORDER BY f.id",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| row.try_get::<Value, _>("x"))
            .collect::<Result<Vec<_>, _>>()?;
            let authorities = if let Some(id) = id {
                self.require_item(actor, id).await?;
                sqlx::query("SELECT to_jsonb(a) x FROM legal_authorities a WHERE a.item_id=$1 ORDER BY a.id")
                    .bind(id)
                    .fetch_all(&mut *self.connection)
                    .await?
                    .into_iter()
                    .map(|row| row.try_get::<Value, _>("x"))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Vec::new()
            };
            let settings = sqlx::query("SELECT to_jsonb(s) x FROM legal_settings s WHERE s.company_id=$1")
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?
                .map(|row| row.try_get::<Value, _>("x"))
                .transpose()?;
            Ok(json!({"forms": forms, "authorities": authorities, "settings": settings}))
        })
    }

    fn law_corpus<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        rule_id: Option<&'a str>,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            if let Some(rule_id) = rule_id {
                self.require_item(actor, rule_id).await?;
            }
            let visibility = self.visibility(actor);
            let rule_visibility = visibility_for(actor, "rule");
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let department = actor.department_id.as_ref().map(|value| value.as_str());
            let company = actor.company_id.as_str().to_owned();
            let user = actor.user_id.as_str().to_owned();
            let mut collected: Vec<(&'static str, Vec<Value>)> = Vec::new();
            for (key, table, column, fields) in [
                ("sources", "legal_law_sources", "rule_id", "c.*"),
                ("revisions", "legal_law_revisions", "rule_id", "c.*"),
                ("applicability", "legal_law_applicability", "rule_id", "c.*"),
                ("textReviews", "legal_text_reviews", "item_id", "c.*"),
            ] {
                let sql = format!(
                    "SELECT to_jsonb(x) x FROM (SELECT {fields},i.title record_title FROM {table} c \
                     JOIN legal_items i ON i.id=c.{column} WHERE {visibility} \
                     {filter} ORDER BY c.created_at DESC,c.id DESC LIMIT {AUXILIARY_ROW_LIMIT}) x",
                    filter = if rule_id.is_some() { "AND i.id=$5" } else { "" }
                );
                let rows = self
                    .rows_with_rule(&sql, &company, branch, department, &user, rule_id)
                    .await?;
                collected.push((key, rows));
            }
            let checks = format!(
                "SELECT to_jsonb(x) x FROM (SELECT c.id,c.source_id,c.company_id,c.rule_id,c.check_date,\
                 c.state,c.attempts,c.checked_at,c.checksum,c.error,c.candidate_revision_id,c.created_at,\
                 i.title record_title FROM legal_law_source_checks c JOIN legal_items i ON i.id=c.rule_id \
                 WHERE {visibility} {filter} ORDER BY c.created_at DESC,c.id DESC LIMIT {AUXILIARY_ROW_LIMIT}) x",
                filter = if rule_id.is_some() { "AND i.id=$5" } else { "" }
            );
            let checks = self
                .rows_with_rule(&checks, &company, branch, department, &user, rule_id)
                .await?;
            let impacts = format!(
                "SELECT to_jsonb(x) x FROM (SELECT c.id check_id,i.id item_id,i.title,i.state,i.due_date,\
                 i.version::bigint version FROM legal_law_source_checks c \
                 JOIN legal_items rule ON rule.id=c.rule_id \
                 JOIN legal_links l ON l.source_id=rule.id \
                 JOIN legal_items i ON i.id=l.target_id \
                 WHERE c.state='pending_review' AND {rule_visibility} AND {visibility} \
                 {filter} ORDER BY c.created_at DESC,i.id LIMIT {AUXILIARY_ROW_LIMIT}) x",
                filter = if rule_id.is_some() { "AND rule.id=$5" } else { "" }
            );
            let mut impacts_query = sqlx::query(AssertSqlSafe(impacts.as_str()))
                .bind(&company)
                .bind(branch)
                .bind(department)
                .bind(&user);
            if let Some(rule_id) = rule_id {
                impacts_query = impacts_query.bind(rule_id);
            }
            let impacts = impacts_query
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(|row| row.try_get::<Value, _>("x"))
                .collect::<Result<Vec<_>, _>>()?;
            let chunks = format!(
                "SELECT to_jsonb(x) x FROM (SELECT c.*,i.title record_title FROM legal_source_chunks c \
                 JOIN legal_documents d ON d.id=c.document_id JOIN legal_items i ON i.id=c.item_id \
                 WHERE {visibility} {filter} AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=c.document_id) \
                 AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=i.id AND x.state IN ('executing','completed')) \
                 AND (d.object_key IS NULL OR EXISTS(SELECT 1 FROM attachment_links l JOIN attachments f ON f.id=l.attachment_id \
                 WHERE l.id=d.attachment_link_id AND l.state='bound' AND f.company_id=i.company_id \
                 AND f.source_state='available' AND f.revoked_at IS NULL AND f.checksum_sha256=d.checksum)) \
                 ORDER BY c.document_id,c.page,c.id LIMIT {AUXILIARY_CHUNK_LIMIT}) x",
                filter = if rule_id.is_some() { "AND i.id=$5" } else { "" }
            );
            let chunks = self
                .rows_with_rule(&chunks, &company, branch, department, &user, rule_id)
                .await?;
            let mut sources = Vec::new();
            let mut revisions = Vec::new();
            let mut applicability = Vec::new();
            let mut text_reviews = Vec::new();
            for (key, rows) in collected {
                match key {
                    "sources" => sources = rows,
                    "revisions" => revisions = rows,
                    "applicability" => applicability = rows,
                    "textReviews" => text_reviews = rows,
                    _ => {}
                }
            }
            Ok(json!({
                "sources": sources,
                "revisions": revisions,
                "applicability": applicability,
                "textReviews": text_reviews,
                "checks": checks,
                "chunks": chunks,
                "impacts": impacts,
                "notice": "Official-source changes and extracted text remain pending until independently reviewed. Existing deadlines require an explicit owner decision.",
            }))
        })
    }

    fn knowledge<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        query: &'a str,
        as_of: &'a str,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let visibility = self.visibility(actor);
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let department = actor.department_id.as_ref().map(|value| value.as_str());
            let company = actor.company_id.as_str().to_owned();
            let user = actor.user_id.as_str().to_owned();
            // Binds 1-4 are the exact retained visibility; the search term
            // follows at 5-6 and the Gregorian date at 7-10 so the same
            // statements also serve the citation read.
            let law_sql = format!(
                "SELECT to_jsonb(x) x FROM (SELECT jsonb_build_object({LAW_FIELDS}) || \
                 jsonb_build_object('rank',ts_rank_cd(c.search,websearch_to_tsquery('simple',$5))+word_similarity($6,c.text)) x \
                 FROM legal_source_chunks c JOIN legal_law_revisions r ON r.id=c.revision_id \
                 JOIN legal_law_sources s ON s.id=r.source_id JOIN legal_documents d ON d.id=c.document_id \
                 JOIN legal_items i ON i.id=c.item_id WHERE {visibility} AND {LAW_AVAILABILITY} AND {UNAMBIGUOUS_REVISION} \
                 AND (c.search@@websearch_to_tsquery('simple',$11) OR c.text %> $12 OR strpos(lower(c.text),lower($13))>0) \
                 ORDER BY ts_rank_cd(c.search,websearch_to_tsquery('simple',$5))+word_similarity($6,c.text) DESC,\
                 r.effective_from DESC,c.id LIMIT 30) s"
            );
            let law_sources = sqlx::query(AssertSqlSafe(law_sql.as_str()))
                .bind(&company)
                .bind(branch)
                .bind(department)
                .bind(&user)
                .bind(query)
                .bind(query)
                .bind(as_of)
                .bind(as_of)
                .bind(as_of)
                .bind(as_of)
                .bind(query)
                .bind(query)
                .bind(query)
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(|row| row.try_get::<Value, _>("x"))
                .collect::<Result<Vec<_>, _>>()?;
            let conflicts_sql = format!(
                "SELECT count(DISTINCT r.source_id)::bigint FROM legal_law_revisions r \
                 JOIN legal_law_sources s ON s.id=r.source_id JOIN legal_documents d ON d.id=r.document_id \
                 JOIN legal_items i ON i.id=r.rule_id WHERE {visibility} AND {LAW_AVAILABILITY} \
                 AND NOT {UNAMBIGUOUS_REVISION}"
            );
            let conflicts = sqlx::query_scalar::<_, i64>(AssertSqlSafe(conflicts_sql.as_str()))
                .bind(&company)
                .bind(branch)
                .bind(department)
                .bind(&user)
                .bind("")
                .bind("")
                .bind(as_of)
                .bind(as_of)
                .bind(as_of)
                .bind(as_of)
                .fetch_one(&mut *self.connection)
                .await?;
            let reviewed_sql = format!(
                "SELECT to_jsonb(x) x FROM (SELECT c.id,'document:'||c.id \"sourceId\",'document' kind,\
                 i.id item_id,i.title,d.id document_id,d.name,d.version::bigint version,d.checksum,\
                 c.text,c.text excerpt,CASE WHEN c.page_mapping THEN c.page ELSE NULL END page,\
                 c.page_mapping,c.section,t.id text_review_id,\
                 ts_rank_cd(c.search,websearch_to_tsquery('simple',$5))+word_similarity($6,c.text) rank \
                 FROM legal_source_chunks c JOIN legal_text_reviews t ON t.id=c.text_review_id AND t.state='approved' \
                 JOIN legal_documents d ON d.id=c.document_id AND d.checksum=c.document_checksum \
                 JOIN legal_items i ON i.id=c.item_id WHERE {visibility} AND i.kind<>'rule' AND {CLEAN_DOCUMENT} \
                 AND (c.search@@websearch_to_tsquery('simple',$5) OR c.text %> $6 OR strpos(lower(c.text),lower($7))>0) \
                 ORDER BY ts_rank_cd(c.search,websearch_to_tsquery('simple',$5))+word_similarity($6,c.text) DESC,\
                 d.created_at DESC,c.id LIMIT 30) x"
            );
            let reviewed = sqlx::query(AssertSqlSafe(reviewed_sql.as_str()))
                .bind(&company)
                .bind(branch)
                .bind(department)
                .bind(&user)
                .bind(query)
                .bind(query)
                .bind(query)
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(|row| row.try_get::<Value, _>("x"))
                .collect::<Result<Vec<_>, _>>()?;
            let authored_sql = format!(
                "SELECT to_jsonb(x) x FROM (SELECT d.id,d.id \"sourceId\",'document' kind,i.id item_id,i.title,\
                 d.id document_id,d.name,d.version::bigint version,d.checksum,d.body text,\
                 substring(d.body from greatest(1,strpos(lower(d.body),lower($5))-120) for 500) excerpt,\
                 NULL::int page,'' section FROM legal_documents d JOIN legal_items i ON i.id=d.item_id \
                 WHERE {visibility} AND i.kind<>'rule' AND d.object_key IS NULL AND d.media_type='text/plain' \
                 AND {CLEAN_DOCUMENT} AND strpos(lower(d.body),lower($5))>0 \
                 ORDER BY i.updated_at DESC,d.version DESC LIMIT 20) x"
            );
            let authored = sqlx::query(AssertSqlSafe(authored_sql.as_str()))
                .bind(&company)
                .bind(branch)
                .bind(department)
                .bind(&user)
                .bind(query)
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(|row| row.try_get::<Value, _>("x"))
                .collect::<Result<Vec<_>, _>>()?;
            let mut documents = reviewed;
            documents.extend(authored);
            let mut sources = documents.clone();
            sources.extend(law_sources.clone());
            let now = now_millis();
            self.record_reads(actor, &law_sources, "law.knowledge.read", now).await?;
            self.record_reads(actor, &documents, "knowledge.read", now).await?;
            let notice = if conflicts > 0 {
                "Overlapping approved law revisions need independent review. Conflicting sources are excluded; no legal conclusion can be inferred for them."
            } else if !sources.is_empty() {
                "Review each cited immutable version and exact page or section before relying on its text. Extracted file text and official laws require independent review."
            } else {
                "No accessible reviewed source matched. Evidence is insufficient; request the missing documents or independently approved official law revision."
            };
            Ok(json!({
                "query": query,
                "asOf": as_of,
                "sources": sources,
                "conflicts": conflicts,
                "notice": notice,
            }))
        })
    }

    fn law_citation<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        chunk_id: &'a str,
        as_of: &'a str,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let visibility = self.visibility(actor);
            let sql = format!(
                "SELECT to_jsonb(x) x FROM (SELECT jsonb_build_object({LAW_FIELDS}) x \
                 FROM legal_source_chunks c JOIN legal_law_revisions r ON r.id=c.revision_id \
                 JOIN legal_law_sources s ON s.id=r.source_id JOIN legal_documents d ON d.id=c.document_id \
                 JOIN legal_items i ON i.id=c.item_id WHERE c.id=$11 AND {visibility} \
                 AND {LAW_AVAILABILITY} AND {UNAMBIGUOUS_REVISION} LIMIT 1) s"
            );
            let source = sqlx::query(AssertSqlSafe(sql.as_str()))
                .bind(actor.company_id.as_str())
                .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
                .bind(actor.department_id.as_ref().map(|value| value.as_str()))
                .bind(actor.user_id.as_str())
                .bind("")
                .bind("")
                .bind(as_of)
                .bind(as_of)
                .bind(as_of)
                .bind(as_of)
                .bind(chunk_id)
                .fetch_optional(&mut *self.connection)
                .await?
                .map(|row| row.try_get::<Value, _>("x"))
                .transpose()?;
            let Some(source) = source else {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "LEGAL_CITATION_UNAVAILABLE",
                    "The cited law source is unavailable, unapproved, conflicting or not applicable on this date.",
                )
                .into());
            };
            let now = now_millis();
            self.record_reads(actor, std::slice::from_ref(&source), "law.citation.read", now)
                .await?;
            let item_id = source.get("item_id").and_then(Value::as_str).unwrap_or_default();
            let source_url = source.get("source_url").and_then(Value::as_str).unwrap_or_default();
            let page = source.get("page").and_then(Value::as_i64);
            let href = url::form_urlencoded::byte_serialize(item_id.as_bytes()).collect::<String>();
            let original = if let Some(page) = page.filter(|_| source_url.ends_with(".pdf")) {
                format!("{source_url}#page={page}")
            } else {
                source_url.to_owned()
            };
            let mut reply = source;
            if let Some(object) = reply.as_object_mut() {
                object.insert("asOf".into(), json!(as_of));
                object.insert("href".into(), json!(format!("/legal?record={href}")));
                object.insert("originalUrl".into(), json!(original));
                object.insert(
                    "notice".into(),
                    json!("The quotation and original version are retained for review. Legal interpretation still requires an independent legal reviewer."),
                );
            }
            Ok(reply)
        })
    }

    fn references<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        item_id: &'a str,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            self.require_item(actor, item_id).await?;
            let rows = sqlx::query(
                "SELECT to_jsonb(x) x FROM (SELECT target_module,target_id FROM legal_native_references \
                 WHERE item_id=$1 ORDER BY ordinal LIMIT 50) x",
            )
            .bind(item_id)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| row.try_get::<Value, _>("x"))
            .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(rows))
        })
    }

    fn trust<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT to_jsonb(x) x FROM (SELECT id,kind,name,source,checksum,state,created_by,\
                 approved_by,version FROM legal_trust_material WHERE company_id=$1 \
                 ORDER BY created_at DESC,id) x",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| row.try_get::<Value, _>("x"))
            .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(rows))
        })
    }

    fn saved_views<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT to_jsonb(x) x FROM (SELECT name,query FROM legal_saved_views \
                 WHERE company_id=$1 AND user_id=$2 ORDER BY name LIMIT 100) x",
            )
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| row.try_get::<Value, _>("x"))
            .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(rows))
        })
    }

    fn ai_history<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        item_id: &'a str,
    ) -> LegalAuxiliaryFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            self.require_item(actor, item_id).await?;
            let rows = sqlx::query(
                "SELECT to_jsonb(x) x FROM (SELECT id,source_version,question,proposal,state,error,created_at \
                 FROM legal_ai_proposals WHERE item_id=$1 AND created_by=$2 \
                 ORDER BY created_at DESC,id LIMIT 30) x",
            )
            .bind(item_id)
            .bind(actor.user_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| row.try_get::<Value, _>("x"))
            .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(rows))
        })
    }
}

impl PgLegalAuxiliaryReads<'_> {
    // Kept as a separate name so the owner-bound `assigned` query is explicit
    // about its fifth bind parameter.
    async fn query_owner(
        &mut self,
        sql: &str,
        actor: &LegalAuxiliaryActor,
    ) -> Result<Vec<Value>, LegalAuxiliaryError> {
        let rows = sqlx::query(AssertSqlSafe(sql))
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(actor.user_id.as_str())
            .bind(actor.user_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| Ok(row.try_get::<Value, _>("x")?))
            .collect()
    }

    async fn rows_with_rule(
        &mut self,
        sql: &str,
        company: &str,
        branch: Option<&str>,
        department: Option<&str>,
        user: &str,
        rule_id: Option<&str>,
    ) -> Result<Vec<Value>, LegalAuxiliaryError> {
        let mut query = sqlx::query(AssertSqlSafe(sql))
            .bind(company)
            .bind(branch)
            .bind(department)
            .bind(user);
        if let Some(rule_id) = rule_id {
            query = query.bind(rule_id);
        }
        let rows = query.fetch_all(&mut *self.connection).await?;
        rows.into_iter()
            .map(|row| Ok(row.try_get::<Value, _>("x")?))
            .collect()
    }
}

/// Shared unavailable mapper for the owner HTTP adapter.
pub fn legal_auxiliary_business_error(error: LegalAuxiliaryError) -> BusinessError {
    match error {
        erpx_server_runtime::public::postgres::PgTransactionError::Business(error) => error,
        erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_) => {
            BusinessError::new(
                FailureKind::Unavailable,
                "LEGAL_UNAVAILABLE",
                "The Legal service is unavailable.",
            )
        }
    }
}

/// Guard kept next to the adapter so an unmounted caller cannot silently use a
/// non-serializable connection.
pub async fn require_auxiliary_transaction(
    connection: &mut PgConnection,
) -> Result<(), LegalAuxiliaryError> {
    require_transaction(connection).await
}
