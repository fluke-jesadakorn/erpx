import{nativeSupportEventHistory,nativeSupportReadAll,SUPPORT_RD_CUSTOMER_CONTACT_NOTES,SUPPORT_RD_SLA_POLICY_APPROVED,SUPPORT_RD_TICKET_INCIDENT_LINKS,SUPPORT_RD_TICKET_RECIPIENTS,SUPPORT_RD_TICKET_WORK}from '@erpx/support-server/adapters/native-support-read';
import{createNativeSupportTicketRead,supportTicketReadPredicate}from '@erpx/support-server/public/support-ticket-read';
import{args}from '@erpx/support-server/adapters/support-ticket-read-sources';
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/people-payroll-server/public/final-root-owner-projection-sources";
import 'server-only';
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type {Row} from '@erpx/commerce-server/public/commerce';

import { readSourceRecord, visibleSourceRecord, sourceRecordGuards } from "@erpx/records-server/public/source-record-policy";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeSupportRuntime } from "@erpx/support-server/adapters/command-runtime";
import { createSupportDetail, createSupportWork, createSupportRecipients } from "@erpx/support-server/application/support";
export { supportCreateSchema, supportChangeSchema } from "@erpx/support-server/domain/support";

/** Purchase metadata only; callers retain their own authority and record gates. */
export function supportWorkspacePurchaseAccess(a: WorkContext, provided?: SqlDatabase) { return nativeSupportRuntime(a, provided).purchase("support.tickets"); }
export async function supportTicketRecipients(a: WorkContext, ticketId: string) { const runtime = nativeSupportRuntime(a); await runtime.requirePurchase("support.tickets", "settle-existing"); return createSupportRecipients({ ...runtime, recipients: async (a, ticketId) => { const write = await runtime.database(); return (await nativeSupportReadAll<{ id: string; name: string }>(SUPPORT_RD_TICKET_RECIPIENTS, [ticketId, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId], write)).results; } })(a, ticketId); }
/** Internal incident adapter shares the native ticket recipient reader; no separate order lookup remains. */
export { supportTicketRecipients as supportRecipients };
export async function supportDetail(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.tickets"); const data = await createSupportDetail({ ticket: getSupportTicket, now: runtime.now, recipients: (actor, id) => access.settleExisting ? supportTicketRecipients(actor, id) : Promise.resolve([]), incidents: async (id) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_TICKET_INCIDENT_LINKS, [id], write)).results; }, history: (id) => nativeSupportEventHistory<Row>(a, 'ticket', id), notes: async (a, orderId) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_CUSTOMER_CONTACT_NOTES, [orderId, a.companyId], write)).results; }, policies: async (companyId) => { const write = await runtime.database(); return (await nativeSupportReadAll<{
        id: string;
        name: string;
        version: number;
    }>(SUPPORT_RD_SLA_POLICY_APPROVED, [companyId], write)).results; } })(a, id); return { ...data, managedInFieldService: data.row.source_kind==='field_service', canWork: data.row.source_kind!=='field_service' && data.canWork && access.settleExisting, canManage: data.row.source_kind!=='field_service' && data.canManage && access.settleExisting, canReport: data.row.source_kind!=='field_service' && data.canReport && access.settleExisting, canConfirm: data.row.source_kind!=='field_service' && data.canConfirm && access.settleExisting, canReopen: data.row.source_kind!=='field_service' && data.canManage && access.newWork, slaPolicies: data.row.source_kind!=='field_service' && access.newWork ? data.slaPolicies : [] }; }
export async function supportWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.tickets"); if (!access.history) return []; const items = await createSupportWork({ tickets: async (a) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_TICKET_WORK, [...args(a), ...supportTicketReadPredicate(a).bindings], write)).results; }, sourceVisible })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.settleExisting, queue: item.queue === "todo" && !access.settleExisting ? "waiting" as const : item.queue })); }

const{getSupportTicket,supportTicketSourceGuards,sourceVisible}=createNativeSupportTicketRead({read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards});
export{getSupportTicket,supportTicketSourceGuards};
