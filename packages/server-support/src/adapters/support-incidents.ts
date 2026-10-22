import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { supportNotificationEffects } from "@erpx/server-collaboration/public/support-notification-effects";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Row } from "@erpx/server-commerce/public/commerce";
import type { PreparedAttachmentBinding } from "@erpx/server-files/public/attachments/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { getSupportTicket, supportRecipients } from "@erpx/server-support/adapters/support";
import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { createGetSupportIncident, createSupportIncident as createIncident, createChangeSupportIncident, createIncidentWorkspace, createIncidentDetail, createIncidentWork } from "@erpx/server-support/application/support-incidents";
export { incidentCreateSchema, incidentChangeSchema } from "@erpx/server-support/domain/support-incidents";
const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
const eligible = supportRecipientEligibility();
const reviewer = `m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${eligible.replaceAll('t.owner_id', 'm.user_id')}`;
export function supportIncidentReadPredicate(a: WorkContext, alias = 't') {
    return { sql: `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
  AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_role_allows(g.role,g.company_id,'support.read') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id))`, bindings: [a.assignmentId, a.userId, a.role] };
}
function notify(db: SqlDatabase, id: string, key: string) {
    return supportNotificationEffects(db).incidentUpdated().bind(`incident-notice-${key}-`, id);
}
export async function getSupportIncident(a: WorkContext, id: string, provided?:SqlDatabase) {
    const runtime = nativeSupportRuntime(a,provided);
    await runtime.requirePurchase("support.incidents");
    return createGetSupportIncident({ ...runtime, incident: async (a, id) => {
            const write = await runtime.database(), permission = supportIncidentReadPredicate(a);
            return await write.prepare(`SELECT t.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,
  CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewer} AND m.user_id=?) THEN 1 ELSE 0 END reviewer_eligible,
  (SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewer}) reviewer_count
  FROM support_incidents t WHERE t.id=? AND ${scope} AND ${permission.sql}`).bind(a.userId, id, ...args(a), ...permission.bindings).first<Row>();
        } })(a, id);
}
export async function createSupportIncident(a: WorkContext, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createIncident({ ...runtime, incident: getSupportIncident, ticket: getSupportTicket, prepare: async (command, input, source) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            return { native: [
                    assertion(write, key, 'source', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.version=? AND t.source_kind='sales_order' AND ${scope})`, [input.ticketId, input.ticketVersion, ...args(a)]),
                    write.prepare('INSERT INTO support_incidents(id,company_id,branch_id,department_id,title,impact,severity,owner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(id, a.companyId, source.branch_id, source.department_id, input.title, input.impact, input.severity, input.ownerId, input.dueDate, a.userId, now, now),
                    assertion(write, key, 'owner', `EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=? AND ${eligible})`, [id]),
                    write.prepare('INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at) VALUES(?,?,?,?,?)').bind(id, input.ticketId, input.ticketVersion, a.userId, now),
                    write.prepare("INSERT INTO support_incident_events VALUES(?,?,1,'create',?,?,?,?::jsonb)").bind(`incident-event-${key}`, id, a.userId, input.impact, now, JSON.stringify(input)), notify(write, id, key),
                ] };
        } })(a, raw, key);
}
export async function changeSupportIncident(a: WorkContext, id: string, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createChangeSupportIncident({ ...runtime, incident: getSupportIncident, ticket: getSupportTicket, binding: async (a, id, input, key, contract) => { const write = await runtime.database(), attachmentSetIds = input.attachmentSetIds, prepared = await prepareAttachmentBinding(write, a, { attachmentSetIds, bindingContract: contract, targetId: id, expectedTargetVersion: input.expectedVersion, commandId: key, bindingId: `${id}:${input.action}:${input.expectedVersion + 1}` }); return { manifest: prepared.manifest, native: prepared }; }, prepare: async (command, input, r, manage, status, binding) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command, prepared = binding ? binding.native as PreparedAttachmentBinding : null;
            const steps = [...(prepared?.guards ?? []), assertion(write, key, 'version', `EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=? AND t.version=? AND t.status=? AND ${scope})`, [id, input.expectedVersion, r.status, ...args(a)])];
            if (!manage)
                steps.push(assertion(write, key, 'owner', `EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=? AND ${eligible})`, [id]));
            if (input.action === 'close')
                steps.push(assertion(write, key, 'reviewer', `EXISTS(SELECT 1 FROM support_incidents t JOIN (${identity_accessProjectionSources.assignments}) m ON ${reviewer} WHERE t.id=? AND m.user_id=?)`, [id, a.userId]));
            if (input.action === 'link') {
                steps.push(assertion(write, key, 'linked-source', `EXISTS(SELECT 1 FROM support_tickets s JOIN support_incidents t ON s.company_id=t.company_id AND s.branch_id=t.branch_id AND s.department_id IS NOT DISTINCT FROM t.department_id WHERE t.id=? AND s.id=? AND s.version=? AND s.source_kind='sales_order')`, [id, input.ticketId, input.ticketVersion]), assertion(write, key, 'not-linked', 'NOT EXISTS(SELECT 1 FROM support_incident_tickets WHERE incident_id=? AND ticket_id=? AND active)', [id, input.ticketId]), write.prepare('INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at) VALUES(?,?,?,?,?) ON CONFLICT(incident_id,ticket_id) DO UPDATE SET linked_version=excluded.linked_version,linked_by=excluded.linked_by,linked_at=excluded.linked_at,active=true').bind(id, input.ticketId, input.ticketVersion, a.userId, now));
            }
            if (input.action === 'unlink')
                steps.push(assertion(write, key, 'linked', 'EXISTS(SELECT 1 FROM support_incident_tickets WHERE incident_id=? AND ticket_id=? AND active) AND (SELECT COUNT(*) FROM support_incident_tickets WHERE incident_id=? AND active)>1', [id, input.ticketId, id]), write.prepare('UPDATE support_incident_tickets SET active=false WHERE incident_id=? AND ticket_id=?').bind(id, input.ticketId));
            steps.push(write.prepare('UPDATE support_incidents SET status=?,owner_id=?,due_date=?,root_cause=?,mitigation=?,mitigated_by=?,version=version+1,updated_at=? WHERE id=?').bind(status, input.action === 'reassign' ? input.ownerId : r.owner_id, input.action === 'reassign' ? input.dueDate : r.due_date, input.action === 'mitigate' ? input.rootCause : r.root_cause, input.action === 'mitigate' ? input.mitigation : r.mitigation, input.action === 'mitigate' ? a.userId : r.mitigated_by, now, id));
            if (prepared)
                steps.push(...prepared.steps);
            if (['reassign', 'reopen', 'reject'].includes(input.action))
                steps.push(assertion(write, key, 'new-owner', `EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=? AND ${eligible})`, [id]));
            steps.push(write.prepare('INSERT INTO support_incident_events VALUES(?,?,?,?,?,?,?,?::jsonb)').bind(`incident-event-${key}`, id, input.expectedVersion + 1, input.action, a.userId, input.reason, now, JSON.stringify({ ...input, status, ...(prepared ? { attachmentManifest: prepared.manifest } : {}) })), notify(write, id, key));
            return { native: steps };
        } })(a, id, raw, key);
}
export async function incidentWorkspace(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.incidents"); const data = await createIncidentWorkspace({ ...runtime, incidents: async (a) => { const write = await runtime.database(), permission = supportIncidentReadPredicate(a); return (await write.prepare(`SELECT t.* FROM support_incidents t WHERE ${scope} AND ${permission.sql} ORDER BY t.updated_at DESC,t.id LIMIT 200`).bind(...args(a), ...permission.bindings).all<Row>()).results; }, tickets: async (a) => { const write = await runtime.database(); return (await write.prepare(`SELECT t.id,t.title,t.version,t.status FROM support_tickets t WHERE ${scope} AND t.source_kind='sales_order' ORDER BY t.updated_at DESC,t.id LIMIT 200`).bind(...args(a)).all<Row>()).results; } })(a); return { ...data, canManage: data.canManage && access.newWork, tickets: access.newWork ? data.tickets : [] }; }
export async function incidentDetail(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.incidents"); const data = await createIncidentDetail({ incident: getSupportIncident, ticket: getSupportTicket, recipients: (actor, id) => access.settleExisting ? supportRecipients(actor, id) : Promise.resolve([]), links: async (id) => { const write = await runtime.database(); return (await write.prepare('SELECT t.id,t.title,t.status,t.version,l.linked_version FROM support_incident_tickets l JOIN support_tickets t ON t.id=l.ticket_id WHERE l.incident_id=? AND l.active ORDER BY l.linked_at,t.id').bind(id).all<Row>()).results; }, candidates: async (a, row, id) => { const write = await runtime.database(); return (await write.prepare("SELECT id,title,version FROM support_tickets WHERE company_id=? AND branch_id=? AND department_id IS NOT DISTINCT FROM ?::text AND source_kind='sales_order' AND id NOT IN(SELECT ticket_id FROM support_incident_tickets WHERE incident_id=? AND active) ORDER BY updated_at DESC LIMIT 200").bind(a.companyId, row.branch_id, row.department_id, id).all<Row>()).results; }, history: async (id) => { const write = await runtime.database(); return (await write.prepare('SELECT * FROM support_incident_events WHERE incident_id=? ORDER BY version DESC').bind(id).all<Row>()).results; } })(a, id); return { ...data, canManage: data.canManage && access.settleExisting, canWork: data.canWork && access.settleExisting, canReview: data.canReview && access.settleExisting, canReopen: data.canManage && access.newWork }; }
export async function incidentWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.incidents"); if (!access.history) return []; const items = await createIncidentWork({ incidents: async (a) => { const write = await runtime.database(), permission = supportIncidentReadPredicate(a); return (await write.prepare(`SELECT t.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewer} AND m.user_id=?) THEN 1 ELSE 0 END reviewer_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewer}) reviewer_count FROM support_incidents t WHERE ${scope} AND ${permission.sql}`).bind(a.userId, ...args(a), ...permission.bindings).all<Row>()).results; } })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.settleExisting, queue: item.queue === "todo" && !access.settleExisting ? "waiting" as const : item.queue })); }

export function incidentUpdatedNotificationSource(){return `
  SELECT DISTINCT ?||u.user_id,t.company_id,u.user_id,'support_incident',t.id,t.version,'Incident updated: review impact and follow up each linked customer case',t.updated_at
  FROM support_incidents t JOIN LATERAL(SELECT t.owner_id user_id UNION SELECT s.owner_id FROM support_incident_tickets l JOIN support_tickets s ON s.id=l.ticket_id WHERE l.incident_id=t.id AND l.active UNION SELECT m.user_id FROM (${identity_accessProjectionSources.assignments}) m WHERE t.status='mitigated' AND ${reviewer}) u ON true
  WHERE t.id=?`;}
