//! Native-owned Collaboration write statements (wf1a7 retirement slice).
//!
//! This is the authoritative owner model for every TypeScript write literal in
//! `packages/server-collaboration/src/**/*.ts` except the blocked
//! `social-home.ts`. The SQL and its argument order live here, in the
//! Collaboration owner crate; the axum router in [`wf1a7_http`] transports a
//! typed `kind`, full-parity values and bounded SQL fragment selectors. The
//! caller executes the returned statement inside its own serializable unit
//! together with receipt/audit/outbox effects — no second transaction, no TS
//! SQL copy and no fallback writer.
use crate::adapters::notification_statement::{
    NotificationStatement, NotificationStatementInput, values_from_json,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

// --- Facts / attachment owner version guards ---------------------------------
pub const SOCIAL_POST_VERSION_STEP: &str = "social_post_version_step";
pub const ATTACHMENT_SERVICE_VERSION_STEP: &str = "attachment_service_version_step";
pub const HANDOVER_ACK_INSERT: &str = "handover_ack_insert";
pub const MESSAGING_FILE_BIND: &str = "messaging_file_bind";

// --- Social AI ---------------------------------------------------------------
pub const AI_DRAFT_PREPARE: &str = "ai_draft_prepare";
pub const AI_USAGE_RESERVE: &str = "ai_usage_reserve";
pub const AI_DRAFT_CAPTURE_PROVIDER: &str = "ai_draft_capture_provider";
pub const AI_DRAFT_CAPTURE_ACTOR: &str = "ai_draft_capture_actor";
pub const AI_DRAFT_FINISH: &str = "ai_draft_finish";
pub const AI_USAGE_FINISH: &str = "ai_usage_finish";
pub const AI_DRAFT_FAIL: &str = "ai_draft_fail";
pub const AI_USAGE_FAIL: &str = "ai_usage_fail";
pub const AI_DRAFT_ACTOR_BACKFILL: &str = "ai_draft_actor_backfill";

// --- Social agent ------------------------------------------------------------
pub const AGENT_RETRY_PREPARE: &str = "agent_retry_prepare";
pub const AGENT_LEASE_EXPIRE: &str = "agent_lease_expire";
pub const AGENT_REPLY_CLAIM: &str = "agent_reply_claim";
pub const AGENT_REPLY_DRAFT: &str = "agent_reply_draft";
pub const AGENT_USAGE_RESERVE: &str = "agent_usage_reserve";
pub const AGENT_REPLY_COMMENT: &str = "agent_reply_comment";
pub const AGENT_REPLY_FINISH: &str = "agent_reply_finish";
pub const AGENT_DRAFT_FINISH: &str = "agent_draft_finish";
pub const AGENT_USAGE_FINISH: &str = "agent_usage_finish";
pub const AGENT_REPLY_FAIL: &str = "agent_reply_fail";
pub const AGENT_DRAFT_FAIL: &str = "agent_draft_fail";
pub const AGENT_USAGE_FAIL: &str = "agent_usage_fail";
pub const AGENT_UNCLAIMABLE_FAIL: &str = "agent_unclaimable_fail";

// --- Messaging ---------------------------------------------------------------
pub const MSG_ROOM_INSERT: &str = "msg_room_insert";
pub const MSG_TEAM_INSERT: &str = "msg_team_insert";
pub const MSG_GENERAL_INSERT: &str = "msg_general_insert";
pub const MSG_CHANNEL_INSERT: &str = "msg_channel_insert";
pub const MSG_RENAME: &str = "msg_rename";
pub const MSG_REMOVE_MEMBERS: &str = "msg_remove_members";
pub const MSG_REMOVE_CHANNEL_MEMBERS: &str = "msg_remove_channel_members";
pub const MSG_MESSAGE_INSERT: &str = "msg_message_insert";
pub const MSG_FILE_BIND_MESSAGE: &str = "msg_file_bind_message";
pub const MSG_READ_UPSERT: &str = "msg_read_upsert";
pub const MSG_MEMBER_BIND: &str = "msg_member_bind";
pub const MSG_ADVANCE_ROOM: &str = "msg_advance_room";
pub const MSG_ADVANCE_ROOM_VERSIONED: &str = "msg_advance_room_versioned";

// --- Web push ---------------------------------------------------------------
pub const PUSH_DISABLE_OWNER: &str = "push_disable_owner";
pub const PUSH_ENABLE_PREFERENCES: &str = "push_enable_preferences";
pub const PUSH_SUBSCRIPTION_STORE: &str = "push_subscription_store";
pub const PUSH_PREFERENCES_SAVE: &str = "push_preferences_save";
pub const PUSH_ENDPOINT_REMOVE: &str = "push_endpoint_remove";
pub const PUSH_SESSION_REMOVE: &str = "push_session_remove";
pub const PUSH_SUBSCRIPTION_REMOVE: &str = "push_subscription_remove";
pub const PUSH_JOBS_REMOVE: &str = "push_jobs_remove";
pub const PUSH_CURSOR_ADVANCE: &str = "push_cursor_advance";
pub const PUSH_DELIVERY_QUEUE: &str = "push_delivery_queue";
pub const PUSH_DELIVERY_LEASE: &str = "push_delivery_lease";
pub const PUSH_DELIVERY_DEAD: &str = "push_delivery_dead";
pub const PUSH_DELIVERY_DEFER: &str = "push_delivery_defer";
pub const PUSH_DELIVERY_COMPLETE: &str = "push_delivery_complete";
pub const PUSH_DELIVERY_RETRY: &str = "push_delivery_retry";
pub const PUSH_SUBSCRIPTION_REVOKE: &str = "push_subscription_revoke";

// --- Social home (retired from social-home.ts) -------------------------------
pub const SH_DRAFT_UPDATE: &str = "social_home_draft_update";
pub const SH_DRAFT_PREPARE: &str = "social_home_draft_prepare";
pub const SH_POST_PREPARE: &str = "social_home_post_prepare";
pub const SH_PARTICIPANT_PREPARE: &str = "social_home_participant_prepare";
pub const SH_SOURCE_PREPARE: &str = "social_home_source_prepare";
pub const SH_DRAFT_BIND_PUBLISHED: &str = "social_home_draft_bind_published";
pub const SH_FOLLOW_PREPARE: &str = "social_home_follow_prepare";
pub const SH_COMMENT_MENTIONS_CLEAR: &str = "social_home_comment_mentions_clear";
pub const SH_COMMENT_TOPICS_CLEAR: &str = "social_home_comment_topics_clear";
pub const SH_COMMENT_UPDATE: &str = "social_home_comment_update";
pub const SH_COMMENT_MENTION_PREPARE: &str = "social_home_comment_mention_prepare";
pub const SH_COMMENT_REMOVE: &str = "social_home_comment_remove";
pub const SH_REVISION_PREPARE: &str = "social_home_revision_prepare";
pub const SH_POST_RETRACT: &str = "social_home_post_retract";
pub const SH_POST_MENTIONS_CLEAR: &str = "social_home_post_mentions_clear";
pub const SH_POST_TOPICS_CLEAR: &str = "social_home_post_topics_clear";
pub const SH_POST_UPDATE: &str = "social_home_post_update";
pub const SH_EDIT_DRAFT_BIND: &str = "social_home_edit_draft_bind";
pub const SH_POST_MENTION_PREPARE: &str = "social_home_post_mention_prepare";
pub const SH_TOPIC_PREPARE: &str = "social_home_topic_prepare";
pub const SH_POST_TOPIC_PREPARE: &str = "social_home_post_topic_prepare";
pub const SH_EDIT_NOTIFICATION: &str = "social_home_edit_notification";
pub const SH_COMMENT_PREPARE: &str = "social_home_comment_prepare";
pub const SH_COMMENT_DRAFT_BIND: &str = "social_home_comment_draft_bind";
pub const SH_REPLY_NOTIFICATION: &str = "social_home_reply_notification";
pub const SH_REACTION_PREPARE: &str = "social_home_reaction_prepare";
pub const SH_REACTION_REMOVE: &str = "social_home_reaction_remove";
pub const SH_FOLLOW_REMOVE: &str = "social_home_follow_remove";
pub const SH_REPORT_PREPARE: &str = "social_home_report_prepare";
pub const SH_POST_MODERATE: &str = "social_home_post_moderate";

/// All kinds accepted by this owner module. Anything else is fail-closed.
const KINDS: &[&str] = &[
    SOCIAL_POST_VERSION_STEP, ATTACHMENT_SERVICE_VERSION_STEP, HANDOVER_ACK_INSERT,
    MESSAGING_FILE_BIND, AI_DRAFT_PREPARE, AI_USAGE_RESERVE, AI_DRAFT_CAPTURE_PROVIDER,
    AI_DRAFT_CAPTURE_ACTOR, AI_DRAFT_FINISH, AI_USAGE_FINISH, AI_DRAFT_FAIL, AI_USAGE_FAIL,
    AI_DRAFT_ACTOR_BACKFILL, AGENT_RETRY_PREPARE, AGENT_LEASE_EXPIRE, AGENT_REPLY_CLAIM,
    AGENT_REPLY_DRAFT, AGENT_USAGE_RESERVE, AGENT_REPLY_COMMENT, AGENT_REPLY_FINISH,
    AGENT_DRAFT_FINISH, AGENT_USAGE_FINISH, AGENT_REPLY_FAIL, AGENT_DRAFT_FAIL,
    AGENT_USAGE_FAIL, AGENT_UNCLAIMABLE_FAIL, MSG_ROOM_INSERT, MSG_TEAM_INSERT,
    MSG_GENERAL_INSERT, MSG_CHANNEL_INSERT, MSG_RENAME, MSG_REMOVE_MEMBERS,
    MSG_REMOVE_CHANNEL_MEMBERS, MSG_MESSAGE_INSERT, MSG_FILE_BIND_MESSAGE, MSG_READ_UPSERT,
    MSG_MEMBER_BIND, MSG_ADVANCE_ROOM_VERSIONED, MSG_ADVANCE_ROOM, PUSH_DISABLE_OWNER, PUSH_ENABLE_PREFERENCES, PUSH_SUBSCRIPTION_STORE,
    PUSH_PREFERENCES_SAVE, PUSH_ENDPOINT_REMOVE, PUSH_SESSION_REMOVE, PUSH_SUBSCRIPTION_REMOVE,
    PUSH_JOBS_REMOVE, PUSH_CURSOR_ADVANCE, PUSH_DELIVERY_QUEUE, PUSH_DELIVERY_LEASE,
    PUSH_DELIVERY_DEAD,
    PUSH_DELIVERY_DEFER, PUSH_DELIVERY_COMPLETE, PUSH_DELIVERY_RETRY,
    PUSH_SUBSCRIPTION_REVOKE,
    SH_DRAFT_UPDATE, SH_DRAFT_PREPARE, SH_POST_PREPARE, SH_PARTICIPANT_PREPARE,
    SH_SOURCE_PREPARE, SH_DRAFT_BIND_PUBLISHED, SH_FOLLOW_PREPARE,
    SH_COMMENT_MENTIONS_CLEAR, SH_COMMENT_TOPICS_CLEAR, SH_COMMENT_UPDATE,
    SH_COMMENT_MENTION_PREPARE, SH_COMMENT_REMOVE, SH_REVISION_PREPARE,
    SH_POST_RETRACT, SH_POST_MENTIONS_CLEAR, SH_POST_TOPICS_CLEAR, SH_POST_UPDATE,
    SH_EDIT_DRAFT_BIND, SH_POST_MENTION_PREPARE, SH_TOPIC_PREPARE,
    SH_POST_TOPIC_PREPARE, SH_EDIT_NOTIFICATION, SH_COMMENT_PREPARE,
    SH_COMMENT_DRAFT_BIND, SH_REPLY_NOTIFICATION, SH_REACTION_PREPARE,
    SH_REACTION_REMOVE, SH_FOLLOW_REMOVE, SH_REPORT_PREPARE, SH_POST_MODERATE,
];

/// Kinds allowed on the private system route (no browser WorkContext). These
/// statements carry their own strict row/version/lease guards and need no
/// caller-supplied Company authority; every other kind requires the browser
/// Holding route with exact host/origin and current context headers.
const SYSTEM_KINDS: &[&str] = &[
    SOCIAL_POST_VERSION_STEP,
    ATTACHMENT_SERVICE_VERSION_STEP,
    HANDOVER_ACK_INSERT,
    AGENT_UNCLAIMABLE_FAIL,
    // Job/queue writes: every statement is row+lease guarded and is driven by
    // the private worker, never by a browser WorkContext.
    AGENT_LEASE_EXPIRE,
    AGENT_REPLY_CLAIM,
    AGENT_RETRY_PREPARE,
    AGENT_REPLY_DRAFT,
    AGENT_USAGE_RESERVE,
    AGENT_REPLY_COMMENT,
    AGENT_REPLY_FINISH,
    AGENT_DRAFT_FINISH,
    AGENT_USAGE_FINISH,
    AGENT_REPLY_FAIL,
    AGENT_DRAFT_FAIL,
    AGENT_USAGE_FAIL,
    PUSH_JOBS_REMOVE,
    PUSH_SUBSCRIPTION_REMOVE,
    PUSH_ENDPOINT_REMOVE,
    PUSH_SESSION_REMOVE,
    PUSH_DISABLE_OWNER,
    PUSH_SUBSCRIPTION_REVOKE,
    PUSH_CURSOR_ADVANCE,
    PUSH_DELIVERY_LEASE,
    PUSH_DELIVERY_DEAD,
    PUSH_DELIVERY_DEFER,
    PUSH_DELIVERY_COMPLETE,
    PUSH_DELIVERY_RETRY,
    SH_DRAFT_UPDATE, SH_DRAFT_PREPARE, SH_POST_PREPARE, SH_PARTICIPANT_PREPARE,
    SH_SOURCE_PREPARE, SH_DRAFT_BIND_PUBLISHED, SH_FOLLOW_PREPARE,
    SH_COMMENT_MENTIONS_CLEAR, SH_COMMENT_TOPICS_CLEAR, SH_COMMENT_UPDATE,
    SH_COMMENT_MENTION_PREPARE, SH_COMMENT_REMOVE, SH_REVISION_PREPARE,
    SH_POST_RETRACT, SH_POST_MENTIONS_CLEAR, SH_POST_TOPICS_CLEAR, SH_POST_UPDATE,
    SH_EDIT_DRAFT_BIND, SH_POST_MENTION_PREPARE, SH_TOPIC_PREPARE,
    SH_POST_TOPIC_PREPARE, SH_EDIT_NOTIFICATION, SH_COMMENT_PREPARE,
    SH_COMMENT_DRAFT_BIND, SH_REPLY_NOTIFICATION, SH_REACTION_PREPARE,
    SH_REACTION_REMOVE, SH_FOLLOW_REMOVE, SH_REPORT_PREPARE, SH_POST_MODERATE,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}
pub fn system_supported(kind: &str) -> bool {
    SYSTEM_KINDS.contains(&kind)
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "COLLABORATION_OWNER_WRITE_INVALID",
        "Review the collaboration write request.",
    )
}

