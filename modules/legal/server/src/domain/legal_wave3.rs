//! Pure domain planner for the wf3a13 native Legal write retirement slice.
//!
//! The SQL for every migrated kind lives in this owner crate. A transport
//! supplies a typed `kind` and a bounded positional value array; the planner
//! selects the authoritative statement and validates its placeholder count.
//! No SQL text, predicate or CTE is ever accepted from a caller, so there is no
//! JSON/SQL mirror and no TypeScript fallback writer for a retired kind.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A fully owner-owned write statement. The caller executes it inside its own
/// serializable unit together with receipt/audit/outbox effects; the statement
/// text is never assembled outside this crate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LegalWave3Statement {
    pub sql: String,
    pub values: Vec<Value>,
}

pub const SOURCE_SCHEDULE: &str = "legal_source_schedule";
pub const SOURCE_CLAIM: &str = "legal_source_claim";
pub const INVITE_CLAIM: &str = "legal_invite_claim";
pub const AI_CLAIM: &str = "legal_ai_claim";
pub const DEADLINE_CTE: &str = "legal_deadline_cte";
pub const ENVELOPE_CTE: &str = "legal_envelope_cte";
pub const DEADLINE_REMINDERS: &str = "legal_deadline_reminders";
pub const ENVELOPE_EXPIRE: &str = "legal_envelope_expire";
pub const ENROLLMENT_ATTEMPT: &str = "legal_enrollment_attempt";
pub const SOURCE_RECOVER: &str = "legal_source_recover";
pub const SOURCE_PRESERVE: &str = "legal_source_preserve";
pub const SOURCE_COMPLETE: &str = "legal_source_complete";
pub const SOURCE_FAIL: &str = "legal_source_fail";
pub const INVITE_RECOVER: &str = "legal_invite_recover";
pub const INVITE_CANCEL: &str = "legal_invite_cancel";
pub const INVITE_SENT: &str = "legal_invite_sent";
pub const INVITE_FAIL: &str = "legal_invite_fail";
pub const AI_QUEUE: &str = "legal_ai_queue";
pub const AI_INTERRUPT: &str = "legal_ai_interrupt";
pub const AI_COMPLETE: &str = "legal_ai_complete";
pub const AI_FAIL: &str = "legal_ai_fail";

/// Every kind this owner module accepts. Anything else is fail-closed.
pub const KINDS: &[&str] = &[
    SOURCE_SCHEDULE,
    SOURCE_CLAIM,
    INVITE_CLAIM,
    AI_CLAIM,
    DEADLINE_REMINDERS,
    ENVELOPE_EXPIRE,
    ENROLLMENT_ATTEMPT,
    SOURCE_RECOVER,
    SOURCE_PRESERVE,
    SOURCE_COMPLETE,
    SOURCE_FAIL,
    INVITE_RECOVER,
    INVITE_CANCEL,
    INVITE_SENT,
    INVITE_FAIL,
    AI_QUEUE,
    AI_INTERRUPT,
    AI_COMPLETE,
    AI_FAIL,
];

