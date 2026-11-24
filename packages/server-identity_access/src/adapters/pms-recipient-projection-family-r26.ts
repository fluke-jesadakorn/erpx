/** Shared PMS assignment-recipient projection head used by the PMS command
 * facts assignment list and the PMS source-recipient list. Both callers select
 * the same distinct user_id/legal_name projection over `assignments`
 * joined to the people snapshot; only the injected people-snapshot join
 * fragment differs. The projection, table and the fixed `ON h.user_id=... AND
 * h.company_id=...` predicates are pinned here; each caller supplies its own
 * join fragment and appends its own WHERE/ORDER tail, so no statement semantics
 * change. No lock, write, session authority, native kind or earlier-round
 * helper is changed. */
export const pmsRecipientProjectionSql = (join: string): string =>
  `SELECT DISTINCT a.user_id id,COALESCE(h.legal_name,a.user_id) name FROM assignments a LEFT ${join} ON h.user_id=a.user_id AND h.company_id=a.company_id`;