async fn ready(connection: &mut PgConnection, company: &CompanyId) -> Result<(), Error> {
    crate::adapters::notification_statement::ready(connection, company).await
}

/// A typed `target` selector maps to a fixed owner table/column — never to a
/// caller-provided identifier.
fn target_clause(kind: &str, target: Option<&str>) -> Option<&'static str> {
    match (kind, target) {
        (MSG_RENAME, Some("team")) => Some("UPDATE messaging_teams SET name=?,version=version+1 WHERE id=?"),
        (MSG_RENAME, Some("room")) => Some("UPDATE messaging_rooms SET name=?,version=version+1 WHERE id=?"),
        (MSG_MEMBER_BIND, Some("team")) => Some("INSERT INTO messaging_team_members(team_id,user_id) VALUES(?,?) ON CONFLICT DO NOTHING"),
        (MSG_MEMBER_BIND, Some("room")) => Some("INSERT INTO messaging_room_members(room_id,user_id) VALUES(?,?) ON CONFLICT DO NOTHING"),
        (MSG_REMOVE_MEMBERS, Some("team")) => Some("DELETE FROM messaging_team_members WHERE team_id=? AND user_id NOT IN"),
        (MSG_REMOVE_MEMBERS, Some("room")) => Some("DELETE FROM messaging_room_members WHERE room_id=? AND user_id NOT IN"),
        (ATTACHMENT_SERVICE_VERSION_STEP, Some("social_draft")) => Some("UPDATE social_post_drafts SET version=version+1,updated_at=? WHERE id=? AND version=?"),
        _ => None,
    }
}

