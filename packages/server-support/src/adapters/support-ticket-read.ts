
import 'server-only';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {assertion} from "@erpx/server-workflow/public/command-assertions";

import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";

import { createGetSupportTicket, createSupportTicketSourceGuards } from "@erpx/server-support/application/support";

import{nativeSupportReadRuntime}from '@erpx/server-support/adapters/read-runtime';
import{nativeSupportReadFirst,SUPPORT_RD_TICKET_BY_ID}from '@erpx/server-support/adapters/native-support-read';
import { eligibilitySelect, supportAssignmentAccessSql } from './eligibility-projection-sources';
import { ticketGuardExistsSql } from './ticket-guard-sources';
import{args}from '@erpx/server-support/adapters/support-ticket-read-sources';
import type{SourceRecordReadServices}from '@erpx/server-records/public/source-record-contract';
/** Canonical owner-local eligibility projection for the Support ticket work list.
 * The by-id reader is native; this prefix is shared so the rendered SQL stays
 * byte-identical for the remaining work-list caller. */
export const ticketEligibilitySelect = (ownerEligible: string, reviewerCount: string, extra = '') =>
    eligibilitySelect(`${extra}CASE WHEN ${ownerEligible} THEN 1 ELSE 0 END owner_eligible,${reviewerCount} reviewer_count`);
export function supportTicketReadPredicate(a: WorkContext, alias = 't') {
    return { sql: supportAssignmentAccessSql(alias, '   ', `   AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id)
   AND (iam_role_allows(g.role,g.company_id,'support.read') OR ${alias}.source_kind<>'sales_order' AND ${alias}.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read'))`), bindings: [a.assignmentId, a.userId, a.role] };
}
export function createNativeSupportTicketRead(sources:SourceRecordReadServices<WorkContext,SqlDatabase,SqlStatement>){const{read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards}=sources;
async function sourceVisible(a: WorkContext, row: Row, provided?:SqlDatabase):Promise<boolean> { if (!row.source_record_json)
    return true; const input = JSON.parse(String(row.source_record_json)); return Boolean(await visibleSourceRecord(a, { kind: input.kind, id: input.id },provided)); }
async function supportTicketSourceGuards(db: SqlDatabase, a: WorkContext, key: string, id: string, version: number, step: string):Promise<SqlStatement[]> { const mutation = await createSupportTicketSourceGuards({ ticket: (actor,id)=>getSupportTicket(actor,id,db), current: async (a, id, version, key, step) => { const access = supportTicketReadPredicate(a); return { native: [db.prepare('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE').bind(id, a.companyId), assertion(db, key, step, ticketGuardExistsSql(`t.version=? AND t.company_id=? AND ${access.sql}`), [id, version, a.companyId, ...access.bindings])] }; }, source: async (a, input) => readSourceRecord(a, input, db), origin: async (a, key, source, step) => ({ native: await sourceRecordGuards(db, a, key, source, step) }), combine: (current, origin) => ({ native: [...current.native as SqlStatement[], ...origin.native as SqlStatement[]] }) })(a, key, id, version, step); return mutation.native as SqlStatement[]; }
async function getSupportTicket(a: WorkContext, id: string, provided?:SqlDatabase):Promise<Row> { const runtime = nativeSupportReadRuntime(a,provided); await runtime.requirePurchase("support.tickets"); return createGetSupportTicket({ ...runtime, ticket: async (a, id) => { const write = await runtime.database(); return nativeSupportReadFirst<Row>(SUPPORT_RD_TICKET_BY_ID, [id, ...args(a), ...supportTicketReadPredicate(a).bindings], write); }, sourceVisible:(a,row)=>sourceVisible(a,row,provided) })(a, id); }
return{getSupportTicket,supportTicketSourceGuards,sourceVisible};}
