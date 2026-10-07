import type { WorkContext } from "../../identity_access/public/authorization";
import type { Clock } from "../../shared/application/ports";
import type { Snapshot } from "../domain/pms-model";
import type { PmsHotelOverview } from "../domain/home";
export type PmsFinancialRows<Row> = {
    results: Row[];
};
export type PmsFinancialOverviewRepository = {
    ledger(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        propertyId: string | null;
        propertyId2: string | null;
    }): Promise<PmsFinancialRows<{
        id: string;
        receivable: number;
        advance: number;
        deposit: number;
        refund_payable: number;
        unallocated: number;
        received: number;
        debit: number;
        credit: number;
    }>>;
    liabilities(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        propertyId: string | null;
        propertyId2: string | null;
    }): Promise<PmsFinancialRows<{
        id: string;
        refund_payable: number;
        unallocated: number;
    }>>;
    transfers(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        propertyId: string | null;
        propertyId2: string | null;
    }): Promise<PmsFinancialRows<{
        id: string;
        property_id: string;
        folio_id: string;
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
    }>>;
    exceptions(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        propertyId: string | null;
        propertyId2: string | null;
    }): Promise<PmsFinancialRows<{
        id: string;
        property_id: string;
        folio_id: string;
        description: string;
        kind: string;
        total: number;
    }>>;
    documents(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        propertyId: string | null;
        propertyId2: string | null;
    }): Promise<PmsFinancialRows<{
        id: string;
        property_id: string;
        folio_id: string;
        kind: string;
        finance_id: string | null;
        journal_id: string | null;
        finance_kind: string;
        created_at: number;
    }>>;
    hotel(actor: WorkContext, id: string): Promise<Snapshot>;
    hotels(actor: WorkContext, id?: string): Promise<PmsHotelOverview[]>;
};
export function createPmsFinancialOverview(ports: {
    repository(actor: WorkContext): Promise<PmsFinancialOverviewRepository>;
    clock: Clock;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
}) {
    /** Posted GL, subledger and original evidence remain separate, auditable figures. */
    async function pmsFinanceOverview(actor: WorkContext, propertyId?: string) {
        ports.require(actor, "finance.read");
        ports.require(actor, "pms.reports");
        if (actor.departmentId || actor.beta)
            throw ports.error(403, "PMS_BRANCH_REQUIRED", "Choose a company or branch.");
        const write = await ports.repository(actor);
        const hotels = await write.hotels(actor, propertyId);
        if (propertyId && !hotels.length)
            throw ports.error(404, "PMS_NOT_FOUND", "Hotel not found.");
        const gl = (await write.ledger({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, propertyId: propertyId ?? null, propertyId2: propertyId ?? null })).results;
        const liabilities = (await write.liabilities({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, propertyId: propertyId ?? null, propertyId2: propertyId ?? null })).results;
        const reviews = (await write.transfers({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, propertyId: propertyId ?? null, propertyId2: propertyId ?? null })).results;
        const exceptions = (await write.exceptions({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, propertyId: propertyId ?? null, propertyId2: propertyId ?? null })).results;
        const documents = (await write.documents({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, propertyId: propertyId ?? null, propertyId2: propertyId ?? null })).results;
        return { hotels: hotels.map(hotel => {
                const line = gl.find(row => row.id === hotel.id);
                const ledger = { receivable: Number(line?.receivable ?? 0), advance: Number(line?.advance ?? 0), deposit: Number(line?.deposit ?? 0), received: Number(line?.received ?? 0) };
                const original = liabilities.find(row => row.id === hotel.id);
                const online = { refundPayable: Number(original?.refund_payable ?? 0), unallocated: Number(original?.unallocated ?? 0) };
                const onlineLedger = { refundPayable: Number(line?.refund_payable ?? 0), unallocated: Number(line?.unallocated ?? 0) };
                return { ...hotel, ledger, differences: { receivable: (hotel.receivable ?? 0) - ledger.receivable, advance: (hotel.advance ?? 0) - ledger.advance, deposit: (hotel.deposit ?? 0) - ledger.deposit },
                    online, onlineLedger, onlineDifferences: { refundPayable: online.refundPayable - onlineLedger.refundPayable, unallocated: online.unallocated - onlineLedger.unallocated },
                    unbalanced: Number(line?.debit ?? 0) - Number(line?.credit ?? 0), pendingTransfers: reviews.filter(row => row.property_id === hotel.id && row.state === "pending").length };
            }), reviews, exceptions, documents, updatedAt: ports.clock.now() };
    }
    return { pmsFinanceOverview };
}