/// Validate a supplied SQL fragment. Fragments come from owner TS policies
/// (feature rollout / source access) and are embedded in a fixed position; they
/// are bounded and may not contain statement terminators or comments.
fn valid_fragment(value: Option<&str>, required: bool) -> Result<Option<String>, Error> {
    match value {
        None if required => Err(invalid().into()),
        None => Ok(None),
        Some(raw) => {
            if raw.is_empty() || raw.len() > 4_000 || raw.contains(';')
                || raw.contains("--") || raw.contains("/*") || raw.contains("*/")
            {
                return Err(invalid().into());
            }
            Ok(Some(raw.to_owned()))
        }
    }
}

/// Return the authoritative static SQL for an owner write kind.
fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        SOCIAL_POST_VERSION_STEP => "UPDATE social_posts SET version=version+1,updated_at=? WHERE id=? AND version=? AND state='published'",
        HANDOVER_ACK_INSERT => "INSERT INTO social_handover_acknowledgements(post_id,user_id,post_version,acknowledged_at,command_id) VALUES(?,?,?,?,?)",
        MESSAGING_FILE_BIND => "INSERT INTO messaging_files(id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at,attachment_set_id,attachment_link_id,attachment_source_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
        AI_DRAFT_PREPARE => "INSERT INTO social_ai_drafts(id,post_id,actor_id,kind,source_version,source_hash,comment_ids_json,sources_json,state,model,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?,'pending',?,?,?)",
        AI_USAGE_RESERVE | AGENT_USAGE_RESERVE => "INSERT INTO social_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) VALUES(?,?,?,?,'reserved',?,?)",
        AI_DRAFT_CAPTURE_PROVIDER => "UPDATE social_ai_drafts SET provider_json=? WHERE id=?",
        AI_DRAFT_CAPTURE_ACTOR => "UPDATE social_ai_drafts SET actor_json=? WHERE id=?",
        AI_DRAFT_FINISH => "UPDATE social_ai_drafts SET result_json=?,state='ready' WHERE id=?",
        AI_USAGE_FINISH | AGENT_USAGE_FINISH => "UPDATE social_ai_usage SET state='succeeded',input_tokens=?,output_tokens=?,lease_until=0 WHERE id=?",
        AI_DRAFT_FAIL => "UPDATE social_ai_drafts SET state='failed',result_json=NULL WHERE id=?",
        AI_USAGE_FAIL | AGENT_USAGE_FAIL => "UPDATE social_ai_usage SET state='failed',lease_until=0 WHERE id=?",
        AI_DRAFT_ACTOR_BACKFILL => "UPDATE social_ai_drafts SET actor_json=? WHERE id=? AND actor_id=? AND post_id=?",
        AGENT_RETRY_PREPARE => "UPDATE social_agent_replies SET state='pending',error=NULL,error_code=NULL,actor_json=?,updated_at=? WHERE id=?",
        AGENT_LEASE_EXPIRE => "UPDATE social_agent_replies r SET state='failed',error='The assistant request timed out. Try again.',error_code='AI_TIMEOUT',updated_at=? WHERE state='running' AND lease_until<?",
        AGENT_REPLY_CLAIM => "UPDATE social_agent_replies SET state='running',attempt=attempt+1,lease_token=?,lease_until=?,updated_at=? WHERE id=(SELECT id FROM social_agent_replies pending WHERE pending.state='pending' AND (?::text IS NULL OR pending.post_id=?) ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *",
        AGENT_REPLY_DRAFT => "INSERT INTO social_ai_drafts(id,post_id,actor_id,kind,source_version,source_hash,comment_ids_json,sources_json,state,model,created_at,expires_at,provider_json) VALUES(?,?,?,'reply',?,?,?,?,'pending',?,?,?,?)",
        AGENT_REPLY_COMMENT => "INSERT INTO social_comments(id,post_id,author_id,author_name,body,rich_content_json,author_kind,last_command_id,created_at,updated_at,parent_comment_id) VALUES(?,?,?,?,?,?,'agent',?,?,?,?)",
        AGENT_REPLY_FINISH => "UPDATE social_agent_replies SET state='completed',error=NULL,error_code=NULL,lease_until=0,updated_at=? WHERE id=?",
        AGENT_DRAFT_FINISH => "UPDATE social_ai_drafts SET state='ready',result_json=? WHERE id=?",
        AGENT_REPLY_FAIL => "UPDATE social_agent_replies SET state='failed',error=?,error_code=?,lease_until=0,updated_at=? WHERE id=? AND state IN ('running','failed') AND lease_token=? AND actor_json=? AND owner_type=? AND owner_id=?",
        AGENT_DRAFT_FAIL => "UPDATE social_ai_drafts SET state='failed' WHERE id=?",
        AGENT_UNCLAIMABLE_FAIL => "UPDATE social_agent_replies SET state='failed',error='The conversation or original access is no longer available. Review access, then retry explicitly.',error_code='ACCESS_CHANGED',lease_until=0,updated_at=? WHERE id=? AND company_id=? AND actor_id=? AND post_id=? AND state='pending' AND actor_json=? AND attempt=? AND lease_token IS NOT DISTINCT FROM ?",
        MSG_ROOM_INSERT => "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,direct_key,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING",
        MSG_TEAM_INSERT => "INSERT INTO messaging_teams(id,company_id,name,owner_id,created_at) VALUES(?,?,?,?,?)",
        MSG_GENERAL_INSERT => "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,created_at) VALUES(?,?,'channel','General',?,?,?)",
        MSG_CHANNEL_INSERT => "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,private,created_at) VALUES(?,?,'channel',?,?,?,?,?)",
        MSG_REMOVE_CHANNEL_MEMBERS => "DELETE FROM messaging_room_members WHERE room_id IN (SELECT id FROM messaging_rooms WHERE team_id=?) AND user_id NOT IN",
        MSG_ADVANCE_ROOM_VERSIONED => "UPDATE messaging_rooms SET last_seq=last_seq+1,version=version+1 WHERE id=?",
        MSG_ADVANCE_ROOM => "UPDATE messaging_rooms SET last_seq=last_seq+1 WHERE id=?",
        MSG_MESSAGE_INSERT => "INSERT INTO messaging_messages(id,room_id,seq,sender_id,sender_name,body,created_at) SELECT ?,id,last_seq,?,?,?,? FROM messaging_rooms WHERE id=?",
        MSG_FILE_BIND_MESSAGE => "UPDATE messaging_files SET message_id=? WHERE id=?",
        MSG_READ_UPSERT => "INSERT INTO messaging_reads(room_id,user_id,seq) SELECT r.id,?,LEAST(?,r.last_seq) FROM messaging_rooms r WHERE r.id=? AND r.company_id=? ON CONFLICT(room_id,user_id) DO UPDATE SET seq=GREATEST(messaging_reads.seq,excluded.seq)",
        PUSH_DISABLE_OWNER => "UPDATE push_preferences SET enabled=false,updated_at=? WHERE company_id=? AND user_id=? AND NOT EXISTS(SELECT 1 FROM push_subscriptions WHERE company_id=? AND user_id=?)",
        PUSH_ENABLE_PREFERENCES => "INSERT INTO push_preferences(company_id,user_id,enabled,updated_at) VALUES(?,?,true,?) ON CONFLICT(company_id,user_id) DO UPDATE SET enabled=true,updated_at=excluded.updated_at",
        PUSH_SUBSCRIPTION_STORE => "INSERT INTO push_subscriptions(id,company_id,user_id,session_id,origin,inbox_path,endpoint_hash,sealed_subscription,created_at,updated_at,scan_at,cursor_at,cursor_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,endpoint_hash) DO UPDATE SET user_id=excluded.user_id,session_id=excluded.session_id,origin=excluded.origin,inbox_path=excluded.inbox_path,sealed_subscription=excluded.sealed_subscription,updated_at=excluded.updated_at,scan_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.scan_at ELSE excluded.scan_at END,cursor_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.cursor_at ELSE excluded.cursor_at END,cursor_id=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.cursor_id ELSE '' END,created_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.created_at ELSE excluded.created_at END",
        PUSH_PREFERENCES_SAVE => "INSERT INTO push_preferences(company_id,user_id,enabled,work_enabled,mention_enabled,system_enabled,calendar_enabled,notice_enabled,quiet_start_minute,quiet_end_minute,updated_at) VALUES(?,?,false,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET work_enabled=excluded.work_enabled,mention_enabled=excluded.mention_enabled,system_enabled=excluded.system_enabled,calendar_enabled=excluded.calendar_enabled,notice_enabled=excluded.notice_enabled,quiet_start_minute=excluded.quiet_start_minute,quiet_end_minute=excluded.quiet_end_minute,updated_at=excluded.updated_at",
        PUSH_ENDPOINT_REMOVE => "DELETE FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=? AND endpoint_hash=?",
        PUSH_SESSION_REMOVE => "DELETE FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=?",
        PUSH_SUBSCRIPTION_REMOVE => "DELETE FROM push_subscriptions WHERE id=?",
        PUSH_JOBS_REMOVE => "DELETE FROM push_delivery_jobs WHERE subscription_id=?",
        PUSH_CURSOR_ADVANCE => "UPDATE push_subscriptions SET scan_at=?,cursor_at=?,cursor_id=? WHERE id=? AND cursor_at=? AND cursor_id=?",
        PUSH_DELIVERY_QUEUE => "INSERT INTO push_delivery_jobs(id,company_id,inbox_delivery_id,subscription_id,next_attempt_at,created_at) SELECT ?,?,?,?,?,? WHERE EXISTS(SELECT 1 FROM inbox_deliveries WHERE id=? AND company_id=? AND user_id=? AND read_at IS NULL AND archived_at IS NULL) ON CONFLICT(inbox_delivery_id,subscription_id) DO NOTHING",
        PUSH_DELIVERY_LEASE => "UPDATE push_delivery_jobs SET lease_token=?,lease_until=?,attempts=attempts+1 WHERE id=(SELECT j.id FROM push_delivery_jobs j WHERE j.status='pending' AND j.next_attempt_at<=? AND (j.lease_until IS NULL OR j.lease_until<?) AND NOT erpx_field_push_job(j) ORDER BY j.next_attempt_at,j.id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *",
        PUSH_DELIVERY_DEAD => "UPDATE push_delivery_jobs SET status='dead',lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
        PUSH_DELIVERY_DEFER => "UPDATE push_delivery_jobs SET next_attempt_at=?,lease_token=NULL,lease_until=NULL,attempts=attempts-1 WHERE id=? AND lease_token=?",
        PUSH_DELIVERY_COMPLETE => "UPDATE push_delivery_jobs SET status='sent',sent_at=?,last_status=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
        PUSH_DELIVERY_RETRY => "UPDATE push_delivery_jobs SET status=?,next_attempt_at=?,last_status=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
        PUSH_SUBSCRIPTION_REVOKE => "DELETE FROM push_subscriptions WHERE user_id=? AND session_id=? RETURNING company_id",
        SH_DRAFT_UPDATE => "UPDATE social_post_drafts SET body=?,branch_id=?,department_id=?,audience_kind=?,audience_id=?,participants_json=?,sources_json=?,purpose=?,mentions_json=?,content_json=?,rich_content_json=?,version=version+1,updated_at=? WHERE id=?",
        SH_DRAFT_PREPARE => "INSERT INTO social_post_drafts(id,author_id,company_id,branch_id,body,created_at,updated_at,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,mentions_json,content_json,rich_content_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        SH_POST_PREPARE => "INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,body,last_command_id,created_at,updated_at,department_id,audience_kind,audience_id,purpose,mentions_json,content_json,rich_content_json) VALUES(?,'team',?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        SH_PARTICIPANT_PREPARE => "INSERT INTO social_post_participants(post_id,user_id) VALUES(?,?)",
        SH_SOURCE_PREPARE => "INSERT INTO social_post_sources(post_id,ordinal,source_kind,source_id) VALUES(?,?,?,?)",
        SH_DRAFT_BIND_PUBLISHED => "UPDATE social_post_drafts SET published_post_id=?,version=version+1,updated_at=? WHERE id=?",
        SH_FOLLOW_PREPARE => "INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES(?,'post',?,?) ON CONFLICT DO NOTHING",
        SH_COMMENT_MENTIONS_CLEAR => "DELETE FROM social_mentions WHERE owner_type='comment' AND owner_id=?",
        SH_COMMENT_TOPICS_CLEAR => "DELETE FROM social_comment_topics WHERE comment_id=?",
        SH_COMMENT_UPDATE => "UPDATE social_comments SET body=?,rich_content_json=?,mentions_json=?,version=version+1,updated_at=?,last_command_id=? WHERE id=? AND post_id=? AND author_id=? AND state='published' AND version=?",
        SH_COMMENT_MENTION_PREPARE => "INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES('comment',?,?,?,?,?,?)",
        SH_COMMENT_REMOVE => "UPDATE social_comments SET state='tombstoned',version=version+1,updated_at=?,last_command_id=? WHERE id=? AND post_id=? AND state='published' AND version=?",
        SH_REVISION_PREPARE => "INSERT INTO social_post_revisions(id,post_id,prior_version,action,body,content_json,rich_content_json,mentions_json,actor_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
        SH_POST_RETRACT => "UPDATE social_posts SET state='archived',retracted_at=?,version=version+1,updated_at=? WHERE id=? AND author_id=? AND version=?",
        SH_POST_MENTIONS_CLEAR => "DELETE FROM social_mentions WHERE owner_type='post' AND owner_id=?",
        SH_POST_TOPICS_CLEAR => "DELETE FROM social_post_topics WHERE post_id=?",
        SH_POST_UPDATE => "UPDATE social_posts SET body=?,content_json=?,rich_content_json=?,mentions_json=?,edited_at=?,version=version+1,updated_at=? WHERE id=? AND author_id=? AND version=?",
        SH_EDIT_DRAFT_BIND => "UPDATE social_post_drafts SET edit_saved_version=?,version=version+1,updated_at=? WHERE id=? AND edit_saved_version IS NULL AND cancelled_at IS NULL",
        SH_POST_MENTION_PREPARE => "INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES('post',?,?,?,?,?,?)",
        SH_TOPIC_PREPARE => "INSERT INTO social_topics(tag,created_by,created_at) VALUES(?,?,?) ON CONFLICT DO NOTHING",
        SH_POST_TOPIC_PREPARE => "INSERT INTO social_post_topics(post_id,tag) VALUES(?,?) ON CONFLICT DO NOTHING",
        SH_EDIT_NOTIFICATION => "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,p.company_id,?,'post',p.id,p.version,'You were mentioned in an edited conversation',? FROM social_posts p WHERE p.id=?",
        SH_COMMENT_PREPARE => "INSERT INTO social_comments(id,post_id,author_id,author_name,body,last_command_id,created_at,updated_at,mentions_json,parent_comment_id,rich_content_json,comment_upload_draft_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
        SH_COMMENT_DRAFT_BIND => "UPDATE social_post_drafts SET published_comment_id=?,version=version+1,updated_at=? WHERE id=? AND published_comment_id IS NULL",
        SH_REPLY_NOTIFICATION => "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,p.company_id,?,'post',p.id,p.version,'Someone replied to your comment',? FROM social_posts p WHERE p.id=?",
        SH_REACTION_PREPARE => "INSERT INTO social_reactions(post_id,principal_id,kind,created_at) VALUES(?,?,'appreciate',?) ON CONFLICT DO NOTHING",
        SH_REACTION_REMOVE => "DELETE FROM social_reactions WHERE post_id=? AND principal_id=?",
        SH_FOLLOW_REMOVE => "DELETE FROM social_follows WHERE principal_id=? AND target_type='post' AND target_id=?",
        SH_REPORT_PREPARE => "INSERT INTO social_reports(id,post_id,reporter_id,reason,created_at) VALUES(?,?,?,?,?) ON CONFLICT(post_id,reporter_id) DO UPDATE SET reason=excluded.reason",
        SH_POST_MODERATE => "UPDATE social_posts SET state='moderated',version=version+1,updated_at=? WHERE id=?",
        _ => return None,
    })
}

