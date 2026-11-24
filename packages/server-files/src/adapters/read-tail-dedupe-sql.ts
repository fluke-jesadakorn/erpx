/** Byte-identical read-tail SQL shared by owner adapters. Text only: every call
 * site still prepares and executes its own statement in its supplied transaction
 * with its own binds; no SQL text is accepted from a caller and no result is
 * cached here. */
export const attachmentSetPurposeSql = (placeholders: string) => `SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`;
export const workforceActorScopeSql = (where: string) => `SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${where}`;
export const SCANS_WITNESS_SQL = "'scans',(SELECT jsonb_agg(to_jsonb(scan) ORDER BY scan.id) FROM attachment_scan_attempts scan WHERE scan.attachment_id=a.id))::text witness";
export const NO_ATTACHMENT_LINK_SQL = "SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id";
