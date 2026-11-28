import{getSupportIncident,supportIncidentReadPredicate}from '@erpx/support-server/public/support-incident-read';
import { readTailR43ColumnsFromSql } from './read-tail-r43-sql';
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type {Row} from '@erpx/commerce-server/public/commerce';
import {getSupportTicket,supportRecipients} from '@erpx/support-server/adapters/support';
import { nativeSupportRuntime } from "@erpx/support-server/adapters/command-runtime";
import { nativeSupportEventHistory, nativeSupportReadAll, SUPPORT_RD_INCIDENT_TICKET_LINKS, SUPPORT_RD_INCIDENT_TICKET_CANDIDATES, SUPPORT_RD_INCIDENT_WORK } from "@erpx/support-server/adapters/native-support-read";
import { createIncidentWorkspace, createIncidentDetail, createIncidentWork } from "@erpx/support-server/application/support-incidents";
export { incidentCreateSchema, incidentChangeSchema } from "@erpx/support-server/domain/support-incidents";
const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
/** One parameterized owner-local workspace list family: the incident and ticket
 * workspace readers differ only by projected columns, table and trailing
 * predicate. Rendering stays byte-identical and the native read ports remain
 * the authority. */
const scopedWorkspaceListSql = (columns: string, table: string, extra: string) =>
    readTailR43ColumnsFromSql(columns, `${table} t WHERE ${scope} AND ${extra} ORDER BY t.updated_at DESC,t.id LIMIT 200`);

export async function incidentWorkspace(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.incidents"); const data = await createIncidentWorkspace({ ...runtime, incidents: async (a) => { const write = await runtime.database(), permission = supportIncidentReadPredicate(a); return (await write.prepare(scopedWorkspaceListSql('t.*','support_incidents',permission.sql)).bind(...args(a), ...permission.bindings).all<Row>()).results; }, tickets: async (a) => { const write = await runtime.database(); return (await write.prepare(scopedWorkspaceListSql('t.id,t.title,t.version,t.status','support_tickets',"t.source_kind='sales_order'")).bind(...args(a)).all<Row>()).results; } })(a); return { ...data, canManage: data.canManage && access.newWork, tickets: access.newWork ? data.tickets : [] }; }
export async function incidentDetail(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.incidents"); const data = await createIncidentDetail({ incident: getSupportIncident, ticket: getSupportTicket, recipients: (actor, id) => access.settleExisting ? supportRecipients(actor, id) : Promise.resolve([]), links: async (id) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_INCIDENT_TICKET_LINKS, [id], write)).results; }, candidates: async (a, row, id) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_INCIDENT_TICKET_CANDIDATES, [a.companyId, row.branch_id, row.department_id, id], write)).results; }, history: (id) => nativeSupportEventHistory<Row>(a, 'incident', id) })(a, id); return { ...data, canManage: data.canManage && access.settleExisting, canWork: data.canWork && access.settleExisting, canReview: data.canReview && access.settleExisting, canReopen: data.canManage && access.newWork }; }
export async function incidentWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.incidents"); if (!access.history) return []; const items = await createIncidentWork({ incidents: async (a) => { const write = await runtime.database(), permission = supportIncidentReadPredicate(a); return (await nativeSupportReadAll<Row>(SUPPORT_RD_INCIDENT_WORK, [a.userId, ...args(a), ...permission.bindings], write)).results; } })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.settleExisting, queue: item.queue === "todo" && !access.settleExisting ? "waiting" as const : item.queue })); }