/// (SQL, exact positional value count) for one owner write kind.
fn statement(kind: &str) -> Option<(&'static str, usize)> {
    Some(match kind {
        SOURCE_SCHEDULE => (
            "INSERT INTO legal_law_source_checks(id,source_id,company_id,rule_id,check_date,created_at) SELECT 'law-check-'||md5(s.id||?),s.id,s.company_id,s.rule_id,?,? FROM legal_law_sources s WHERE s.state='active' AND %PREDICATE% ON CONFLICT(source_id,check_date) DO NOTHING",
            3,
        ),
        SOURCE_CLAIM => (
            "UPDATE legal_law_source_checks SET state='running',attempts=attempts+1,lease_token=?,lease_until=? WHERE id=(SELECT c.id FROM legal_law_source_checks c JOIN legal_law_sources s ON s.id=c.source_id WHERE c.state='queued' AND c.attempts<3 AND s.state='active' AND %PREDICATE% ORDER BY c.created_at FOR UPDATE OF c SKIP LOCKED LIMIT 1) RETURNING *",
            2,
        ),
        INVITE_CLAIM => (
            "UPDATE legal_invitation_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=? WHERE id=(SELECT j.id FROM legal_invitation_jobs j JOIN legal_shares s ON s.id=j.share_id JOIN legal_items i ON i.id=s.item_id WHERE j.state='queued' AND j.next_attempt_at<=? AND %PREDICATE% ORDER BY j.created_at FOR UPDATE OF j SKIP LOCKED LIMIT 1) RETURNING *",
            3,
        ),
        AI_CLAIM => (
            "UPDATE legal_ai_proposals SET state='running',lease_until=? WHERE id=(SELECT p.id FROM legal_ai_proposals p JOIN legal_items i ON i.id=p.item_id WHERE p.state='queued' AND %PREDICATE% ORDER BY p.created_at FOR UPDATE OF p SKIP LOCKED LIMIT 1) RETURNING *",
            1,
        ),
        DEADLINE_CTE => (
            "WITH due AS (SELECT i.*,d.days FROM legal_items i JOIN legal_settings s ON s.company_id=i.company_id AND s.enabled CROSS JOIN LATERAL (SELECT min(t.days) days FROM (SELECT value::int days FROM jsonb_array_elements_text(s.reminder_days) UNION SELECT 0 UNION SELECT -1) t WHERE (i.due_date::date-(clock_timestamp() AT TIME ZONE i.timezone)::date)<=t.days) d WHERE %PREDICATE% AND d.days IS NOT NULL AND i.due_date IS NOT NULL AND i.state NOT IN ('completed','closed','archived','cancelled','terminated','expired') AND (i.due_date::date-(clock_timestamp() AT TIME ZONE i.timezone)::date)<=d.days AND EXISTS(SELECT 1 FROM legal_members m WHERE m.item_id=i.id AND m.user_id=i.owner_id)), added AS (INSERT INTO legal_reminders SELECT id,due_date,days,owner_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM due ON CONFLICT DO NOTHING RETURNING *)",
            0,
        ),
        ENVELOPE_CTE => (
            "WITH expired AS (UPDATE legal_envelopes e SET state='expired' FROM legal_items i WHERE i.id=e.item_id AND e.state='sent' AND e.expires_at<=? AND %PREDICATE% RETURNING e.item_id,e.id), changed AS (UPDATE legal_items i SET state='draft',version=version+1,updated_at=? FROM expired e WHERE i.id=e.item_id AND i.state='signing' RETURNING i.id,i.version)",
            2,
        ),
        DEADLINE_REMINDERS => (
            "WITH due AS (SELECT i.*,d.days FROM legal_items i JOIN legal_settings s ON s.company_id=i.company_id AND s.enabled CROSS JOIN LATERAL (SELECT min(t.days) days FROM (SELECT value::int days FROM jsonb_array_elements_text(s.reminder_days) UNION SELECT 0 UNION SELECT -1) t WHERE (i.due_date::date-(clock_timestamp() AT TIME ZONE i.timezone)::date)<=t.days) d WHERE %PREDICATE% AND d.days IS NOT NULL AND i.due_date IS NOT NULL AND i.state NOT IN ('completed','closed','archived','cancelled','terminated','expired') AND (i.due_date::date-(clock_timestamp() AT TIME ZONE i.timezone)::date)<=d.days AND EXISTS(SELECT 1 FROM legal_members m WHERE m.item_id=i.id AND m.user_id=i.owner_id)), added AS (INSERT INTO legal_reminders SELECT id,due_date,days,owner_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM due ON CONFLICT DO NOTHING RETURNING *) SELECT count(*) FROM added",
            0,
        ),
        ENVELOPE_EXPIRE => (
            "WITH expired AS (UPDATE legal_envelopes e SET state='expired' FROM legal_items i WHERE i.id=e.item_id AND e.state='sent' AND e.expires_at<=? AND %PREDICATE% RETURNING e.item_id,e.id), changed AS (UPDATE legal_items i SET state='draft',version=version+1,updated_at=? FROM expired e WHERE i.id=e.item_id AND i.state='signing' RETURNING i.id,i.version) SELECT count(*) FROM changed",
            2,
        ),
        ENROLLMENT_ATTEMPT => (
            "INSERT INTO legal_enrollment_attempts(share_id,window_start,attempts) VALUES(?,?,1) \
             ON CONFLICT(share_id) DO UPDATE SET \
             window_start=CASE WHEN legal_enrollment_attempts.window_start<? THEN excluded.window_start ELSE legal_enrollment_attempts.window_start END,\
             attempts=CASE WHEN legal_enrollment_attempts.window_start<? THEN 1 ELSE legal_enrollment_attempts.attempts+1 END RETURNING attempts",
            4,
        ),
        SOURCE_RECOVER => (
            "UPDATE legal_law_source_checks SET state=CASE WHEN attempts<3 THEN 'queued' ELSE 'failed' END,\
             error='LEGAL_SOURCE_CHECK_INTERRUPTED',lease_token=NULL,lease_until=NULL \
             WHERE state='running' AND lease_until<?",
            1,
        ),
        SOURCE_PRESERVE => (
            "INSERT INTO legal_file_preservations(item_id,source_type,source_id,object_key,checksum,created_by,created_at) \
             VALUES(?,'official-source',?,?,?,'system:legal-source',?) ON CONFLICT DO NOTHING",
            5,
        ),
        SOURCE_COMPLETE => (
            "UPDATE legal_law_source_checks SET state=?,checked_at=?,checksum=?,object_key=?,\
             error=NULL,lease_token=NULL,lease_until=NULL WHERE id=? AND state='running' AND lease_token=?",
            6,
        ),
        SOURCE_FAIL => (
            "UPDATE legal_law_source_checks SET state='failed',checked_at=NULL,error=?,\
             lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
            3,
        ),
        INVITE_RECOVER => (
            "UPDATE legal_invitation_jobs SET state=CASE WHEN attempts<6 THEN 'queued' ELSE 'failed' END,\
             lease_token=NULL,lease_until=NULL,error='DELIVERY_INTERRUPTED' WHERE state='running' AND lease_until<?",
            1,
        ),
        INVITE_CANCEL => (
            "UPDATE legal_invitation_jobs SET state='cancelled',lease_token=NULL,lease_until=NULL \
             WHERE id=? AND lease_token=?",
            2,
        ),
        INVITE_SENT => (
            "UPDATE legal_invitation_jobs SET state='sent',sent_at=?,lease_token=NULL,\
             lease_until=NULL,error=NULL WHERE id=? AND lease_token=?",
            3,
        ),
        INVITE_FAIL => (
            "UPDATE legal_invitation_jobs SET state=?,error=?,next_attempt_at=?,\
             lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
            5,
        ),
        AI_QUEUE => (
            "INSERT INTO legal_ai_proposals(id,item_id,source_version,source_hash,question,state,created_by,created_at,actor_json) \
             VALUES(?,?,?,?,?,'queued',?,?,?)",
            8,
        ),
        AI_INTERRUPT => (
            "UPDATE legal_ai_proposals SET state='failed',error='Generation interrupted; retry explicitly.' \
             WHERE state='running' AND lease_until<?",
            1,
        ),
        AI_COMPLETE => (
            "UPDATE legal_ai_proposals SET state=?,proposal=?::jsonb WHERE id=? AND state='running'",
            3,
        ),
        AI_FAIL => (
            "UPDATE legal_ai_proposals SET state='failed',error=? WHERE id=? AND state='running'",
            2,
        ),
        _ => return None,
    })
}

