export type Count = {
    n: number;
};
export type Totals = {
    direct_total: number;
    room_total: number;
    cash_received: number;
    transfer_verified: number;
};
export type HospitalityDayControlRepository = {
    openShifts(facts: {
        scope: [
            string,
            string
        ];
    }): Promise<number>;
    unreviewedVariances(facts: {
        scope: [
            string,
            string
        ];
    }): Promise<number>;
    openBills(facts: {
        scope: [
            string,
            string
        ];
        businessDate: string;
    }): Promise<number>;
    pendingPayments(facts: {
        scope: [
            string,
            string
        ];
        businessDate: string;
    }): Promise<number>;
    pendingHotelTransfers(facts: {
        companyId: string;
        propertyId: string;
        businessDate: string;
    }): Promise<number>;
    pendingChargeCorrections(facts: {
        companyId: string;
        propertyId: string;
    }): Promise<number>;
    property(facts: {
        propertyId: string;
        companyId: string;
    }): Promise<{
        config_json: string;
    } | null>;
    unpostedBookingReceipts(facts: {
        propertyId: string;
        companyId: string;
        timezone: string;
        businessDate: string;
    }): Promise<number>;
    failedBookingRefunds(facts: {
        propertyId: string;
        companyId: string;
        timezone: string;
        businessDate: string;
    }): Promise<number>;
    unpostedCounterReceipts(facts: {
        propertyId: string;
        companyId: string;
        timezone: string;
        businessDate: string;
    }): Promise<number>;
    failedCounterRefunds(facts: {
        propertyId: string;
        companyId: string;
        timezone: string;
        businessDate: string;
    }): Promise<number>;
    unresolvedRefunds(facts: {
        scope: [
            string,
            string
        ];
        businessDate: string;
    }): Promise<number>;
    failedJobs(facts: {
        companyId: string;
    }): Promise<number>;
    missingRoomCharges(facts: {
        scope: [
            string,
            string
        ];
        businessDate: string;
        propertyId: string;
    }): Promise<number>;
    missingDirectInvoices(facts: {
        scope: [
            string,
            string
        ];
        businessDate: string;
    }): Promise<number>;
    unmatchedVerifiedTransfers(facts: {
        scope: [
            string,
            string
        ];
        businessDate: string;
    }): Promise<number>;
    unmatchedVerifiedHotelTransfers(facts: {
        companyId: string;
        propertyId: string;
        businessDate: string;
    }): Promise<number>;
    totals(facts: {
        scope: [
            string,
            string
        ];
        businessDate: string;
    }): Promise<Totals | null>;
    stockMoves(facts: {
        scope: [
            string,
            string
        ];
        businessDate: string;
    }): Promise<Count | null>;
    stockLedger(facts: {
        companyId: string;
        businessDate: string;
        branchId: string;
        branchId2: string;
    }): Promise<Count | null>;
    unbalancedJournals(facts: {
        companyId: string;
        businessDate: string;
        branchId: string;
        propertyId: string;
    }): Promise<number>;
};
/** Same scoped, read-only controls are used by the Night Audit UI and start command. */
export async function hospitalityDayControls(repository: HospitalityDayControlRepository, companyId: string, branchId: string, propertyId: string, businessDate: string) {
    const scope: [
        string,
        string
    ] = [companyId, branchId];
    // A Night Audit command supplies a single transaction-bound connection; keep
    // these reads sequential so node-postgres never queues concurrent queries on it.
    const openShifts = await repository.openShifts({ scope: scope });
    const unreviewedVariances = await repository.unreviewedVariances({ scope: scope });
    const openBills = await repository.openBills({ scope: scope, businessDate: businessDate });
    const pendingPayments = await repository.pendingPayments({ scope: scope, businessDate: businessDate });
    const pendingHotelTransfers = await repository.pendingHotelTransfers({ companyId: companyId, propertyId: propertyId, businessDate: businessDate });
    const pendingChargeCorrections = await repository.pendingChargeCorrections({ companyId: companyId, propertyId: propertyId });
    const property = await repository.property({ propertyId: propertyId, companyId: companyId });
    const timezone = property ? (JSON.parse(property.config_json) as {
        timezone: string;
    }).timezone : 'Asia/Bangkok';
    const unpostedBookingReceipts = await repository.unpostedBookingReceipts({ propertyId: propertyId, companyId: companyId, timezone: timezone, businessDate: businessDate });
    const failedBookingRefunds = await repository.failedBookingRefunds({ propertyId: propertyId, companyId: companyId, timezone: timezone, businessDate: businessDate });
    const unpostedCounterReceipts = await repository.unpostedCounterReceipts({ propertyId: propertyId, companyId: companyId, timezone: timezone, businessDate: businessDate });
    const failedCounterRefunds = await repository.failedCounterRefunds({ propertyId: propertyId, companyId: companyId, timezone: timezone, businessDate: businessDate });
    const unresolvedRefunds = await repository.unresolvedRefunds({ scope: scope, businessDate: businessDate });
    const failedJobs = await repository.failedJobs({ companyId: companyId });
    const missingRoomCharges = await repository.missingRoomCharges({ scope: scope, businessDate: businessDate, propertyId: propertyId });
    const missingDirectInvoices = await repository.missingDirectInvoices({ scope: scope, businessDate: businessDate });
    const unmatchedVerifiedTransfers = await repository.unmatchedVerifiedTransfers({ scope: scope, businessDate: businessDate });
    const unmatchedVerifiedHotelTransfers = await repository.unmatchedVerifiedHotelTransfers({ companyId: companyId, propertyId: propertyId, businessDate: businessDate });
    const totals = await repository.totals({ scope: scope, businessDate: businessDate });
    const stockMoves = await repository.stockMoves({ scope: scope, businessDate: businessDate });
    const stockLedger = await repository.stockLedger({ companyId: companyId, businessDate: businessDate, branchId: branchId, branchId2: branchId });
    const stockValueMinor = Number(stockMoves?.n ?? 0), stockLedgerMinor = Number(stockLedger?.n ?? 0);
    const unbalancedJournals = await repository.unbalancedJournals({ companyId: companyId, businessDate: businessDate, branchId: branchId, propertyId: propertyId });
    const blockers = [
        openShifts ? `${openShifts} restaurant cash session(s) still open` : null,
        unreviewedVariances ? `${unreviewedVariances} restaurant cash variance(s) awaiting independent review` : null,
        openBills ? `${openBills} restaurant bill(s) still open for this business date` : null,
        pendingPayments ? `${pendingPayments} restaurant payment(s) awaiting a result or bank verification` : null,
        pendingHotelTransfers ? `${pendingHotelTransfers} hotel bank transfer(s) awaiting Finance verification` : null,
        pendingChargeCorrections ? `${pendingChargeCorrections} hotel folio correction(s) awaiting independent review` : null,
        unpostedBookingReceipts ? `${unpostedBookingReceipts} verified online hotel receipt(s) still need accounting recovery` : null,
        failedBookingRefunds ? `${failedBookingRefunds} online hotel refund(s) need recovery` : null,
        unpostedCounterReceipts ? `${unpostedCounterReceipts} verified counter receipt(s) still need accounting recovery` : null,
        failedCounterRefunds ? `${failedCounterRefunds} counter refund(s) need recovery` : null,
        unresolvedRefunds ? `${unresolvedRefunds} restaurant refund(s) awaiting resolution` : null,
        failedJobs ? `${failedJobs} restaurant background job(s) need recovery` : null,
        missingRoomCharges ? `${missingRoomCharges} room charge(s) lack a hotel revenue posting` : null,
        missingDirectInvoices ? `${missingDirectInvoices} direct restaurant sale(s) lack an invoice or revenue posting` : null,
        unmatchedVerifiedTransfers ? `${unmatchedVerifiedTransfers} verified bank transfer(s) lack matching imported bank evidence` : null,
        unmatchedVerifiedHotelTransfers ? `${unmatchedVerifiedHotelTransfers} verified hotel transfer(s) lack matching imported bank evidence` : null,
        stockValueMinor !== stockLedgerMinor ? 'Restaurant stock movements do not match their inventory ledger posting' : null,
        unbalancedJournals ? `${unbalancedJournals} restaurant or hotel journal(s) are unbalanced` : null,
    ].filter((value): value is string => Boolean(value));
    return { businessDate, blockers, openShifts, unreviewedVariances, openBills, pendingPayments, pendingHotelTransfers, pendingChargeCorrections, unpostedBookingReceipts, failedBookingRefunds, unpostedCounterReceipts, failedCounterRefunds, unresolvedRefunds, failedJobs, missingRoomCharges, missingDirectInvoices, unmatchedVerifiedTransfers, unmatchedVerifiedHotelTransfers, unbalancedJournals,
        directSalesMinor: Number(totals?.direct_total ?? 0), roomChargesMinor: Number(totals?.room_total ?? 0), cashReceivedMinor: Number(totals?.cash_received ?? 0), verifiedTransferMinor: Number(totals?.transfer_verified ?? 0), stockValueMinor, stockLedgerMinor };
}