/// The full owner statement, including typed fragment composition. The caller
/// can never supply SQL text directly: fragments are validated and inserted in
/// fixed positions of owner-owned templates.
pub fn statement_for(
    kind: &str,
    values_count: usize,
    target: Option<&str>,
    predicate: Option<&str>,
) -> Option<String> {
    match kind {
        // Kinds whose table/column is a typed selector.
        MSG_RENAME | MSG_MEMBER_BIND | ATTACHMENT_SERVICE_VERSION_STEP => {
            target_clause(kind, target).map(str::to_owned)
        }
        // Kinds with a variable `NOT IN (...)` member list.
        MSG_REMOVE_MEMBERS | MSG_REMOVE_CHANNEL_MEMBERS => {
            let base = if kind == MSG_REMOVE_MEMBERS {
                target_clause(kind, target)?
            } else {
                sql(kind)?
            };
            if values_count < 1 {
                return None;
            }
            let placeholders = std::iter::repeat("?")
                .take(values_count - 1)
                .collect::<Vec<_>>()
                .join(",");
            Some(base.replacen("NOT IN", &format!("NOT IN ({placeholders})"), 1))
        }
        // Dynamic owner policy predicates embedded at a fixed point.
        AGENT_LEASE_EXPIRE => {
            let p = predicate?;
            Some(format!("{} AND ({p})", sql(kind)?))
        }
        AGENT_REPLY_CLAIM => {
            let p = predicate?;
            let base = sql(kind)?;
            Some(base.replacen(
                "pending.post_id=?)",
                &format!("pending.post_id=?) AND ({p})"),
                1,
            ))
        }
        MSG_READ_UPSERT => {
            let p = predicate?;
            let base = sql(kind)?;
            Some(base.replacen(
                "r.company_id=? ON CONFLICT",
                &format!("r.company_id=? AND ({p}) ON CONFLICT"),
                1,
            ))
        }
        SH_EDIT_NOTIFICATION | SH_REPLY_NOTIFICATION => {
            let p = predicate?;
            Some(format!("{} AND ({p})", sql(kind)?))
        }
        _ => sql(kind).map(str::to_owned),
    }
}

