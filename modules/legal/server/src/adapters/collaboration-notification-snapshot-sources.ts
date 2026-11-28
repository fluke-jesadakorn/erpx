import 'server-only';
import {closedProjection} from './closed-projection-source';
import {legalMembersSnapshotSource} from './legal-members-read-sources';
const legalItemsSnapshotSql = closedProjection(['id','company_id','branch_id','department_id','kind','title','description','classification','country','jurisdiction','language','currency','timezone','priority','due_date','owner_id','created_by','state','version','created_at','updated_at','escalation_owner','sla_started_at','sla_due_at','intake_form_id','intake_answers'], ', ', 'legal_items');
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "legal_hold_notifications_legal_items_i_join": `JOIN (${legalItemsSnapshotSql}) i`,
  "legal_law_source_notifications_legal_items_i_from": `FROM (${legalItemsSnapshotSql}) i`,
  "legal_law_source_notifications_legal_members_m_from": legalMembersSnapshotSource,
  "legal_reminder_notifications_legal_items_i_join": `JOIN (${legalItemsSnapshotSql}) i`,
  "legal_reminder_notifications_legal_settings_s_join": closedProjection(['company_id','enabled','ai_enabled','version','reminder_days'], ', ', 'legal_settings', ['JOIN (', ') s']),
  "legal_reminder_notifications_legal_members_m_from": legalMembersSnapshotSource,
  "legal_reminder_notifications_legal_items_i_from": `FROM (${legalItemsSnapshotSql}) i`,
  "legal_reminder_notifications_legal_obligations_o_join": closedProjection(['item_id','data'], ', ', 'legal_obligations', ['JOIN (', ') o'])
} as const;}
