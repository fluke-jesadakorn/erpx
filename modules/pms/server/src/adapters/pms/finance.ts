import { pmsFinanceFacts } from "@erpx/finance-server/public/pms-command-facts";
import "server-only";
import { holdingResources } from "../../../../../../platform/runtime/server/src/public/runtime";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { loadPms } from "../../public/pms";
import { pmsFinancialHotelOverview } from "@erpx/pms-server/adapters/pms/home";
import { requirePmsFinancialRead } from "@erpx/pms-server/adapters/property-purchase-boundary";
import { pmsFinancialOverviewFacts } from "@erpx/finance-server/public/pms-finance-facts";
import { createPmsFinancialOverview, type PmsFinancialOverviewRepository } from "@erpx/pms-server/application/finance";
import { pmsPropertyScopeSql } from "../pms-read-tail-r41-sql";
import { pmsRecordPropertyJoinSql } from "../pms-read-tail-r45-sql";
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
        }) => db.prepare(pmsPropertyScopeSql('liabilities')).bind(facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2).all<{
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
        }) => db.prepare(`${pmsRecordPropertyJoinSql('transferList')}
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
