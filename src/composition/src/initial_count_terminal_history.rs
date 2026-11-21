//! Count terminal and Files version-chain source decision.
//!
//! This is a source predicate, never Count, Finance, IAM or purchase authority.
//! The coordinator supplies the actual selected Count from retained owner
//! evidence and must acquire every owner fence before its first snapshot.
use erpx_server_files::public::count_history_attachment_sources::CurrentCountHistoryAttachmentSources;
use erpx_server_inventory::public::count_terminal_history_source::CurrentCountTerminalHistorySource;
use erpx_server_inventory::public::initial_count_history_source::CurrentInitialCountHistorySource;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use erpx_server_workflow::public::record_origin::CurrentInitialCountHistoryReceiptJoin;
use sqlx::{AssertSqlSafe, PgConnection};

type Error = PgTransactionError<BusinessError>;

// Mirrors the Count branch of supplyInventoryTerminalReceipts and the Files
// recordAttachmentVersionSteps source. JSON casts and recursive UNION retain
// PostgreSQL's NULL/error/collation semantics; no candidate cap is introduced.
const TERMINALS: &str = r#", count_history_bindings AS (
 SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version
 FROM count_history_attachment_sets source
 JOIN count_history_attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN initial_count_history_support_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))
), count_history_terminals AS (
 SELECT count.id,count.company_id,count.status,count.version,event.reviewed_version+1 terminal_version
 FROM initial_count_history_counts count JOIN count_history_decisions event ON event.count_id=count.id
 JOIN initial_count_history_support_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
 AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
)
SELECT EXISTS(SELECT 1 FROM count_history_terminals terminal
 WHERE terminal.id=$1 AND terminal.company_id=$2 AND terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS (SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN count_history_bindings binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
 AND binding.target_type IN ('stock_count','inventory_stock_count')
 WHERE binding.next_version<=terminal.version)
 SELECT 1 FROM versions WHERE version=terminal.version))"#;

pub struct CurrentInitialCountTerminalHistory<'a> {
    counts: &'a CurrentInitialCountHistorySource,
    decisions: &'a CurrentCountTerminalHistorySource,
    receipts: &'a CurrentInitialCountHistoryReceiptJoin,
    files: &'a CurrentCountHistoryAttachmentSources,
    company: CompanyId,
    count_id: String,
    matches: bool,
}
impl CurrentInitialCountTerminalHistory<'_> {
    pub fn matches(&self) -> bool {
        self.matches
    }
    /// Recompute the full joined decision after deferred constraints and all
    /// owner effects; the source fences alone do not retain row contents.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if evaluate(
            c,
            self.counts,
            self.decisions,
            self.receipts,
            self.files,
            &self.company,
            &self.count_id,
        )
        .await?
            != self.matches
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COUNT_TERMINAL_HISTORY_CHANGED",
                "The Count terminal History source changed.",
            )
            .into());
        }
        Ok(())
    }
}

/// `count_id` is a server-derived candidate from a retained Inventory/Finance
/// source, not a client-provided authority. This result grants no access.
pub async fn inspect_initial_count_terminal_history<'a>(
    c: &mut PgConnection,
    counts: &'a CurrentInitialCountHistorySource,
    decisions: &'a CurrentCountTerminalHistorySource,
    receipts: &'a CurrentInitialCountHistoryReceiptJoin,
    files: &'a CurrentCountHistoryAttachmentSources,
    company: &CompanyId,
    count_id: &str,
) -> Result<CurrentInitialCountTerminalHistory<'a>, Error> {
    let matches = evaluate(c, counts, decisions, receipts, files, company, count_id).await?;
    Ok(CurrentInitialCountTerminalHistory {
        counts,
        decisions,
        receipts,
        files,
        company: company.clone(),
        count_id: count_id.to_owned(),
        matches,
    })
}

async fn evaluate(
    c: &mut PgConnection,
    counts: &CurrentInitialCountHistorySource,
    decisions: &CurrentCountTerminalHistorySource,
    receipts: &CurrentInitialCountHistoryReceiptJoin,
    files: &CurrentCountHistoryAttachmentSources,
    company: &CompanyId,
    count_id: &str,
) -> Result<bool, Error> {
    counts.require_current(&mut *c).await?;
    decisions.require_current(&mut *c).await?;
    receipts.require_current(&mut *c).await?;
    files.require_current(&mut *c).await?;
    let sql = format!(
        "WITH {}, {}, {}, {}{}",
        counts.cte_sql(),
        decisions.cte_sql(),
        receipts.cte_sql(),
        files.cte_sql(),
        TERMINALS
    );
    let matches = sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(count_id)
        .bind(company.as_str())
        .fetch_one(&mut *c)
        .await?;
    counts.require_current(&mut *c).await?;
    decisions.require_current(&mut *c).await?;
    receipts.require_current(&mut *c).await?;
    files.require_current(c).await?;
    Ok(matches)
}
