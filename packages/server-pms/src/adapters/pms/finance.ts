import { pmsFinanceFacts } from "@erpx/server-finance/public/pms-command-facts";
import "server-only";
import { holdingResources } from "../../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { loadPms } from "../../../../../packages/server-pms/src/public/pms";
import { pmsFinancialHotelOverview } from "@erpx/server-pms/adapters/pms/home";
import { requirePmsFinancialRead } from "@erpx/server-pms/adapters/property-purchase-boundary";
import { pmsFinancialOverviewFacts } from "@erpx/server-finance/public/pms-finance-facts";
import { createPmsFinancialOverview, type PmsFinancialOverviewRepository } from "@erpx/server-pms/application/finance";
function repository(db: SqlDatabase): PmsFinancialOverviewRepository {
    const financial = pmsFinancialOverviewFacts(db);
    return {
        bankEvidence: (companyId,bank,currency) => pmsFinanceFacts(db).readFinanceBankTransactions({companyId,bank,currency}),
        ledger: facts => financial.ledger(facts),
        liabilities: (facts: {
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
            propertyId: string | null;
            propertyId2: string | null;
        }) => db.prepare(`SELECT p.id,
    COALESCE(sum(CASE WHEN e.data_json::jsonb->>'description'='payment.refund_liability' THEN (e.data_json::jsonb->>'total')::bigint
      WHEN e.data_json::jsonb->>'description'='payment.refund_settle' AND source.data_json::jsonb->>'description'<>'payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint ELSE 0 END),0) refund_payable,
    COALESCE(sum(CASE WHEN e.data_json::jsonb->>'description'='payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint
      WHEN e.data_json::jsonb->>'description'='payment.refund_settle' AND source.data_json::jsonb->>'description'='payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint ELSE 0 END),0) unallocated
    FROM pms_properties p LEFT JOIN pms_records e ON e.property_id=p.id AND e.kind='entry'
    LEFT JOIN pms_records source ON source.property_id=p.id AND source.id=e.data_json::jsonb->>'originalId' AND source.kind='entry'
    WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?) GROUP BY p.id`).bind(facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2).all<{
            id: string;
            refund_payable: number;
            unallocated: number;
        }>(),
        transfers: (facts: {
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
            propertyId: string | null;
            propertyId2: string | null;
        }) => db.prepare(`SELECT t.id,t.property_id,p.version property_version,p.config_json::jsonb->'accounts'->>'bank' bank_account,t.guest_account_id,t.state,t.amount_minor,t.currency,t.version,t.reference,t.recorded_by,
      t.bank_transaction_id,t.entry_id,t.finance_payment_id,t.updated_at
    FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id
    WHERE t.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)
    ORDER BY CASE WHEN t.state='pending' THEN 0 ELSE 1 END,t.updated_at DESC LIMIT 100`).bind(facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2).all<{
            id: string;
            property_id: string;
            property_version: number;
            bank_account: string;
            guest_account_id: string;
            state: string;
            amount_minor: number;
            currency: string;
            version: number;
            reference: string;
            recorded_by: string;
            bank_transaction_id: string | null;
            entry_id: string | null;
            finance_payment_id: string | null;
            updated_at: number;
        }>(),
        exceptions: facts => financial.exceptions(facts),
        documents: facts => financial.documents(facts), hotel: (actor, id) => loadPms(db, actor, id), hotels: (actor, id) => pmsFinancialHotelOverview(db, actor, id)
    };
}
const application = createPmsFinancialOverview({ repository: async (actor) => { const database=(await holdingResources(actor.holdingId)).write; await requirePmsFinancialRead(database,actor,true); return repository(database); }, clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message) });
export const pmsFinanceOverview = application.pmsFinanceOverview;
