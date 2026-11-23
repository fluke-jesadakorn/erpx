//! Native-owned Files wave-1b write kinds and exact owner SQL.
//! One allow-listed kind maps to one authoritative SQL statement executed in Rust.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const SR_001: &str = "SR_001";
const SQL_SR_001: &str = "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at)           SELECT $1,company_id,id,$2,'set.reused',jsonb_build_object('requestedSetId',$3::text)::text,$4 FROM attachment_sets WHERE id=$5 AND company_id=$6 AND created_by=$7 AND state='draft'"; // binds=7
pub const SR_002: &str = "SR_002";
const SQL_SR_002: &str = "INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,         expected_target_version,policy_version,classification,audience_json,retention_policy_key,retention_policy_version,retain_until,destruction_mode,         required_count,max_files,state,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,'draft',$20,$21,$22)"; // binds=22
pub const SR_003: &str = "SR_003";
const SQL_SR_003: &str = "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"; // binds=7
pub const SR_004: &str = "SR_004";
const SQL_SR_004: &str = "INSERT INTO attachment_links(id,set_id,caption,state,supersedes_link_id,created_by,created_at,expires_at) VALUES($1,$2,$3,'draft',$4,$5,$6,$7)"; // binds=7
pub const SR_005: &str = "SR_005";
const SQL_SR_005: &str = "INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,         owner_version,attachment_set_id,attachment_link_id,company_id,quarantine_key) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)"; // binds=18
pub const SR_006: &str = "SR_006";
const SQL_SR_006: &str = "UPDATE attachment_links SET upload_intent_id=$1 WHERE id=$2"; // binds=2
pub const SR_007: &str = "SR_007";
const SQL_SR_007: &str = "INSERT INTO attachment_events(id,company_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"; // binds=8
pub const SR_008: &str = "SR_008";
const SQL_SR_008: &str = "INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,           company_id,detected_media_type,source_state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,'quarantined')"; // binds=15
pub const SR_009: &str = "SR_009";
const SQL_SR_009: &str = "UPDATE upload_intents SET status='uploaded',actual_bytes=$1,checksum_sha256=$2,detected_media_type=$3,final_key=$4,updated_at=$5 WHERE id=$6 AND company_id=$7 AND status='pending'"; // binds=7
pub const SR_010: &str = "SR_010";
const SQL_SR_010: &str = "INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES($1,$2,1,'pending',$3)"; // binds=3
pub const SR_011: &str = "SR_011";
const SQL_SR_011: &str = "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES($1,$2,$3,'original',$4,$5,'stored',$6)"; // binds=6
pub const SR_012: &str = "SR_012";
const SQL_SR_012: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"; // binds=9
pub const SR_013: &str = "SR_013";
const SQL_SR_013: &str = "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) SELECT $1,$2,$3,$4,$5, 'status','allowed',$6 WHERE $7::text IS NOT NULL"; // binds=7
pub const SR_014: &str = "SR_014";
const SQL_SR_014: &str = "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,$6, 'allowed',$7)"; // binds=7
pub const SR_015: &str = "SR_015";
const SQL_SR_015: &str = "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,'denied',$7,$8)"; // binds=8
pub const SR_016: &str = "SR_016";
const SQL_SR_016: &str = "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"; // binds=7
pub const SR_017: &str = "SR_017";
const SQL_SR_017: &str = "UPDATE attachment_sets SET updated_at=$1 WHERE id=$2 AND company_id=$3 AND state='draft'"; // binds=3
pub const SR_018: &str = "SR_018";
const SQL_SR_018: &str = "INSERT INTO attachment_links(id,set_id,attachment_id,caption,state,created_by,created_at,expires_at) VALUES($1,$2,$3,$4,'draft',$5,$6,$7)"; // binds=7
pub const SR_019: &str = "SR_019";
const SQL_SR_019: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"; // binds=9
pub const SR_020: &str = "SR_020";
const SQL_SR_020: &str = "UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,updated_at=$3 WHERE id=$4 AND company_id=$5 AND state='draft'"; // binds=5
pub const SR_021: &str = "SR_021";
const SQL_SR_021: &str = "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"; // binds=7
pub const SR_022: &str = "SR_022";
const SQL_SR_022: &str = "INSERT INTO attachment_publications(id,company_id,source_link_id,target_type,target_id,derivative_object_key,derivative_media_type,derivative_checksum_sha256,         derivative_bytes,alt_text,ordinal,revision,state,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12, 'draft',$13,$14)"; // binds=14
pub const SR_023: &str = "SR_023";
const SQL_SR_023: &str = "INSERT INTO attachment_publication_orders(company_id,target_type,target_id,version,updated_by,updated_at) VALUES($1,$2,$3,1,$4,$5) ON CONFLICT(company_id,target_type,target_id) DO NOTHING"; // binds=5
pub const SR_024: &str = "SR_024";
const SQL_SR_024: &str = "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES($1,$2,$3,'publication',$4,$5,'stored',$6)"; // binds=6
pub const SR_025: &str = "SR_025";
const SQL_SR_025: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"; // binds=8
pub const SR_026: &str = "SR_026";
const SQL_SR_026: &str = "UPDATE attachment_publications SET state='revoked',revoked_by=$1,revoked_at=$2,revocation_reason=$3 WHERE id=$4 AND company_id=$5 AND state='approved'"; // binds=5
pub const SR_027: &str = "SR_027";
const SQL_SR_027: &str = "INSERT INTO attachment_events(id,company_id,link_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING"; // binds=7
pub const SR_028: &str = "SR_028";
const SQL_SR_028: &str = "UPDATE attachment_publications SET ordinal=$1 WHERE id=$2 AND company_id=$3"; // binds=3
pub const SR_029: &str = "SR_029";
const SQL_SR_029: &str = "INSERT INTO attachment_publication_orders(company_id,target_type,target_id,version,updated_by,updated_at) VALUES($1,$2,$3,2,$4,$5) ON CONFLICT(company_id,target_type,target_id) DO UPDATE SET version=attachment_publication_orders.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at WHERE attachment_publication_orders.version=$6"; // binds=6
pub const SR_030: &str = "SR_030";
const SQL_SR_030: &str = "INSERT INTO attachment_events(id,company_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6)"; // binds=6
pub const SR_031: &str = "SR_031";
const SQL_SR_031: &str = "UPDATE attachment_publications SET state='approved',approved_by=$1,approved_at=$2 WHERE id=$3 AND company_id=$4 AND state='draft'"; // binds=4
pub const SR_032: &str = "SR_032";
const SQL_SR_032: &str = "INSERT INTO attachment_events(id,company_id,link_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING"; // binds=7
pub const SR_033: &str = "SR_033";
const SQL_SR_033: &str = "UPDATE attachments SET source_state='scanning' WHERE id=$1 AND source_state IN ('quarantined','scan_error')"; // binds=1
pub const SR_034: &str = "SR_034";
const SQL_SR_034: &str = "UPDATE attachment_scan_attempts SET state='infected',scanner_engine=$1,scanner_signature=$2,scanner_definition_at=$3,result_code='FOUND',completed_at=$4,claim_until=NULL WHERE id=$5 AND lease_token=$6"; // binds=6
pub const SR_035: &str = "SR_035";
const SQL_SR_035: &str = "UPDATE attachments SET source_state='malware_rejected',detected_media_type=COALESCE(detected_media_type,$1),scanned_at=$2,scanner_definition_at=$3 WHERE id=$4"; // binds=4
pub const SR_036: &str = "SR_036";
const SQL_SR_036: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"; // binds=7
pub const SR_037: &str = "SR_037";
const SQL_SR_037: &str = "UPDATE attachment_scan_attempts SET state='clean',scanner_engine=$1,scanner_signature=NULL,scanner_definition_at=$2,result_code='OK',completed_at=$3,claim_until=NULL WHERE id=$4 AND lease_token=$5"; // binds=5
pub const SR_038: &str = "SR_038";
const SQL_SR_038: &str = "UPDATE attachments SET source_state='available',detected_media_type=COALESCE(detected_media_type,$1),scanned_at=$2,scanner_definition_at=$3,preview_object_key=$4,preview_media_type=$5,preview_checksum_sha256=$6,preview_bytes=$7 WHERE id=$8"; // binds=8
pub const SR_039: &str = "SR_039";
const SQL_SR_039: &str = "UPDATE upload_intents SET status='ready',updated_at=$1 WHERE id=$2"; // binds=2
pub const SR_040: &str = "SR_040";
const SQL_SR_040: &str = "UPDATE attachment_links SET expires_at=$1 WHERE upload_intent_id=$2 AND state='draft'"; // binds=2
pub const SR_041: &str = "SR_041";
const SQL_SR_041: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"; // binds=7
pub const SR_042: &str = "SR_042";
const SQL_SR_042: &str = "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES($1,$2,$3,'preview',$4,$5,'stored',$6)"; // binds=6
pub const SR_043: &str = "SR_043";
const SQL_SR_043: &str = "UPDATE attachment_scan_attempts SET state='error',error_code=$1,completed_at=$2,claim_until=NULL WHERE id=$3 AND lease_token=$4"; // binds=4
pub const SR_044: &str = "SR_044";
const SQL_SR_044: &str = "UPDATE attachments SET source_state='scan_error' WHERE id=$1"; // binds=1
pub const SR_045: &str = "SR_045";
const SQL_SR_045: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"; // binds=7
pub const SR_046: &str = "SR_046";
const SQL_SR_046: &str = "INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES($1,$2,$3,'pending',$4)"; // binds=4
pub const SR_047: &str = "SR_047";
const SQL_SR_047: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) SELECT $1,company_id,$2,$3,$4,$5,$6,$7,$8 FROM attachment_sets WHERE id=$9"; // binds=9
pub const SR_048: &str = "SR_048";
const SQL_SR_048: &str = "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,'system:jobs','set.expired','{}',$4)"; // binds=4
pub const SR_049: &str = "SR_049";
const SQL_SR_049: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,'system:jobs','link.expired','{}',$6)"; // binds=6
pub const SR_050: &str = "SR_050";
const SQL_SR_050: &str = "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,'system:jobs','set.expired','{}',$4)"; // binds=4
pub const SR_051: &str = "SR_051";
const SQL_SR_051: &str = "UPDATE attachments SET source_state='destroyed',destroyed_at=$1 WHERE id=$2 AND source_state='available' AND legal_hold=0 RETURNING company_id"; // binds=2
pub const SR_052: &str = "SR_052";
const SQL_SR_052: &str = "UPDATE attachment_storage_ledger SET state='destroyed',destroyed_at=$1 WHERE attachment_id=$2 AND state<>'destroyed'"; // binds=2
pub const SR_053: &str = "SR_053";
const SQL_SR_053: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,'system:jobs','source.destroyed','{}',$4)"; // binds=4
pub const SR_054: &str = "SR_054";
const SQL_SR_054: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,'system:jobs','source.bytes_deleted','{}',$4) ON CONFLICT(id) DO NOTHING"; // binds=4
pub const SR_055: &str = "SR_055";
const SQL_SR_055: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) SELECT $1,company_id,$2,'system:jobs','source.byte_delete_failed',$3,$4 FROM attachments WHERE id=$5"; // binds=5
pub const MIG_001: &str = "MIG_001";
const SQL_MIG_001: &str = "INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,retention_policy_key,retention_policy_version,retain_until,destruction_mode,required_count,max_files,state,created_by,created_at,updated_at,bound_at,bound_command_id)         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15::bigint,$16::bigint,$17,$18,$19,'draft',$20,$21,$22,NULL,NULL) ON CONFLICT(id) DO NOTHING"; // binds=22
pub const SR_068: &str = "SR_068";
const SQL_SR_068: &str = "WITH tgt AS (SELECT $1::bigint AS now, $2::text AS company_id, $3::text AS target_id, $4::bigint AS arg4), guarded AS (SELECT 1 FROM tgt WHERE EXISTS(SELECT 1 FROM leave_requests lr WHERE lr.id=tgt.target_id AND lr.company_id=tgt.company_id)), expired_links AS (UPDATE attachment_links link SET state='expired' FROM attachment_sets set_row, tgt WHERE link.set_id=set_row.id AND set_row.company_id=tgt.company_id AND set_row.target_type='leave_request' AND set_row.target_id=tgt.target_id AND set_row.state='draft' AND erpx_files_legacy_cleanup_set_allowed(set_row.id) AND link.state='draft' AND link.expires_at<=tgt.now AND EXISTS(SELECT 1 FROM guarded) RETURNING link.id,link.set_id,link.attachment_id), link_events AS (INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) SELECT 'attachment-event-'||gen_random_uuid()::text,set_row.company_id,expired_links.attachment_id,expired_links.id,expired_links.set_id,'system:jobs','link.expired','{}',tgt.now FROM expired_links JOIN attachment_sets set_row ON set_row.id=expired_links.set_id, tgt), expired_intents AS (UPDATE upload_intents intent SET status='expired',updated_at=tgt.now FROM attachment_sets set_row, tgt WHERE intent.attachment_set_id=set_row.id AND set_row.company_id=tgt.company_id AND set_row.target_type='leave_request' AND set_row.target_id=tgt.target_id AND set_row.state='draft' AND erpx_files_legacy_cleanup_set_allowed(set_row.id) AND intent.status='pending' AND intent.expires_at<=tgt.now AND EXISTS(SELECT 1 FROM guarded) RETURNING 1), expired_sets AS (UPDATE attachment_sets set_row SET state='expired',updated_at=tgt.now FROM tgt WHERE set_row.company_id=tgt.company_id AND set_row.target_type='leave_request' AND set_row.target_id=tgt.target_id AND set_row.state='draft' AND erpx_files_legacy_cleanup_set_allowed(set_row.id) AND NOT EXISTS(SELECT 1 FROM attachment_links active WHERE active.set_id=set_row.id AND active.state IN ('draft','bound')) AND ((EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=set_row.id) AND NOT EXISTS(SELECT 1 FROM attachment_links future_link WHERE future_link.set_id=set_row.id AND future_link.expires_at>tgt.now)) OR (NOT EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=set_row.id) AND set_row.created_at<=tgt.arg4)) AND EXISTS(SELECT 1 FROM guarded) RETURNING set_row.id,set_row.company_id), set_events AS (INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) SELECT 'attachment-event-'||gen_random_uuid()::text,expired_sets.company_id,expired_sets.id,'system:jobs','set.expired','{}',tgt.now FROM expired_sets, tgt) SELECT COALESCE(json_agg(json_build_object('links',(SELECT COALESCE(json_agg(expired_links),'[]'::json) FROM expired_links),'sets',(SELECT COALESCE(json_agg(expired_sets),'[]'::json) FROM expired_sets))),'[]'::json)::text FROM (SELECT 1) anchor"; // binds=4
pub const SR_065: &str = "SR_065";
const SQL_SR_065: &str = "WITH expired AS (UPDATE attachment_links link SET state='expired' FROM attachment_sets set_row WHERE link.set_id=set_row.id AND set_row.target_type<>'leave_request' AND erpx_files_legacy_cleanup_set_allowed(set_row.id) AND link.state='draft' AND link.expires_at<=$1 RETURNING link.id,link.set_id,link.attachment_id), events AS (INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) SELECT 'attachment-event-'||gen_random_uuid()::text,set_row.company_id,expired.attachment_id,expired.id,expired.set_id,'system:jobs','link.expired','{}',$1 FROM expired JOIN attachment_sets set_row ON set_row.id=expired.set_id) SELECT COALESCE(json_agg(expired),'[]'::json)::text FROM expired"; // binds=1
pub const SR_067: &str = "SR_067";
const SQL_SR_067: &str = "WITH expired AS (UPDATE attachment_sets s SET state='expired',updated_at=$1 WHERE s.state='draft' AND s.target_type<>'leave_request' AND erpx_files_legacy_cleanup_set_allowed(s.id) AND NOT EXISTS(SELECT 1 FROM attachment_links active WHERE active.set_id=s.id AND active.state IN ('draft','bound')) AND ((EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=s.id) AND NOT EXISTS(SELECT 1 FROM attachment_links future_link WHERE future_link.set_id=s.id AND future_link.expires_at>$2)) OR (NOT EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=s.id) AND s.created_at<=$3)) RETURNING id,company_id), events AS (INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) SELECT 'attachment-event-'||gen_random_uuid()::text,expired.company_id,expired.id,'system:jobs','set.expired','{}',$1 FROM expired) SELECT COALESCE(json_agg(expired),'[]'::json)::text FROM expired"; // binds=3
pub const SR_059: &str = "SR_059";
const SQL_SR_059: &str = "WITH expired AS (UPDATE attachment_links link SET state='expired' FROM attachment_sets set_row WHERE link.set_id=set_row.id AND set_row.target_type<>'leave_request' AND erpx_files_legacy_cleanup_set_allowed(set_row.id) AND link.state='draft' AND link.expires_at<=$1 RETURNING link.id,link.set_id,link.attachment_id) SELECT COALESCE(json_agg(expired),'[]'::json)::text FROM expired"; // binds=1
pub const SR_060: &str = "SR_060";
const SQL_SR_060: &str = "WITH expired AS (UPDATE upload_intents intent SET status='expired',updated_at=$1 WHERE intent.attachment_set_id IS NOT NULL AND intent.status='pending' AND intent.expires_at<=$2 AND erpx_files_legacy_cleanup_set_allowed(intent.attachment_set_id) AND NOT EXISTS(SELECT 1 FROM attachment_sets set_row WHERE set_row.id=intent.attachment_set_id AND set_row.target_type='leave_request') RETURNING 1) SELECT COALESCE(json_agg(expired),'[]'::json)::text FROM expired"; // binds=2
pub const SR_061: &str = "SR_061";
const SQL_SR_061: &str = "WITH expired AS (UPDATE attachment_sets s SET state='expired',updated_at=$1 WHERE s.state='draft' AND s.target_type<>'leave_request' AND erpx_files_legacy_cleanup_set_allowed(s.id) AND NOT EXISTS(SELECT 1 FROM attachment_links active WHERE active.set_id=s.id AND active.state IN ('draft','bound')) AND ((EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=s.id) AND NOT EXISTS(SELECT 1 FROM attachment_links future_link WHERE future_link.set_id=s.id AND future_link.expires_at>$2)) OR (NOT EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=s.id) AND s.created_at<=$3)) RETURNING id,company_id) SELECT COALESCE(json_agg(expired),'[]'::json)::text FROM expired"; // binds=3
pub const SR_062: &str = "SR_062";
const SQL_SR_062: &str = "WITH expired AS (UPDATE attachment_links link SET state='expired' FROM attachment_sets set_row WHERE link.set_id=set_row.id AND set_row.company_id=$1 AND set_row.target_type='leave_request' AND set_row.target_id=$2 AND set_row.state='draft' AND erpx_files_legacy_cleanup_set_allowed(set_row.id) AND link.state='draft' AND link.expires_at<=$3 RETURNING link.id,link.set_id,link.attachment_id) SELECT COALESCE(json_agg(expired),'[]'::json)::text FROM expired"; // binds=3
pub const SR_063: &str = "SR_063";
const SQL_SR_063: &str = "WITH expired AS (UPDATE upload_intents intent SET status='expired',updated_at=$1 FROM attachment_sets set_row WHERE intent.attachment_set_id=set_row.id AND set_row.company_id=$2 AND set_row.target_type='leave_request' AND set_row.target_id=$3 AND set_row.state='draft' AND erpx_files_legacy_cleanup_set_allowed(set_row.id) AND intent.status='pending' AND intent.expires_at<=$4 RETURNING 1) SELECT COALESCE(json_agg(expired),'[]'::json)::text FROM expired"; // binds=4
pub const SR_064: &str = "SR_064";
const SQL_SR_064: &str = "WITH expired AS (UPDATE attachment_sets set_row SET state='expired',updated_at=$1 WHERE set_row.company_id=$2 AND set_row.target_type='leave_request' AND set_row.target_id=$3 AND set_row.state='draft' AND erpx_files_legacy_cleanup_set_allowed(set_row.id) AND NOT EXISTS(SELECT 1 FROM attachment_links active WHERE active.set_id=set_row.id AND active.state IN ('draft','bound')) AND ((EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=set_row.id) AND NOT EXISTS(SELECT 1 FROM attachment_links future_link WHERE future_link.set_id=set_row.id AND future_link.expires_at>$4)) OR (NOT EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=set_row.id) AND set_row.created_at<=$5)) RETURNING set_row.id,set_row.company_id) SELECT COALESCE(json_agg(expired),'[]'::json)::text FROM expired"; // binds=5
pub const SR_056: &str = "SR_056";
const SQL_SR_056: &str = "UPDATE attachment_links l SET attachment_id=$1 FROM attachment_sets s WHERE l.id=$2 AND l.set_id=s.id AND s.company_id=$3 AND l.state='draft'"; // binds=3
pub const SR_057: &str = "SR_057";
const SQL_SR_057: &str = "UPDATE attachment_links l SET attachment_id=$1 FROM attachment_sets s WHERE l.id=$2 AND l.set_id=s.id AND s.company_id=$3 AND l.state='draft' AND s.state='draft'"; // binds=3
pub const SR_058: &str = "SR_058";
const SQL_SR_058: &str = "UPDATE attachment_links l SET state='bound',bound_at=$1,binding_type=$2,binding_id=$3 FROM attachment_sets s WHERE l.id=$4 AND l.set_id=s.id AND s.company_id=$5 AND l.state='draft'"; // binds=5
pub const MIG_002: &str = "MIG_002";
const SQL_MIG_002: &str = "INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES($1,$2,'draft',$3,$4,$5) ON CONFLICT(id) DO NOTHING"; // binds=5
pub const MIG_003: &str = "MIG_003";
const SQL_MIG_003: &str = "INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,revision,status,created_by,expires_at,created_at,updated_at,owner_version,attachment_set_id,attachment_link_id,company_id,quarantine_key)         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12, 'uploaded',$13,$14,$15,$16,$17,$18,$19,$20,$21) ON CONFLICT(id) DO NOTHING"; // binds=21
pub const MIG_004: &str = "MIG_004";
const SQL_MIG_004: &str = "UPDATE attachment_links SET upload_intent_id=$1 WHERE id=$2 AND upload_intent_id IS NULL"; // binds=2
pub const MIG_005: &str = "MIG_005";
const SQL_MIG_005: &str = "INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,legal_hold,created_at,company_id,source_state)         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13, $14,$15,'quarantined') ON CONFLICT(id) DO NOTHING"; // binds=15
pub const MIG_006: &str = "MIG_006";
const SQL_MIG_006: &str = "UPDATE attachment_links SET attachment_id=$1 WHERE id=$2 AND attachment_id IS NULL"; // binds=2
pub const MIG_007: &str = "MIG_007";
const SQL_MIG_007: &str = "INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES($1,$2,1,'pending',$3) ON CONFLICT(attachment_id,attempt) DO NOTHING"; // binds=3
pub const MIG_008: &str = "MIG_008";
const SQL_MIG_008: &str = "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES($1,$2,$3,'original',$4,$5,'stored',$6) ON CONFLICT(object_key) DO NOTHING"; // binds=6
pub const MIG_009: &str = "MIG_009";
const SQL_MIG_009: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,'system:migration','legacy.specialized_backfilled',$6,$7) ON CONFLICT(id) DO NOTHING"; // binds=7
pub const MIG_010: &str = "MIG_010";
const SQL_MIG_010: &str = "UPDATE attachment_links SET state='bound',bound_at=$1,binding_type=$2,binding_id=$3 WHERE id=$4 AND state='draft'"; // binds=4
pub const MIG_011: &str = "MIG_011";
const SQL_MIG_011: &str = "UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,updated_at=$3 WHERE id=$4 AND state='draft'"; // binds=4
pub const MIG_012: &str = "MIG_012";
const SQL_MIG_012: &str = "UPDATE attachments SET legacy_sanitized_only=true WHERE id=$1 AND company_id IS NULL AND source_state='legacy_unscanned'"; // binds=1
pub const MIG_013: &str = "MIG_013";
const SQL_MIG_013: &str = "UPDATE attachments SET source_state='quarantined' WHERE id=$1 AND company_id IS NULL AND source_state='legacy_unscanned'"; // binds=1
pub const MIG_014: &str = "MIG_014";
const SQL_MIG_014: &str = "UPDATE attachments SET source_state='quarantined' WHERE id=$1 AND company_id IS NULL AND source_state='legacy_unscanned'"; // binds=1
pub const MIG_015: &str = "MIG_015";
const SQL_MIG_015: &str = "UPDATE attachments SET source_state='quarantined' WHERE id=$1 AND company_id IS NULL AND source_state='legacy_unscanned'"; // binds=1
pub const MIG_016: &str = "MIG_016";
const SQL_MIG_016: &str = "INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,retention_policy_key,retention_policy_version,retain_until,destruction_mode,required_count,max_files,state,created_by,created_at,updated_at,bound_at,bound_command_id)         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15::bigint,$16::bigint,$17,$18,$19,'bound',$20,$21,$22,$23,$24) ON CONFLICT(id) DO NOTHING"; // binds=24
pub const MIG_017: &str = "MIG_017";
const SQL_MIG_017: &str = "INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id)         VALUES($1,$2,$3,$4,'bound',$5,$6,$7,$8, 'legacy_import',$9) ON CONFLICT(id) DO NOTHING"; // binds=9
pub const MIG_018: &str = "MIG_018";
const SQL_MIG_018: &str = "UPDATE attachments SET company_id=$1,source_state='quarantined' WHERE id=$2 AND company_id IS NULL"; // binds=2
pub const MIG_019: &str = "MIG_019";
const SQL_MIG_019: &str = "UPDATE upload_intents SET attachment_set_id=$1,attachment_link_id=$2,company_id=$3,quarantine_key=$4,status='uploaded',updated_at=$5 WHERE id=$6"; // binds=6
pub const MIG_020: &str = "MIG_020";
const SQL_MIG_020: &str = "INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES($1,$2,1,'pending',$3) ON CONFLICT(attachment_id,attempt) DO NOTHING"; // binds=3
pub const MIG_021: &str = "MIG_021";
const SQL_MIG_021: &str = "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES($1,$2,$3,'original',$4,$5,'stored',$6) ON CONFLICT(object_key) DO NOTHING"; // binds=6
pub const MIG_022: &str = "MIG_022";
const SQL_MIG_022: &str = "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,'system:migration','legacy.backfilled',$6,$7) ON CONFLICT(id) DO NOTHING"; // binds=7
pub const ORPH_001: &str = "ORPH_001";
const SQL_ORPH_001: &str = "INSERT INTO attachment_orphan_preservations(id,object_key,object_sha256,manifest_sha256,bytes,preserved_directory,reference_proof_sha256,operator_id,reason,created_at,preservation_mode) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'file-store-sealed') ON CONFLICT(object_key) DO NOTHING"; // binds=10
pub const ORPH_002: &str = "ORPH_002";
const SQL_ORPH_002: &str = "INSERT INTO attachment_orphan_preservation_events(id,preservation_id,event_kind,operator_id,recorded_at) VALUES($1,$2,'preserved',$3,$4) ON CONFLICT(preservation_id,event_kind) DO NOTHING"; // binds=4
pub const ORPH_003: &str = "ORPH_003";
const SQL_ORPH_003: &str = "INSERT INTO attachment_orphan_preservations(id,object_key,object_sha256,manifest_sha256,bytes,preserved_directory,reference_proof_sha256,operator_id,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"; // binds=10
pub const MIG_023: &str = "MIG_023";
const SQL_MIG_023: &str = "INSERT INTO attachment_derivatives(id,attachment_id,company_id,page_number,object_key,checksum_sha256,original_checksum,bytes,created_by,created_at) VALUES($1,$2,$3,1,$4,$5,$6,$7,$8,$9) ON CONFLICT(id) DO NOTHING"; // binds=9
pub const MIG_024: &str = "MIG_024";
const SQL_MIG_024: &str = "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES($1,$2,$3,'derivative',$4,$5,'stored',$6) ON CONFLICT(object_key) DO NOTHING"; // binds=6


