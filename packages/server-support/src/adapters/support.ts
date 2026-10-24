import{createNativeSupportTicketRead,supportTicketReadPredicate}from '@erpx/server-support/public/support-ticket-read';
import{scope,args,eligible,reviewers}from '@erpx/server-support/adapters/support-ticket-read-sources';
import { finalRootOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as salesProjectionSources } from "@erpx/server-sales/public/final-root-owner-projection-sources";
import { supportNotificationEffects } from "@erpx/server-collaboration/public/support-notification-effects";
import 'server-only';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {assertion} from "@erpx/server-workflow/public/command-assertions";

import { readSourceRecord, visibleSourceRecord, sourceRecordGuards } from "@erpx/server-records/public/source-record-policy";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { createSupportWorkspace, createSupportDetail, createSupportTicket as createSupportTicketUsecase, createChangeSupportTicket, createSupportWork, createSupportRecipients } from "@erpx/server-support/application/support";
import type { SupportCommand, SupportRow as RowType } from "@erpx/server-support/application/command";
import type { SupportClock } from "@erpx/server-support/domain/support-clock";
import type { SupportCreate, SupportChange } from "@erpx/server-support/domain/support";
import type { AuthorizedSourceRecord } from "@erpx/server-records/public/source-record-policy";
export { supportCreateSchema, supportChangeSchema } from "@erpx/server-support/domain/support";

/** Purchase metadata only; callers retain their own authority and record gates. */
export function supportWorkspacePurchaseAccess(a: WorkContext, provided?: SqlDatabase) { return nativeSupportRuntime(a, provided).purchase("support.tickets"); }
export async function supportWorkspace(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.tickets"); const data = await createSupportWorkspace({ ...runtime, tickets: async (a) => { const write = await runtime.database(), access = supportTicketReadPredicate(a); return (await write.prepare(`SELECT t.id,t.title,t.status,t.due_date,t.owner_id,t.version,t.updated_at,t.source_kind,t.source_record_json FROM support_tickets t WHERE ${scope} AND ${access.sql} ORDER BY t.updated_at DESC,t.id LIMIT 200`).bind(...args(a), ...access.bindings).all<Row>()).results; }, sourceVisible, orderSources: async (a) => { const write = await runtime.database(); return (await write.prepare(`SELECT t.id,t.number,t.version,p.name customer_name FROM (${commerceProjectionSources.commerce_orders}) t JOIN (${financeProjectionSources.finance_parties}) p ON p.id=t.party_id WHERE ${scope} AND t.kind IN ('sales','quote') AND p.kind='customer' ORDER BY t.updated_at DESC LIMIT 200`).bind(...args(a)).all<Row>()).results; } })(a); return { ...data, canCreate: data.canCreate && access.newWork, canCreateOrder: data.canCreateOrder && access.newWork, sources: access.newWork ? data.sources : [] }; }
export async function supportRecipients(a: WorkContext, orderId: string) { const runtime = nativeSupportRuntime(a); await runtime.requirePurchase("support.tickets", "settle-existing"); return createSupportRecipients({ ...runtime, recipients: async (a, orderId) => { const write = await runtime.database(); return (await write.prepare(`SELECT DISTINCT t.owner_id id,p.legal_name name FROM(SELECT o.company_id,o.branch_id,o.department_id,a.user_id owner_id FROM (${commerceProjectionSources.commerce_orders}) o JOIN (${identity_accessProjectionSources.assignments}) a ON a.company_id=o.company_id WHERE o.id=?) t JOIN (${people_payrollProjectionSources.hr_people}) p ON p.company_id=t.company_id AND p.user_id=t.owner_id WHERE ${scope} AND ${eligible} ORDER BY p.legal_name`).bind(orderId, ...args(a)).all<{
        id: string;
        name: string;
    }>()).results; } })(a, orderId); }
export async function supportTicketRecipients(a: WorkContext, ticketId: string) { const runtime = nativeSupportRuntime(a); await runtime.requirePurchase("support.tickets", "settle-existing"); return createSupportRecipients({ ...runtime, recipients: async (a, ticketId) => { const write = await runtime.database(); return (await write.prepare(`SELECT DISTINCT m.user_id id,p.legal_name name FROM support_tickets t JOIN (${identity_accessProjectionSources.assignments}) m ON m.company_id=t.company_id JOIN (${people_payrollProjectionSources.hr_people}) p ON p.company_id=t.company_id AND p.user_id=m.user_id WHERE t.id=? AND ${scope} AND ${eligible.replaceAll('t.owner_id', 'm.user_id')} ORDER BY p.legal_name`).bind(ticketId, ...args(a)).all<{
        id: string;
        name: string;
    }>()).results; } })(a, ticketId); }
export async function supportDetail(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.tickets"); const data = await createSupportDetail({ ticket: getSupportTicket, now: runtime.now, recipients: (actor, id) => access.settleExisting ? supportTicketRecipients(actor, id) : Promise.resolve([]), incidents: async (id) => { const write = await runtime.database(); return (await write.prepare('SELECT i.id,i.title,i.status FROM support_incident_tickets l JOIN support_incidents i ON i.id=l.incident_id WHERE l.ticket_id=? AND l.active ORDER BY i.updated_at DESC').bind(id).all<Row>()).results; }, history: async (id) => { const write = await runtime.database(); return (await write.prepare('SELECT * FROM support_ticket_events WHERE ticket_id=? ORDER BY version DESC').bind(id).all<Row>()).results; }, notes: async (a, orderId) => { const write = await runtime.database(); return (await write.prepare(`SELECT id,version,channel,occurred_at,body FROM (${salesProjectionSources.customer_contact_notes}) customer_contact_notes WHERE order_id=? AND company_id=? AND status='active' ORDER BY occurred_at DESC`).bind(orderId, a.companyId).all<Row>()).results; }, policies: async (companyId) => { const write = await runtime.database(); return (await write.prepare("SELECT id,name,version FROM support_sla_policies WHERE company_id=? AND status='approved' ORDER BY name").bind(companyId).all<{
        id: string;
        name: string;
        version: number;
    }>()).results; } })(a, id); return { ...data, managedInFieldService: data.row.source_kind==='field_service', canWork: data.row.source_kind!=='field_service' && data.canWork && access.settleExisting, canManage: data.row.source_kind!=='field_service' && data.canManage && access.settleExisting, canReport: data.row.source_kind!=='field_service' && data.canReport && access.settleExisting, canConfirm: data.row.source_kind!=='field_service' && data.canConfirm && access.settleExisting, canReopen: data.row.source_kind!=='field_service' && data.canManage && access.newWork, slaPolicies: data.row.source_kind!=='field_service' && access.newWork ? data.slaPolicies : [] }; }