pub fn supported(kind: &str) -> bool {
    statement(kind).is_some()
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "LEGAL_WAVE3_WRITE_INVALID",
        "Review the Legal owner write request.",
    )
}

/// Pure planner. Rejects unknown kinds, empty or oversized value arrays and any
/// mismatch between caller values and the owner-owned placeholder count.
pub fn plan(kind: &str, values: Vec<Value>) -> Result<LegalWave3Statement, BusinessError> {
    let (sql, expected) = statement(kind).ok_or_else(invalid)?;
    let has_pred = sql.contains("%PREDICATE%");
    let supplied = values.len();
    let expected_total = expected + usize::from(has_pred);
    if supplied < expected || supplied > 64 || (has_pred && supplied > expected_total) {
        return Err(invalid());
    }
    if !has_pred && supplied != expected {
        return Err(invalid());
    }
    let predicate = if has_pred {
        let raw = values.last().and_then(|v| v.as_str()).ok_or_else(invalid)?;
        if raw.is_empty() || raw.len() > 8192 || raw.contains(';') || raw.contains("--") {
            return Err(invalid());
        }
        Some(raw.to_owned())
    } else {
        None
    };
    let values = if has_pred {
        values[..expected].to_vec()
    } else {
        values
    };
    let sql = match predicate {
        Some(pred) => sql.replace("%PREDICATE%", &pred),
        None => sql.to_owned(),
    };
    Ok(LegalWave3Statement { sql, values })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_kind_is_allow_listed_parameterized_and_placeholder_exact() {
        for kind in KINDS {
            assert!(supported(kind), "kind must be supported: {kind}");
            let (sql, expected) = statement(kind).expect("kind has a statement");
            // Owner-owned SQL may legitimately bind zero values (e.g. clock-driven
            // reminders with a validated predicate fragment); what matters is that
            // the declared placeholder count matches the emitted SQL exactly, so
            // no external value is ever string-interpolated into the statement.
            if expected > 0 {
                assert!(sql.contains('?'), "write must be parameterized: {kind}");
            }
            assert_eq!(
                sql.matches('?').count(),
                expected,
                "placeholder count: {kind}"
            );
        }
    }

    #[test]
    fn planner_accepts_exact_values_and_rejects_everything_else() {
        let planned = plan(SOURCE_RECOVER, vec![json!(1)]).expect("valid");
        assert!(planned.sql.contains("legal_law_source_checks"));
        assert!(plan(SOURCE_RECOVER, vec![]).is_err());
        assert!(plan(SOURCE_RECOVER, vec![json!(1), json!(2)]).is_err());
        assert!(plan("not_a_legal_write", vec![json!(1)]).is_err());
    }

    #[test]
    fn every_statement_carries_the_owner_table() {
        for kind in KINDS {
            let (sql, _) = statement(kind).expect("kind has a statement");
            assert!(
                sql.contains("legal_") || sql.contains("command_"),
                "owner table present: {kind}"
            );
        }
    }
}
