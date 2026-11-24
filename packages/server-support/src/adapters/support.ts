import{nativeSupportReadAll,SUPPORT_RD_TICKET_EVENT_HISTORY,SUPPORT_RD_TICKET_INCIDENT_LINKS}from '@erpx/server-support/adapters/native-support-read';
import{createNativeSupportTicketRead,supportTicketReadPredicate,ticketEligibilitySelect}from '@erpx/server-support/public/support-ticket-read';
import{scope,args,eligible,reviewers}from '@erpx/server-support/adapters/support-ticket-read-sources';
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as salesProjectionSources } from "@erpx/server-sales/public/final-root-owner-projection-sources";
import 'server-only';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';

import { readSourceRecord, visibleSourceRecord, sourceRecordGuards } from "@erpx/server-records/public/source-record-policy";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { createSupportDetail, createSupportWork, createSupportRecipients } from "@erpx/server-support/application/support";
import { supportPolicySelect } from "./support-policy-read-sources";
export { supportCreateSchema, supportChangeSchema } from "@erpx/server-support/domain/support";

/** Purchase metadata only; callers retain their own authority and record gates. */
export function supportWorkspacePurchaseAccess(a: WorkContext, provided?: SqlDatabase) { return nativeSupportRuntime(a, provided).purchase("support.tickets"); }
export async function supportTicketRecipients(a: WorkContext, ticketId: string) { const runtime = nativeSupportRuntime(a); await runtime.requirePurchase("support.tickets", "settle-existing"); return createSupportRecipients({ ...runtime, recipients: async (a, ticketId) => { const write = await runtime.database(); return (await write.prepare(`SELECT DISTINCT m.user_id id,p.legal_name name FROM support_tickets t JOIN (${identity_accessProjectionSources.assignments}) m ON m.company_id=t.company_id JOIN (${people_payrollProjectionSources.hr_people}) p ON p.company_id=t.company_id AND p.user_id=m.user_id WHERE t.id=? AND ${scope} AND ${eligible.replaceAll('t.owner_id', 'm.user_id')} ORDER BY p.legal_name`).bind(ticketId, ...args(a)).all<{
        id: string;
        name: string;
    }>()).results; } })(a, ticketId); }
/** Internal incident adapter shares the native ticket recipient reader; no separate order lookup remains. */
export { supportTicketRecipients as supportRecipients };
export async function supportDetail(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.tickets"); const data = await createSupportDetail({ ticket: getSupportTicket, now: runtime.now, recipients: (actor, id) => access.settleExisting ? supportTicketRecipients(actor, id) : Promise.resolve([]), incidents: async (id) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_TICKET_INCIDENT_LINKS, [id], write)).results; }, history: async (id) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_TICKET_EVENT_HISTORY, [id], write)).results; }, notes: async (a, orderId) => { const write = await runtime.database(); return (await write.prepare(`SELECT id,version,channel,occurred_at,body FROM (${salesProjectionSources.customer_contact_notes}) customer_contact_notes WHERE order_id=? AND company_id=? AND status='active' ORDER BY occurred_at DESC`).bind(orderId, a.companyId).all<Row>()).results; }, policies: async (companyId) => { const write = await runtime.database(); return (await write.prepare(supportPolicySelect('id,name,version', "company_id=? AND status='approved'", ' ORDER BY name')).bind(companyId).all<{
        id: string;
        name: string;
        version: number;
    }>()).results; } })(a, id); return { ...data, managedInFieldService: data.row.source_kind==='field_service', canWork: data.row.source_kind!=='field_service' && data.canWork && access.settleExisting, canManage: data.row.source_kind!=='field_service' && data.canManage && access.settleExisting, canReport: data.row.source_kind!=='field_service' && data.canReport && access.settleExisting, canConfirm: data.row.source_kind!=='field_service' && data.canConfirm && access.settleExisting, canReopen: data.row.source_kind!=='field_service' && data.canManage && access.newWork, slaPolicies: data.row.source_kind!=='field_service' && access.newWork ? data.slaPolicies : [] }; }
export async function supportWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.tickets"); if (!access.history) return []; const items = await createSupportWork({ tickets: async (a) => { const write = await runtime.database(), access = supportTicketReadPredicate(a); return (await write.prepare(`${ticketEligibilitySelect(eligible, reviewers)} FROM support_tickets t WHERE ${scope} AND ${access.sql} AND t.source_kind<>'field_service' ORDER BY t.updated_at DESC`).bind(...args(a), ...access.bindings).all<Row>()).results; }, sourceVisible })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.settleExisting, queue: item.queue === "todo" && !access.settleExisting ? "waiting" as const : item.queue })); }

const{getSupportTicket,supportTicketSourceGuards,sourceVisible}=createNativeSupportTicketRead({read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards});
export{getSupportTicket,supportTicketSourceGuards};
