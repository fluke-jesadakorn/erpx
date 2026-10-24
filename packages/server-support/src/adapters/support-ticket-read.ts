import { finalRootOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";

import 'server-only';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {assertion} from "@erpx/server-workflow/public/command-assertions";

import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";

import { createGetSupportTicket, createSupportTicketSourceGuards } from "@erpx/server-support/application/support";

import{nativeSupportReadRuntime}from '@erpx/server-support/adapters/read-runtime';
import{scope,args,eligible,reviewers}from '@erpx/server-support/adapters/support-ticket-read-sources';
import type{SourceRecordReadServices}from '@erpx/server-records/public/source-record-contract';
export function supportTicketReadPredicate(a: WorkContext, alias = 't') {
    return { sql: `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
   AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
   AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id)
   AND (iam_role_allows(g.role,g.company_id,'support.read') OR ${alias}.source_kind<>'sales_order' AND ${alias}.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read')))`, bindings: [a.assignmentId, a.userId, a.role] };
}
export function createNativeSupportTicketRead(sources:SourceRecordReadServices<WorkContext,SqlDatabase,SqlStatement>){const{read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards}=sources;
async function sourceVisible(a: WorkContext, row: Row, provided?:SqlDatabase):Promise<boolean> { if (!row.source_record_json)
    return true; const input = JSON.parse(String(row.source_record_json)); return Boolean(await visibleSourceRecord(a, { kind: input.kind, id: input.id },provided)); }
async function supportTicketSourceGuards(db: SqlDatabase, a: WorkContext, key: string, id: string, version: number, step: string):Promise<SqlStatement[]> { const mutation = await createSupportTicketSourceGuards({ ticket: (actor,id)=>getSupportTicket(actor,id,db), current: async (a, id, version, key, step) => { const access = supportTicketReadPredicate(a); return { native: [db.prepare('SELECT id FROM support_tickets WHERE id=? AND company_id=? FOR SHARE').bind(id, a.companyId), assertion(db, key, step, `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.version=? AND t.company_id=? AND ${access.sql})`, [id, version, a.companyId, ...access.bindings])] }; }, source: async (a, input) => readSourceRecord(a, input, db), origin: async (a, key, source, step) => ({ native: await sourceRecordGuards(db, a, key, source, step) }), combine: (current, origin) => ({ native: [...current.native as SqlStatement[], ...origin.native as SqlStatement[]] }) })(a, key, id, version, step); return mutation.native as SqlStatement[]; }
async function getSupportTicket(a: WorkContext, id: string, provided?:SqlDatabase):Promise<Row> { const runtime = nativeSupportReadRuntime(a,provided); await runtime.requirePurchase("support.tickets"); return createGetSupportTicket({ ...runtime, ticket: async (a, id) => { const write = await runtime.database(), access = supportTicketReadPredicate(a); return await write.prepare(`SELECT t.*,o.number order_number,p.name customer_name,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=t.company_id AND user_id=t.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,${reviewers} reviewer_count FROM support_tickets t LEFT JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=t.order_id AND o.company_id=t.company_id LEFT JOIN (${financeProjectionSources.finance_parties}) p ON p.id=t.customer_id AND p.company_id=t.company_id WHERE t.id=? AND ${scope} AND ${access.sql}`).bind(id, ...args(a), ...access.bindings).first<Row>(); }, sourceVisible:(a,row)=>sourceVisible(a,row,provided) })(a, id); }
return{getSupportTicket,supportTicketSourceGuards,sourceVisible};}