/// Social-draft revoke writers (were the last 3 Files owner TS writes).
pub const SF_EXPIRE_POST_UPLOADS: &str = "SF_EXPIRE_POST_UPLOADS";
const SQL_SF_EXPIRE_POST_UPLOADS: &str = "UPDATE upload_intents u SET status='expired',updated_at=$1 WHERE u.owner_type='social_draft' AND u.status='ready' AND u.owner_id IN (SELECT id FROM (SELECT id,author_id,company_id,published_post_id,edit_post_id,edit_saved_version,cancelled_at,edit_post_version,comment_parent_post_id,published_comment_id FROM social_post_drafts) drafts WHERE published_post_id=$2 OR edit_post_id=$3) AND NOT EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND a.revoked_at IS NULL)"; // binds=3
pub const SF_EXPIRE_DRAFT_UPLOADS: &str = "SF_EXPIRE_DRAFT_UPLOADS";
const SQL_SF_EXPIRE_DRAFT_UPLOADS: &str = "UPDATE upload_intents u SET status='expired',updated_at=$1 WHERE u.owner_type='social_draft' AND u.owner_id=$2 AND u.status='ready' AND NOT EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND a.revoked_at IS NULL)"; // binds=2
pub const SF_REVOKE_DRAFT: &str = "SF_REVOKE_DRAFT";
const SQL_SF_REVOKE_DRAFT: &str = "UPDATE attachments SET revoked_at=$1 WHERE owner_type='social_draft' AND owner_id=$2 AND revoked_at IS NULL AND NOT (id = ANY(ARRAY(SELECT jsonb_array_elements_text($3::jsonb))))";
pub const SF_REVOKE_POST: &str = "SF_REVOKE_POST";
const SQL_SF_REVOKE_POST: &str = "UPDATE attachments SET revoked_at=$1 WHERE owner_type='social_draft' AND revoked_at IS NULL AND owner_id IN (SELECT id FROM (SELECT id,author_id,company_id,published_post_id,edit_post_id,edit_saved_version,cancelled_at,edit_post_version,comment_parent_post_id,published_comment_id FROM social_post_drafts) drafts WHERE published_post_id=$2 OR edit_post_id=$3) AND NOT (id = ANY(ARRAY(SELECT jsonb_array_elements_text($4::jsonb))))";
pub const SF_REVOKE_COMMENT: &str = "SF_REVOKE_COMMENT";
const SQL_SF_REVOKE_COMMENT: &str = "UPDATE attachments SET revoked_at=$1 WHERE owner_type='social_draft' AND owner_id=$2 AND revoked_at IS NULL AND NOT (id = ANY(ARRAY(SELECT jsonb_array_elements_text($3::jsonb))))";