export async function createSupportTicket(a: WorkContext, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createSupportTicketUsecase({ ...runtime, ticket: getSupportTicket, source: async (a, input) => readSourceRecord(a, input, await runtime.database()), report: async (command: SupportCommand, input: Extract<SupportCreate, {
            kind: string;
        }>, source: AuthorizedSourceRecord | null) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            return { native: [
                    ...(source ? await sourceRecordGuards(write, a, key, source) : []),
                    write.prepare("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,created_by,created_at,updated_at,source_record_json) VALUES(?,?,?,?,NULL,NULL,?,?,NULL,NULL,'triage',?,?,?,?,?)")
                        .bind(id, a.companyId, source?.branchId ?? a.branchId, source?.departmentId ?? a.departmentId, input.title, input.issue, source?.kind ?? 'general', a.userId, now, now, source ? JSON.stringify(source) : null),
                    write.prepare("INSERT INTO support_ticket_events SELECT ?,id,version,'report',?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?")
                        .bind(`ticket-event-${key}`, a.userId, input.issue, JSON.stringify(input), id),
                    supportNotificationEffects(write).ticketReopened().bind(`ticket-triage-${key}-`, id),
                ] };
        }, order: async (command: SupportCommand, input: Extract<SupportCreate, {
            orderId: string;
        }>) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            return { native: [
                    assertion(write, key, 'source', `EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_orders}) t JOIN (${financeProjectionSources.finance_parties}) p ON p.id=t.party_id AND p.company_id=t.company_id WHERE t.id=? AND t.version=? AND t.kind IN ('sales','quote') AND p.kind='customer' AND ${scope})`, [input.orderId, input.sourceVersion, ...args(a)]),
                    write.prepare(`INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at) SELECT ?,company_id,branch_id,department_id,id,party_id,?,?,?,?,?,?,? FROM (${commerceProjectionSources.commerce_orders}) commerce_orders WHERE id=?`).bind(id, input.title, input.issue, input.ownerId, input.dueDate, a.userId, now, now, input.orderId),
                    assertion(write, key, 'owner', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND ${eligible})`, [id]),
                    write.prepare("INSERT INTO support_ticket_events SELECT ?,id,version,'create',?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?").bind(`ticket-event-${key}`, a.userId, input.issue, JSON.stringify(input), id),
                    supportNotificationEffects(write).ticketCreated().bind(`ticket-notice-${key}`, id),
                ] };
        } })(a, raw, key);
}
export async function changeSupportTicket(a: WorkContext, id: string, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createChangeSupportTicket({ ...runtime, ticket: getSupportTicket, report: async (command: SupportCommand, input: SupportChange, r: RowType) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            const access = supportTicketReadPredicate(a), steps = [assertion(write, key, 'own-report', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.company_id=? AND t.version=? AND t.created_by=? AND t.status=? AND ${access.sql})`, [id, a.companyId, input.expectedVersion, a.userId, r.status, ...access.bindings])];
            if (input.action === 'edit-report')
                steps.push(write.prepare('UPDATE support_tickets SET title=?,issue=?,version=version+1,updated_at=? WHERE id=?').bind(input.title, input.issue, now, id));
            else
                steps.push(write.prepare("UPDATE support_tickets SET status=CASE WHEN ?='withdraw-report' THEN 'cancelled' ELSE status END,version=version+1,updated_at=? WHERE id=?").bind(input.action, now, id));
            if (r.source_record_json) {
                const original = JSON.parse(String(r.source_record_json));
                steps.push(...await sourceRecordGuards(write, a, key, await readSourceRecord(a, { kind: original.kind, id: original.id }, write)));
            }
            steps.push(write.prepare('INSERT INTO support_ticket_events SELECT ?,id,version,?,?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?').bind(`ticket-event-${key}`, input.action, a.userId, input.reason, JSON.stringify(input), id));
            return { native: steps };
        }, assigned: async (command: SupportCommand, input: SupportChange, r: RowType, manage: boolean, status: string, nextClock: SupportClock | null) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            const steps = [assertion(write, key, 'version', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.version=? AND t.status=? AND ${scope})`, [id, input.expectedVersion, r.status, ...args(a)])];
            if (r.source_record_json) {
                const original = JSON.parse(String(r.source_record_json));
                steps.push(...await sourceRecordGuards(write, a, key, await readSourceRecord(a, { kind: original.kind, id: original.id }, write)));
            }
            if (!manage)
                steps.push(assertion(write, key, 'owner', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.owner_id=? AND ${eligible})`, [id, a.userId]));
            if (input.action === 'confirm')
                steps.push(assertion(write, key, 'customer-confirmation', `EXISTS(SELECT 1 FROM (${salesProjectionSources.customer_contact_notes}) customer_contact_notes WHERE id=? AND version=? AND order_id=? AND company_id=? AND customer_id=? AND status='active' AND occurred_at>=? AND created_at>=?)`, [input.noteId, input.noteVersion, r.order_id, a.companyId, r.customer_id, r.resolved_at, r.resolved_at]));
            const resetting = ['reopen', 'reject-resolution'].includes(input.action);
            steps.push(write.prepare('UPDATE support_tickets SET status=?,owner_id=?,due_date=?,blocked_reason=?,resolution=?,resolved_by=?,resolved_at=?,confirmation_note_id=?,confirmation_note_version=?,confirmed_by=?,confirmed_at=?,confirmation_reason=?,version=version+1,updated_at=? WHERE id=?').bind(status, 'ownerId' in input ? input.ownerId : r.owner_id, 'dueDate' in input ? input.dueDate : r.due_date, status === 'waiting_customer' ? (input.action === 'reassign' ? r.blocked_reason : input.reason) : null, input.action === 'resolve' ? input.reason : resetting ? null : r.resolution, input.action === 'resolve' ? a.userId : resetting ? null : r.resolved_by, input.action === 'resolve' ? now : resetting ? null : r.resolved_at, input.action === 'confirm' ? input.noteId : input.action === 'reopen' ? null : r.confirmation_note_id, input.action === 'confirm' ? input.noteVersion : input.action === 'reopen' ? null : r.confirmation_note_version, input.action === 'confirm-general' ? a.userId : input.action === 'reopen' ? null : r.confirmed_by, input.action === 'confirm-general' ? now : input.action === 'reopen' ? null : r.confirmed_at, input.action === 'confirm-general' ? input.reason : input.action === 'reopen' ? null : r.confirmation_reason, now, id));
            if (['triage', 'reassign', 'reopen', 'reject-resolution'].includes(input.action))
                steps.push(assertion(write, key, 'recipient', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND ${eligible})`, [id]));
            if (nextClock)
                steps.push(write.prepare('UPDATE support_tickets SET sla_clock_json=?::jsonb WHERE id=?').bind(JSON.stringify(nextClock), id));
            steps.push(write.prepare('INSERT INTO support_ticket_events SELECT ?,id,version,?,?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?').bind(`ticket-event-${key}`, input.action, a.userId, input.reason, JSON.stringify({ ...input, ...(nextClock ? { clock: nextClock } : {}) }), id));
            if (input.action !== 'cancel')
                steps.push(supportNotificationEffects(write).ticketUpdated().bind(`ticket-notice-${key}`, id));
            if (input.action === 'resolve')
                steps.push(supportNotificationEffects(write).ticketReviewers().bind(`ticket-review-${key}-`, id));
            return { native: steps };
        } })(a, id, raw, key);
}
export async function supportWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.tickets"); if (!access.history) return []; const items = await createSupportWork({ tickets: async (a) => { const write = await runtime.database(), access = supportTicketReadPredicate(a); return (await write.prepare(`SELECT t.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,${reviewers} reviewer_count FROM support_tickets t WHERE ${scope} AND ${access.sql} AND t.source_kind<>'field_service' ORDER BY t.updated_at DESC`).bind(...args(a), ...access.bindings).all<Row>()).results; }, sourceVisible })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.settleExisting, queue: item.queue === "todo" && !access.settleExisting ? "waiting" as const : item.queue })); }

const{getSupportTicket,supportTicketSourceGuards,sourceVisible}=createNativeSupportTicketRead({read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards});
export{getSupportTicket,supportTicketSourceGuards};
