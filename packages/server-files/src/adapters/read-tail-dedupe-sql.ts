/** Byte-identical read-tail SQL shared by owner adapters. Text only: every call
 * site still prepares and executes its own statement in its supplied transaction
 * with its own binds; no SQL text is accepted from a caller and no result is
 * cached here. */
export const attachmentSetPurposeSql = (placeholders: string) => `SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`;
export const workforceActorScopeSql = (where: string) => `SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${where}`;
export const scansAggSql = (alias: string) => `(SELECT jsonb_agg(to_jsonb(scan) ORDER BY scan.id) FROM attachment_scan_attempts scan WHERE scan.attachment_id=${alias}.id)`;
export const SCANS_WITNESS_SQL = `'scans',${scansAggSql('a')})::text witness`;
export const NO_ATTACHMENT_LINK_SQL = "SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id";
/** Byte-identical link/set built fragments shared by the finance, legal and
 * generic owner guards. Text only: call sites render the same statement with
 * their own binds and predicates; no SQL or relation name is accepted from a
 * caller and no result is cached here. */
export const LINK_SET_ATTACHMENT_EXISTS_SQL = "EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id";
export const LINK_SET_ID_SELECT_SQL = "SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id";
/** Byte-identical link/attachment projection prefix shared by the finance-review
 * and service-repository selectors. Text only: each call site renders its own
 * statement with its own binds; no SQL text is accepted from a caller. */
export const LINK_ATTACHMENT_SELECT_SQL = "SELECT l.id link_id,l.attachment_id";