/// Brand publication writers (were the last 4 brand TS writers, retired as
/// 503 stubs). Byte-identical to the retired TypeScript facts writers, with
/// the purpose-key column choice frozen per kind: primary vs inverse
/// publication columns are separate kinds so owner SQL stays static.
pub const BRAND_PUB_71P: &str = "BRAND_PUB_71P";
const SQL_BRAND_PUB_71P: &str = "INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at,primary_publication_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(scope_kind,scope_id) DO UPDATE SET primary_publication_id=excluded.primary_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at"; // binds=7
pub const BRAND_PUB_71I: &str = "BRAND_PUB_71I";
const SQL_BRAND_PUB_71I: &str = "INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at,inverse_publication_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(scope_kind,scope_id) DO UPDATE SET inverse_publication_id=excluded.inverse_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at"; // binds=7
pub const BRAND_PUB_72: &str = "BRAND_PUB_72";
const SQL_BRAND_PUB_72: &str = "INSERT INTO storefront_app_brands(company_id,short_name,accent_color,updated_by,updated_at,logo_publication_id) SELECT id,left(display_name,40),'#3659c9',$1,$2,$3 FROM organization_nodes WHERE id=$4 AND kind='company' AND status='active' ON CONFLICT(company_id) DO UPDATE SET logo_publication_id=excluded.logo_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at"; // binds=4
pub const BRAND_PUB_73P: &str = "BRAND_PUB_73P";
const SQL_BRAND_PUB_73P: &str = "UPDATE organization_brand_profiles SET primary_publication_id=NULL WHERE scope_kind=$1 AND scope_id=$2 AND primary_publication_id=$3"; // binds=3
pub const BRAND_PUB_73I: &str = "BRAND_PUB_73I";
const SQL_BRAND_PUB_73I: &str = "UPDATE organization_brand_profiles SET inverse_publication_id=NULL WHERE scope_kind=$1 AND scope_id=$2 AND inverse_publication_id=$3"; // binds=3
pub const BRAND_PUB_74: &str = "BRAND_PUB_74";
const SQL_BRAND_PUB_74: &str = "UPDATE storefront_app_brands SET logo_publication_id=NULL WHERE company_id=$1 AND logo_publication_id=$2"; // binds=2

