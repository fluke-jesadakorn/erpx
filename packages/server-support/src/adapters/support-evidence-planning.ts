import type{SqlDatabase,SqlStatement}from '@erpx/server-runtime/public/runtime/types';
import type{WorkContext}from '@erpx/server-identity_access/public/authorization';
import type{SourceRecordReadServices}from '@erpx/server-records/public/source-record-contract';
import{assertion}from '@erpx/server-workflow/public/command-assertions';
import{nativeSupportReadRuntime}from '@erpx/server-support/adapters/read-runtime';
import{createNativeSupportTicketRead}from '@erpx/server-support/public/support-ticket-read';
import{supportRecipientEligibility}from '@erpx/server-support/adapters/support-recipient';
import{createSupportEvidenceGuard}from '@erpx/server-support/application/support-evidence';
const eligible=supportRecipientEligibility();
export function createNativeSupportEvidencePlanner(sources:SourceRecordReadServices<WorkContext,SqlDatabase,SqlStatement>){const{getSupportTicket}=createNativeSupportTicketRead(sources);
function nativeGuard(a: WorkContext, supplied?: SqlDatabase) { const runtime = nativeSupportReadRuntime(a, supplied); return createSupportEvidenceGuard({ ...runtime, ticket: getSupportTicket, eligible: async (a, id) => { const db = await runtime.database(); return Boolean(await db.prepare(`SELECT 1 AS ok FROM support_tickets t WHERE t.id=? AND ${eligible.replaceAll('t.owner_id', '?::text')}`).bind(id, a.userId).first()); }, guard: async (a, id, expectedVersion, key, manager) => { const db = await runtime.database(); return { native: [assertion(db, key, 'support-evidence', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${manager ? eligible.replaceAll('t.owner_id', '?::text') : eligible + ' AND t.owner_id=?'})`, [id, a.companyId, expectedVersion, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId])] }; } }); }
async function supportEvidenceGuard(db: SqlDatabase, a: WorkContext, id: string, expectedVersion: number, key: string) { const { row, mutation } = await nativeGuard(a, db)(a, id, expectedVersion, key); return { row, guards: mutation.native as SqlStatement[] }; }
function supportEvidenceEvent(db: SqlDatabase, id: string, key: string, actor: string, action: string, evidence: Record<string, unknown>, reason: string) {
    return db.prepare('INSERT INTO support_ticket_events SELECT ?,id,version,?,?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?').bind(`support-evidence-${key}`, action, actor, reason, JSON.stringify(evidence), id);
}
return{nativeGuard,supportEvidenceGuard,supportEvidenceEvent};}