/// Build the authoritative write statement on the caller's serializable unit.
pub async fn wf1a7_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
    predicate: Option<&str>,
    target: Option<&str>,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let kind = input.kind.as_deref().ok_or_else(invalid)?;
    if !supported(kind) {
        return Err(invalid().into());
    }
    let values = values_from_json(input)?;
    let needs_predicate = matches!(
        kind,
        AGENT_LEASE_EXPIRE | AGENT_REPLY_CLAIM | MSG_READ_UPSERT
        | SH_EDIT_NOTIFICATION | SH_REPLY_NOTIFICATION
    );
    let _ = valid_fragment(predicate, needs_predicate)?;
    let statement = statement_for(kind, values.len(), target, predicate).ok_or_else(invalid)?;
    if values.len() != statement.matches('?').count() {
        return Err(invalid().into());
    }
    Ok(NotificationStatement {
        sql: statement,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_allow_listed_with_owner_sql() {
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            let target = if *kind == ATTACHMENT_SERVICE_VERSION_STEP {
                Some("social_draft")
            } else {
                Some("room")
            };
            let stmt = statement_for(kind, 1, target, Some("TRUE"))
                .unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.contains('?'), "write must be parameterized: {kind}");
        }
    }

    #[test]
    fn system_route_is_restricted() {
        for kind in SYSTEM_KINDS {
            assert!(supported(kind));
        }
        assert!(!system_supported(MSG_ROOM_INSERT));
        assert!(system_supported(SOCIAL_POST_VERSION_STEP));
    }

    #[test]
    fn typed_targets_map_to_fixed_tables() {
        assert_eq!(
            statement_for(MSG_RENAME, 2, Some("team"), None).unwrap(),
            "UPDATE messaging_teams SET name=?,version=version+1 WHERE id=?"
        );
        assert!(statement_for(MSG_RENAME, 2, Some("evil"), None).is_none());
        assert_eq!(
            statement_for(MSG_REMOVE_MEMBERS, 3, Some("room"), None).unwrap(),
            "DELETE FROM messaging_room_members WHERE room_id=? AND user_id NOT IN (?,?)"
        );
    }

    #[test]
    fn predicates_need_owner_policy() {
        assert!(statement_for(AGENT_REPLY_CLAIM, 5, None, None).is_none());
        assert!(statement_for(AGENT_REPLY_CLAIM, 5, None, Some("TRUE")).is_some());
        assert!(statement_for(MSG_READ_UPSERT, 6, None, Some("TRUE")).is_some());
    }

    #[test]
    fn fragments_cannot_smuggle_statements() {
        assert!(valid_fragment(Some("TRUE); DROP TABLE x"), false).is_err());
        assert!(valid_fragment(Some("TRUE -- comment"), false).is_err());
        assert!(valid_fragment(Some("TRUE"), false).is_ok());
    }
}