/// Owner-owned read: the project attachment-set routing fact. This is the
/// native successor of the retired TypeScript project-attachment selector. It
/// carries no write ledger: the caller's unit already holds the read scope.
pub const RD_PROJECT_ATTACHMENT_TARGET: &str = "RD_PROJECT_ATTACHMENT_TARGET";
const SQL_RD_PROJECT_ATTACHMENT_TARGET: &str = "SELECT target_type FROM attachment_sets WHERE id=$1 AND company_id=$2"; // binds=2

/// Read kinds plan the owner SQL only; no receipt/audit/outbox write ledger.
pub fn is_read(kind: &str) -> bool { kind == RD_PROJECT_ATTACHMENT_TARGET }

/// Every kind this owner module accepts. Anything else is fail-closed.
const KINDS: &[&str] = &[
    SR_001,
    SR_002,
    SR_003,
    SR_004,
    SR_005,
    SR_006,
    SR_007,
    SR_008,
    SR_009,
    SR_010,
    SR_011,
    SR_012,
    SR_013,
    SR_014,
    SR_015,
    SR_016,
    SR_017,
    SR_018,
    SR_019,
    SR_020,
    SR_021,
    SR_022,
    SR_023,
    SR_024,
    SR_025,
    SR_026,
    SR_027,
    SR_028,
    SR_029,
    SR_030,
    SR_031,
    SR_032,
    SR_033,
    SR_034,
    SR_035,
    SR_036,
    SR_037,
    SR_038,
    SR_039,
    SR_040,
    SR_041,
    SR_042,
    SR_043,
    SR_044,
    SR_045,
    SR_046,
    SR_047,
    SR_048,
    SR_049,
    SR_050,
    SR_051,
    SR_052,
    SR_053,
    SR_054,
    SR_055,
    SF_REVOKE_DRAFT,
    SF_REVOKE_POST,
    SF_REVOKE_COMMENT,
    SF_EXPIRE_DRAFT_UPLOADS,
    SF_EXPIRE_POST_UPLOADS,
    SR_056,
    SR_057,
    SR_058,
    SR_059,
    SR_060,
    SR_061,
    SR_062,
    SR_063,
    SR_064,
    SR_065,
    SR_067,
    SR_068,
    BRAND_PUB_71P,
    BRAND_PUB_71I,
    BRAND_PUB_72,
    BRAND_PUB_73P,
    BRAND_PUB_73I,
    BRAND_PUB_74,
    MIG_001,
    MIG_002,
    MIG_003,
    MIG_004,
    MIG_005,
    MIG_006,
    MIG_007,
    MIG_008,
    MIG_009,
    MIG_010,
    MIG_011,
    MIG_012,
    MIG_013,
    MIG_014,
    MIG_015,
    MIG_016,
    MIG_017,
    MIG_018,
    MIG_019,
    MIG_020,
    MIG_021,
    MIG_022,
    ORPH_001,
    ORPH_002,
    ORPH_003,
    MIG_023,
    MIG_024,
    RD_PROJECT_ATTACHMENT_TARGET,
];
pub fn supported(kind: &str) -> bool { KINDS.contains(&kind) }
pub fn sql(kind: &str) -> Option<&'static str> { Some(match kind {
    SR_001 => SQL_SR_001,
    SR_002 => SQL_SR_002,
    SR_003 => SQL_SR_003,
    SR_004 => SQL_SR_004,
    SR_005 => SQL_SR_005,
    SR_006 => SQL_SR_006,
    SR_007 => SQL_SR_007,
    SR_008 => SQL_SR_008,
    SR_009 => SQL_SR_009,
    SR_010 => SQL_SR_010,
    SR_011 => SQL_SR_011,
    SR_012 => SQL_SR_012,
    SR_013 => SQL_SR_013,
    SR_014 => SQL_SR_014,
    SR_015 => SQL_SR_015,
    SR_016 => SQL_SR_016,
    SR_017 => SQL_SR_017,
    SR_018 => SQL_SR_018,
    SR_019 => SQL_SR_019,
    SR_020 => SQL_SR_020,
    SR_021 => SQL_SR_021,
    SR_022 => SQL_SR_022,
    SR_023 => SQL_SR_023,
    SR_024 => SQL_SR_024,
    SR_025 => SQL_SR_025,
    SR_026 => SQL_SR_026,
    SR_027 => SQL_SR_027,
    SR_028 => SQL_SR_028,
    SR_029 => SQL_SR_029,
    SR_030 => SQL_SR_030,
    SR_031 => SQL_SR_031,
    SR_032 => SQL_SR_032,
    SR_033 => SQL_SR_033,
    SR_034 => SQL_SR_034,
    SR_035 => SQL_SR_035,
    SR_036 => SQL_SR_036,
    SR_037 => SQL_SR_037,
    SR_038 => SQL_SR_038,
    SR_039 => SQL_SR_039,
    SR_040 => SQL_SR_040,
    SR_041 => SQL_SR_041,
    SR_042 => SQL_SR_042,
    SR_043 => SQL_SR_043,
    SR_044 => SQL_SR_044,
    SR_045 => SQL_SR_045,
    SR_046 => SQL_SR_046,
    SR_047 => SQL_SR_047,
    SR_048 => SQL_SR_048,
    SR_049 => SQL_SR_049,
    SR_050 => SQL_SR_050,
    SR_051 => SQL_SR_051,
    SR_052 => SQL_SR_052,
    SR_053 => SQL_SR_053,
    SR_054 => SQL_SR_054,
    SR_055 => SQL_SR_055,
    SR_056 => SQL_SR_056,
    SR_057 => SQL_SR_057,
    SR_058 => SQL_SR_058,
    SR_059 => SQL_SR_059,
    SR_060 => SQL_SR_060,
    SR_061 => SQL_SR_061,
    SR_062 => SQL_SR_062,
    SR_063 => SQL_SR_063,
    SR_064 => SQL_SR_064,
    SR_065 => SQL_SR_065,
    SR_067 => SQL_SR_067,
    SR_068 => SQL_SR_068,
    MIG_001 => SQL_MIG_001,
    MIG_002 => SQL_MIG_002,
    MIG_003 => SQL_MIG_003,
    MIG_004 => SQL_MIG_004,
    MIG_005 => SQL_MIG_005,
    MIG_006 => SQL_MIG_006,
    MIG_007 => SQL_MIG_007,
    MIG_008 => SQL_MIG_008,
    MIG_009 => SQL_MIG_009,
    MIG_010 => SQL_MIG_010,
    MIG_011 => SQL_MIG_011,
    MIG_012 => SQL_MIG_012,
    MIG_013 => SQL_MIG_013,
    MIG_014 => SQL_MIG_014,
    MIG_015 => SQL_MIG_015,
    MIG_016 => SQL_MIG_016,
    MIG_017 => SQL_MIG_017,
    MIG_018 => SQL_MIG_018,
    MIG_019 => SQL_MIG_019,
    MIG_020 => SQL_MIG_020,
    MIG_021 => SQL_MIG_021,
    MIG_022 => SQL_MIG_022,
    ORPH_001 => SQL_ORPH_001,
    ORPH_002 => SQL_ORPH_002,
    ORPH_003 => SQL_ORPH_003,
    SF_REVOKE_DRAFT => SQL_SF_REVOKE_DRAFT,
    SF_EXPIRE_DRAFT_UPLOADS => SQL_SF_EXPIRE_DRAFT_UPLOADS,
    SF_EXPIRE_POST_UPLOADS => SQL_SF_EXPIRE_POST_UPLOADS,
    SF_REVOKE_POST => SQL_SF_REVOKE_POST,
    SF_REVOKE_COMMENT => SQL_SF_REVOKE_COMMENT,
    BRAND_PUB_71P => SQL_BRAND_PUB_71P,
    BRAND_PUB_71I => SQL_BRAND_PUB_71I,
    BRAND_PUB_72 => SQL_BRAND_PUB_72,
    BRAND_PUB_73P => SQL_BRAND_PUB_73P,
    BRAND_PUB_73I => SQL_BRAND_PUB_73I,
    BRAND_PUB_74 => SQL_BRAND_PUB_74,
    MIG_023 => SQL_MIG_023,
    MIG_024 => SQL_MIG_024,
    RD_PROJECT_ATTACHMENT_TARGET => SQL_RD_PROJECT_ATTACHMENT_TARGET,
    _ => return None,
}) }
pub fn invalid() -> BusinessError { BusinessError::new(FailureKind::InvalidInput, "FILES_COMMAND_INVALID", "Use one bounded Files command.") }
