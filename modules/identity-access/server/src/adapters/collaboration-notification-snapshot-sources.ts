import 'server-only';
import { assignmentsProjection } from './assignment-projection-source';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "customer_authority_notifications_assignments_m_join": `JOIN (${assignmentsProjection}) m`,
  "customer_authority_notifications_assignments_a_join": `JOIN (${assignmentsProjection}) a`,
  "customer_portal_notifications_assignments_a_from": `FROM (${assignmentsProjection}) a`,
  "organization_change_notice_assignments_a_join": `JOIN (${assignmentsProjection}) a`,
  "people_roster_facts_assignments_a_from": `FROM (${assignmentsProjection}) a`,
  "quality_notifications_assignments_a_join": `JOIN (${assignmentsProjection}) a`,
  "quality_notifications_assignments_a_from": `FROM (${assignmentsProjection}) a`,
  "quality_notifications_assignments_m_join": `JOIN (${assignmentsProjection}) m`,
  "rfq_notifications_assignments_a_join": `JOIN (${assignmentsProjection}) a`,
  "supplier_batch_notifications_assignments_a_join": `JOIN (${assignmentsProjection}) a`,
  "supplier_bill_notifications_assignments_a_join": `JOIN (${assignmentsProjection}) a`,
  "supplier_promise_notifications_assignments_a_join": `JOIN (${assignmentsProjection}) a`
} as const;}
