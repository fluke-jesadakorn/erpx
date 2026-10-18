import {captureReceiptAllocationCredit} from "@/lib/pms/pms-receipt-funding";
import { canonicalGuestAccount, parseGuestAccountJson } from "@/lib/identity_access/guest-account-compat";
import { z } from "zod";
import { executePms, newProperty, PmsError, readiness, type Effects, type Posting } from "@/lib/pms/pms-engine";
import { commandSchema, dateField, idField, textField, operationPermission, pmsAccountKeys, plusDays, record, records, type Config, type Property, type Snapshot } from "../domain/pms";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { prepareRoomCharge, roomChargeSchema } from "@/lib/pms/pms-room-charge";
import { hasPermission } from "@/lib/identity_access/permissions";
import { samePerson } from "@/lib/identity_access/iam";
import type { WorkItem } from "@/lib/work_projection/work-item";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { PmsSetupFacts } from "@/lib/pms/pms-read";
import { pmsReadSelectionSchema, type PmsReadSelection } from "@/lib/pms/pms-read-selection";
import type { Clock } from "../../shared/application/ports";
import type { PmsRecord, Kind } from "../domain/pms-model";
import type { FunctionalFxEvidence } from "../../finance/public/pms-journal-contracts";
export type ReadSnapshot = Snapshot & {
    dashboardHasStays?: boolean;
    setupFacts?: PmsSetupFacts;
    listMeta?: Awaited<ReturnType<typeof import("../public/pms/read-repository").readPmsOperationsList>>['listMeta'];
};
export type PropertyRow = {
    id: string;
    company_id: string;
    branch_id: string;
    name: string;
    business_date: string;
    state: Property["state"];
    config_json: string;
    version: number;
};
export type TransferRequestRow = {
    id: string;
    company_id: string;
    property_id: string;
    guest_account_id: string;
    shift_id: string;
    kind: 'receipt' | 'deposit';
    business_date: string;
    currency: string;
    amount_minor: number;
    reference: string;
    state: 'pending' | 'verified' | 'cancelled';
    version: number;
    recorded_by: string;
    bank_transaction_id: string | null;
    entry_id: string | null;
    finance_payment_id: string | null;
    created_at: number;
    updated_at: number;
};
export type ChargeCorrectionRow = {
    id: string;
    company_id: string;
    property_id: string;
    entry_id: string;
    guest_account_id: string;
    status: 'pending' | 'approved' | 'rejected';
    version: number;
    reason: string;
    requested_by: string;
    reviewed_by: string | null;
    review_reason: string | null;
    reversal_entry_id: string | null;
    created_at: number;
    reviewed_at: number | null;
    updated_at: number;
};
export type PmsRows<Row> = {
    results: Row[];
};
export type PmsEffect = {
    run(): Promise<PmsRows<Record<string, unknown>>>;
};
export type PmsAttachmentBinding<Effect> = {
    guards: Effect[];
    steps: Effect[];
};
export type PmsUnit<Effect extends PmsEffect> = {
    readPmsProperties(facts: {
        id: string;
        companyId: string;
        lock: boolean;
    }): Promise<PropertyRow | null>;
    readPmsProperties2(facts: {
        companyId: string;
        fact1: readonly string[];
        actor: WorkContext;
    }): Promise<PropertyRow[]>;
    readAssignments(facts: {
        companyId: string;
        fact1: number;
        fact2: number;
        fact3: string;
    }): Promise<{
        id: string;
        name: string;
    }[]>;
    readOrganizationNodes(facts: {
        companyId: string;
        fact1: readonly string[];
        actor: WorkContext;
    }): Promise<{
        id: string;
        name: string;
    }[]>;
    readFinanceAccounts(facts: {
        companyId: string;
    }): Promise<{
        id: string;
        name: string;
        code: string;
        chart_id: string;
        category: string;
    }[]>;
    readFinanceCharts(facts: {
        companyId: string;
    }): Promise<{
        id: string;
        name: string;
    }[]>;
    readFinanceParties(facts: {
        companyId: string;
    }): Promise<{
        id: string;
        name: string;
    }[]>;
    readCommerceProducts(facts: {
        companyId: string;
    }): Promise<{
        id: string;
        name: string;
    }[]>;
    readCommerceWarehouses(facts: {
        companyId: string;
        fact1: string;
    }): Promise<{
        id: string;
        name: string;
    }[]>;
    readPmsDocuments(facts: {
        id: string;
        fact1: string | null;
        fact2: string | null;
    }): Promise<{
        id: string;
        guest_account_id: string;
        kind: string;
        created_at: number;
    }[]>;
    readReceiptAllocationCredits(facts: {
        companyId:string;propertyId:string;branchId:string;chartId:string;currency:string;
        receivableAccountId:string;advanceAccountId:string;entries:Array<{id:string;date:string}>;
    }):Promise<Array<{entryId:string;amount:number}>>;
    readFinanceJournalLines(facts: {
        id: string;
    }): Promise<{
        account_id: string;
        debit: number;
        credit: number;
    }[]>;
    readPmsRecords(facts: {
        id: string;
    }): Promise<{
        due: number;
        advance: number;
        deposit: number;
    } | null>;
    readPmsTransferRequests(facts: {
        companyId: string;
        id: string;
    }): Promise<TransferRequestRow[]>;
    readPmsChargeCorrections(facts: {
        companyId: string;
        id: string;
        fact2: readonly string[];
        frontMode: boolean;
    }): Promise<{
        id: string;
        entry_id: string;
        guest_account_id: string;
        status: string;
        version: number;
        reason: string;
        requested_by: string;
        reviewed_by: string | null;
        review_reason: string | null;
        reversal_entry_id: string | null;
        created_at: number;
        entry_kind: string;
        entry_description: string;
        entry_total: number;
    }[]>;
    readPmsBookingEmails(facts: {
        id: string;
    }): Promise<{
        id: string;
        reservation_id: string;
        state: string;
        attempts: number;
        last_error: string | null;
    }[]>;
    readPosConnections(facts: {
        companyId: string;
    }): Promise<Record<string, unknown> | null>;
    readPmsStripeRefunds(propertyId: string): Promise<{id:string;property_id:string;guest_account_id:string;amount_minor:number;refund_amount_minor:number|null;refund_state:string;last_error:string|null;refund_reason:string|null;payer_email:string|null}[]>;
    readPmsStripePayments(facts: {
        id: string;
    }): Promise<{
        id: string;
        guest_account_id: string;
        state: string;
        kind: string;
        amount_minor: number;
        refund_state: string | null;
        payer_email: string | null;
    }[]>;
    readPmsRecords2(facts: {
        companyId: string;
        propertyId: string;
    }): Promise<{
        id: string;
        property_id: string;
        amount_minor: number;
        status: string;
        refund_state: string;
        created_at: number;
        last_error: string | null;
        needs_receipt_recovery: boolean;
    }[]>;
    readFinanceBankTransactions(facts: {
        companyId: string;
        bank: string;
        currency: string;
    }): Promise<{
        id: string;
        transaction_date: string;
        external_id: string;
        amount_minor: number;
        currency: string;
        version: number;
    }[]>;
    readPosConnections2(facts: {
        companyId: string;
        fact1: string;
    }): Promise<Record<string, unknown> | null>;
    readFinanceCharts2(facts: {
        chartId: string;
        companyId: string;
        currency: string;
    }): Promise<Record<string, unknown> | null>;
    readFinanceAccounts2(facts: {
        id: string;
        companyId: string;
        chartId: string;
        currency: string;
    }): Promise<{
        category: string;
    } | null>;
    readFinanceAccounts3(facts: {
        account: string;
        companyId: string;
        chartId: string;
        currency: string;
    }): Promise<Record<string, unknown> | null>;
    readFinanceAccounts4(facts: {
        id: string;
        companyId: string;
        chartId: string;
        currency: string;
    }): Promise<Record<string, unknown> | null>;
    readFinancePeriods(facts: {
        companyId: string;
        date: string;
        date2: string;
    }): Promise<{
        id: string;
    }[]>;
    insertPmsPostings(facts: {
        id: `${string}-${string}-${string}-${string}-${string}`;
        id2: string;
        source: string;
        recordId: string;
        id3: string;
        date: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    deleteCommandAssertions(facts: {
        id: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readCommerceWarehouses2(facts: {
        warehouseId: string;
        companyId: string;
        branchId: string;
    }): Promise<Record<string, unknown> | null>;
    readCommerceStock(facts: {
        warehouseId: string;
        productId: string;
    }): Promise<{
        quantity_milli: number;
        value_minor: number;
    } | null>;
    readInventoryLedger(facts: {
        warehouseId: string;
        productId: string;
        date: string;
    }): Promise<Record<string, unknown> | null>;
    updateCommerceStock(facts: {
        q: number;
        cost: number;
        warehouseId: string;
        productId: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    insertPmsStockMoves(facts: {
        id: string;
        id2: string;
        warehouseId: string;
        productId: string;
        fact4: number;
        fact5: number;
        date: string;
        fact7: number;
        description: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    insertPmsSources(facts: {
        id: string;
        key: string;
        id2: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    insertFinanceParties(facts: {
        party: string;
        companyId: string;
        fact2: string;
        name: string;
        currency: string;
        now: number;
        now2: number;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readPmsPostings(facts: {
        id: string;
        id2: string;
        id3: string;
    }): Promise<{
        journal_id: string;
    } | null>;
    insertFinanceInvoices(facts: {
        id: string;
        companyId: string;
        party: string;
        fact3: string;
        businessDate: string;
        fact5: string;
        currency: string;
        fact7: number;
        tax: number;
        total: number;
        currentDue: number;
        receivable: string;
        revenue: string;
        tax2: string;
        id2: string;
        fact15: "paid" | "posted";
        fact16: string | null;
        userId: string;
        approvedBy: string | null;
        now: number;
        now2: number;
        now3: number;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readPmsPostings2(facts: {
        id: string;
        entryId: string | undefined;
    }): Promise<{
        journal_id: string;
    } | null>;
    insertFinancePayments(facts: {
        id: string;
        companyId: string;
        party: string;
        fact3: "refund" | "receipt";
        fact4: string;
        businessDate: string;
        currency: string;
        amount: number;
        fact8: string;
        fact9: string | null;
        userId: string;
        userId2: string;
        now: number;
        now2: number;
    }): Promise<PmsRows<Record<string, unknown>>>;
    insertPmsDocuments(facts: {
        id: string;
        id2: string;
        id3: string;
        kind: "refund" | "receipt" | "invoice" | "credit_note";
        financeId: string | null;
        fact5: string;
        now: number;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readPmsImportBalances(facts: {
        id: string;
        guestAccountId: string;
    }): Promise<PmsRows<{
        id: string;
        total: number;
    }>>;
    readPmsDocuments2(facts: {
        id: string;
        guestAccountId: string;
    }): Promise<{
        id: string;
        snapshot_json: string;
    }[]>;
    deleteFinanceAllocations(facts: {
        id: string;
        companyId: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    updateFinanceInvoices(facts: {
        outstanding: number;
        outstanding2: number;
        outstanding3: number;
        outstanding4: number;
        fact4: number;
        id: string;
        companyId: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    insertFinanceAllocations(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        companyId: string;
        id: string;
        id2: string;
        n: number;
        userId: string;
        fact6: number;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readAssignments2(facts: {
        assigneeId: unknown;
        companyId: string;
        fact2: number;
        fact3: number;
        branchId: string;
    }): Promise<Record<string, unknown> | null>;
    readFinanceParties2(facts: {
        partyId: string;
        companyId: string;
        currency: string;
    }): Promise<Record<string, unknown> | null>;
    readCommerceProducts2(facts: {
        warehouseId: string;
        productId: string;
        companyId: string;
        branchId: string;
    }): Promise<Record<string, unknown> | null>;
    readContext(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readOrganizationNodes2(facts: {
        branchId: string;
        companyId: string;
    }): Promise<Record<string, unknown> | null>;
    insertPmsProperties(facts: {
        target: string;
        companyId: string;
        branchId: string;
        name: string;
        businessDate: string;
        fact5: string;
        userId: string;
        fact7: number;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readPmsChargeCorrections2(facts: {
        id: string;
    }): Promise<Record<string, unknown> | null>;
    insertPmsChargeCorrections(facts: {
        id: string;
        companyId: string;
        id2: string;
        id3: string;
        guestAccountId: string;
        reason: string;
        userId: string;
        now: number;
        now2: number;
    }): Effect;
    insertPmsChargeCorrectionEvents(facts: {
        fact0: string;
        id: string;
        companyId: string;
        id2: string;
        id3: string;
        userId: string;
        reason: string;
        now: number;
    }): Effect;
    updatePmsProperties(facts: {
        nextVersion: number;
        now: number;
        id: string;
        companyId: string;
        version: number;
    }): Effect;
    readPmsChargeCorrections3(facts: {
        id: string;
        companyId: string;
        id2: string;
    }): Promise<ChargeCorrectionRow | null>;
    updatePmsChargeCorrections(facts: {
        status: string;
        correctionVersion: number;
        userId: string;
        reason: string;
        reversalEntryId: string | null;
        now: number;
        now2: number;
        id: string;
        companyId: string;
        id2: string;
        expectedVersion: number;
    }): Effect;
    insertPmsChargeCorrectionEvents2(facts: {
        fact0: string;
        id: string;
        companyId: string;
        id2: string;
        entry_id: string;
        status: string;
        userId: string;
        correctionVersion: number;
        reason: string;
        reversalEntryId: string | null;
        now: number;
    }): Effect;
    updatePmsProperties2(facts: {
        propertyVersionAfter: number;
        now: number;
        id: string;
        companyId: string;
        propertyVersion: number;
    }): Effect;
    readAttachmentSets(facts: {
        companyId: string;
        attachmentSetIds: readonly string[];
    }): Promise<PmsRows<{
        id: string;
        purpose_key: string;
        component_kind: string;
        component_ref: string | null;
    }>>;
    insertPmsTransferRequests(facts: {
        id: `${string}-${string}-${string}-${string}-${string}`;
        companyId: string;
        id2: string;
        guestAccountId: string;
        shiftId: string;
        kind: "deposit" | "receipt";
        businessDate: string;
        currency: string;
        amount: number;
        reference: string;
        userId: string;
        now: number;
        now2: number;
    }): Effect;
    readPmsTransferRequests2(facts: {
        id: string;
        companyId: string;
        id2: string;
    }): Promise<TransferRequestRow | null>;
    updatePmsTransferRequests(facts: {
        userId: string;
        fact1: number;
        id: string;
        version: number;
    }): Effect;
    readCompanyFxRates(facts: {
        companyId: string;
        requestedCurrency: string;
    }): Promise<Record<string, unknown> | null>;
    readMarketingCampaigns(facts: {
        companyId: string;
        fact1: string;
    }): Promise<Record<string, unknown> | null>;
    readPmsTransferRequests3(facts: {
        id: string;
        companyId: string;
        id2: string;
    }): Promise<TransferRequestRow | null>;
    updateFinanceBankTransactions(facts: {
        bankTransactionId: string;
        companyId: string;
    }): Promise<{
        id: string;
        version: number;
        match_status: string;
        bank_account: string;
        currency: string;
        amount_minor: number;
        external_id: string;
        transaction_date: string;
    } | null>;
    readPmsBookingPayments(facts: {
        id: string;
    }): Promise<Record<string, unknown> | null>;
    readPmsBookingAllocations(facts: {
        id: string;
        toGuestAccountId: string;
    }): Promise<Record<string, unknown> | null>;
    readPmsBookingPayments2(facts: {
        reservationId: string;
    }): Promise<Record<string, unknown> | null>;
    readPmsBookingPayments3(facts: {
        reservationId: string;
    }): Promise<Record<string, unknown> | null>;
    readAttachmentPublications(facts: {
        publicationId: string;
        companyId: string;
        targetType: string;
        targetId: string;
    }): Promise<Record<string, unknown> | null>;
    readPosConnections3(facts: {
        companyId: string;
    }): Promise<{
        account_id: string;
        livemode: boolean;
    } | null>;
    updatePmsBookingEmails(facts: {
        fact0: number;
        emailId: string;
        id: string;
    }): Promise<Record<string, unknown> | null>;
    updatePmsDocuments(facts: {
        toGuestAccountId: string;
        id: string;
        id2: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    updatePmsTransferRequests2(facts: {
        userId: string;
        matchedBankId: string;
        id: string;
        id2: string;
        fact4: number;
        id3: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    updateFinanceBankTransactions2(facts: {
        id: string;
        matchedBankId: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    updatePmsProperties3(facts: {
        name: string;
        businessDate: string;
        state: "setup" | "open" | "closing";
        fact3: string;
        version: number;
        fact5: number;
        id: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    insertPmsJobs(facts: {
        nightJob: string;
        id: string;
        businessDate: string;
        fact3: string;
        fact4: number;
    }): Promise<PmsRows<Record<string, unknown>>>;
    updatePmsJobs(facts: {
        fact0: "pending" | "completed";
        fact1: number;
        id: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readPmsJobs(facts: {
        fact0: string | null;
        fact1: string | null;
        fact2: string | null;
        fact3: string | null;
    }): Promise<{
        id: string;
        property_id: string;
        actor_json: string;
    }[]>;
    readPmsProperties3(facts: {
        property_id: string;
    }): Promise<Record<string, unknown> | null>;
    updatePmsJobs2(facts: {
        code: string;
        fact1: number;
        id: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    updatePmsRecords(facts: {
        code: string;
        fact1: number;
        id: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readContext2(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    updatePmsProperties4(facts: {
        version: number;
        fact1: number;
        id: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    readContext3(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    updatePmsProperties5(facts: {
        version: number;
        fact1: number;
        id: string;
    }): Promise<PmsRows<Record<string, unknown>>>;
    insertPmsRecordsBatch(facts: Array<{
        id: string;
        id2: string;
        kind: Kind;
        fact3: string;
        version: number;
        updatedAt: number;
    }>): Promise<PmsRows<Record<string, unknown>>>;
    readMoneyTransactionSnapshots(facts: {
        companyId: string;
        fact1: string;
        date: string;
        fact3: string;
    }): Promise<PmsRows<{
        source_id: string;
    }>>;
    readPmsDashboard(arg0: string, arg1: string): Promise<{
        records: PmsRecord[];
        hasStays: boolean;
    }>;
    readPmsOperationsList(arg0:string,arg1:string,arg2:Extract<PmsReadSelection,{view:'list'}>): ReturnType<typeof import('../public/pms/read-repository').readPmsOperationsList>;
    readPmsCalendar(arg0: string, arg1: string, arg2: string): Promise<PmsRecord[]>;
    readPmsDetail(arg0: string, arg1: string): Promise<PmsRecord[]>;
    readPmsConfiguration(arg0: string): Promise<PmsRecord[]>;
    readAllPmsRecords(arg0: string): Promise<PmsRecord[]>;
    readPmsInventoryCommand(propertyId: string): Promise<PmsRecord[]>;
    readPmsStayCommand(propertyId: string, targetId: string, targetKind: 'stay' | 'reservation', guestId: string, guestAccountId: string, roomId: string): Promise<Snapshot['records']>;
    readPmsCashierCommand(propertyId: string, targetId: string, targetKind: 'guest_account' | 'account_close' | 'refund' | 'shift' | 'new_shift', shiftId: string, userId: string): Promise<Snapshot['records']>;
    readPmsPosCommand(propertyId: string, stayId: string, sourceKey: string): Promise<Snapshot['records']>;
    readPmsNightCommand(propertyId: string, businessDate: string, targetId: string): Promise<Snapshot['records']>;
    readPmsGuestCommandRecord(propertyId: string, guestId: string): Promise<PmsRecord[]>;
    readPmsReservationCommand(propertyId:string,guestId:string,source:string,externalRef:string):Promise<PmsRecord[]>;
    readPmsHousekeepingCommand(propertyId:string,targetId:string,targetKind:"task"|"room"):Promise<PmsRecord[]>;
    readPmsChoices(id:string,selection:Extract<PmsReadSelection,{view:'choices'}>):Promise<PmsRecord[]>;
    readPmsSetupFacts(arg0: string): Promise<PmsSetupFacts>;
    hospitalityDayControls(arg0: string, arg1: string, arg2: string, arg3: string): Promise<{
        businessDate: string;
        blockers: string[];
        openShifts: number;
        unreviewedVariances: number;
        openBills: number;
        pendingPayments: number;
        pendingHotelTransfers: number;
        pendingChargeCorrections: number;
        unpostedBookingReceipts: number;
        failedBookingRefunds: number;
        unpostedCounterReceipts: number;
        failedCounterRefunds: number;
        unresolvedRefunds: number;
        failedJobs: number;
        missingRoomCharges: number;
        missingDirectInvoices: number;
        unmatchedVerifiedTransfers: number;
        unmatchedVerifiedHotelTransfers: number;
        unbalancedJournals: number;
        directSalesMinor: number;
        roomChargesMinor: number;
        cashReceivedMinor: number;
        verifiedTransferMinor: number;
        stockValueMinor: number;
        stockLedgerMinor: number;
    }>;
    prepareJournal(arg0: WorkContext, arg1: {
        chartId: string;
        periodId: string;
        entryDate: string;
        currency: string;
        sourceType: string;
        sourceId: string;
        memo: string;
        isOpening: false;
        lines: {
            accountId: string;
            debit: string;
            credit: string;
            dimensions: {
                branchId: string;
                propertyId: string;
                recordId: string;
            };
            description: string;
        }[];
    }, arg2: `${string}-${string}-${string}-${string}-${string}`, arg3: "posted"): Promise<{
        steps: Effect[];
        guards: Effect[];
        preparedLines: {
            debit: number;
            credit: number;
            baseDebit: number;
            baseCredit: number;
            accountId: string;
            description?: string | undefined;
            dimensions?: Record<string, string> | undefined;
        }[];
        preparedFxEvidence: FunctionalFxEvidence | null;
        result: {
            id: string;
            number: string;
            status: "draft" | "posted";
            version: number;
            debit: string;
            credit: string;
            fxRateMicros: number;
            baseCurrency: string;
        };
    }>;
    assertProductionModuleEnabled(arg0: string, arg1: "pms"): Promise<void>;
    prepareAttachmentBinding(arg0: WorkContext, arg1: {
        attachmentSetIds: string[];
        bindingContract: {
            targetType: string;
            bindingType: string;
            contexts: {
                purposeKey: string;
                componentKind: "action";
                componentRef: string;
            }[];
        };
        targetId: string;
        expectedTargetVersion: number;
        commandId: string;
        bindingId: string;
    }): Promise<PmsAttachmentBinding<Effect>>;
    preparePmsAccounting(arg0: WorkContext, arg1: Snapshot, arg2: unknown): Promise<{
        chartId: string;
        accounts: Record<"inventory" | "cash" | "bank" | "edc" | "receivable" | "advance" | "deposit" | "revenue" | "service" | "tax" | "prepaidTax" | "consumption" | "commission" | "cashDifference", string>;
        taxBps: number;
        serviceBps: number;
        taxInclusive: boolean;
        advanceTax: boolean;
    }>;
    captureMoneySnapshots(actor: { companyId: string }, inputs: readonly {
        sourceType: string; sourceId: string; amountKind: string; transactionDate: string;
        currency: string; currencyScale: number; amountUnits: string;
    }[]): Promise<void>;
    snapshotMoney(arg0: WorkContext | {
        companyId: string;
    }, arg1: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
        currency: string;
        currencyScale: number;
        amountUnits: string;
    } | {
        sourceType: string;
        sourceId: string;
        amountKind: string;
        amountUnits: string;
        transactionDate: string;
        currency: string;
        currencyScale: number;
    }): Promise<{
        amountUnits: string;
        usdUnits: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        statement: Effect;
    }>;
    pmsOperationalWork(arg0: WorkContext): Promise<WorkItem[]>;
    snapshotMoneyReversal(arg0: {
        companyId: string;
    }, arg1: {
        amountKind: string;
        amountUnits: string;
        original: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
        };
        sourceType: string;
        sourceId: string;
        transactionDate: string;
    } | {
        amountUnits: string;
        original: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
        };
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
    }): Promise<{
        currency: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        amountUnits: string;
        statement: Effect;
    }>;
    snapshotMoneyCarry(arg0: {
        companyId: string;
    }, arg1: {
        amountUnits: string;
        allocationOffsetUnits: string;
        original: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
        };
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
    } | {
        amountUnits: string;
        original: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
        };
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
    } | {
        amountKind: string;
        amountUnits: string;
        original: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
        };
        sourceType: string;
        sourceId: string;
        transactionDate: string;
    }): Promise<{
        amountUnits: string;
        usdUnits: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        statement: Effect;
    }>;
    transactional: boolean;
    transaction<Result>(work: (unit: PmsUnit<Effect>) => Promise<Result>,propertyLockId?:string, command?: { actor: WorkContext; operation: string; input: unknown; key: string }): Promise<Result>;
    batch(effects: Effect[]): Promise<unknown>;
    replay<Result extends object = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    execute<Result extends object>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: {
        companyId: string;
        branchId: string | null;
        departmentId: null;
    }): Promise<Result>;
};
export type PmsRepository<Effect extends PmsEffect> = PmsUnit<Effect>;
export function createPmsApplication<Effect extends PmsEffect>(ports: {
    forActor(actor: WorkContext): Promise<PmsUnit<Effect>>;
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string,details?:{currentVersion:number}): Error;
    isBusinessError(error: unknown): error is Error & {
        code: string;
    };
    environment(key: "NODE_ENV" | "RESEND_API_KEY" | "RESEND_FROM" | "PMS_STRIPE_LIVE_ENABLED"): string | undefined;
}) {
    function property(row: PropertyRow): Property {
        return {
            id: row.id,
            companyId: row.company_id,
            branchId: row.branch_id,
            name: row.name,
            businessDate: row.business_date,
            state: row.state,
            config: parseGuestAccountJson(row.config_json),
            version: Number(row.version),
        };
    }
    function scope(actor: WorkContext, p: Property) {
        if (p.companyId !== actor.companyId ||
            (actor.branchId && p.branchId !== actor.branchId) ||
            actor.departmentId ||
            actor.beta)
            throw ports.error(404, "PMS_NOT_FOUND", "Hotel not found in this workspace");
    }
    function assertExpectedVersion(p:Property,expectedVersion?:number|null){
        if(expectedVersion!==undefined&&p.version!==expectedVersion)
            throw ports.error(409,'PMS_VERSION_CONFLICT','Hotel data changed. Load the latest version and review again',{currentVersion:p.version});
    }
    async function loadPms(db: PmsUnit<Effect>, actor: WorkContext, id: string, lock = false, selection?: PmsReadSelection,expectedVersion?:number|null): Promise<ReadSnapshot> {
        if (lock && selection)
            throw new Error("PMS_COMMAND_REQUIRES_COMPLETE_STATE");
        const row = await db.readPmsProperties({ id: id, companyId: actor.companyId, lock: lock });
        if (!row)
            throw ports.error(404, "PMS_NOT_FOUND", "Hotel not found");
        const p = property(row);
        scope(actor, p);
        if(lock)assertExpectedVersion(p,expectedVersion);
        const selected = selection ? pmsReadSelectionSchema.parse(selection) : undefined;
        if(selected?.view==='choices' && !(selected.kind==='block'
          ? hasPermission(actor,'pms.reservations')
          : hasPermission(actor,'pms.reservations')||hasPermission(actor,'pms.frontdesk')||hasPermission(actor,'pms.cashier')))
            throw ports.error(403,'PMS_CHOICES_FORBIDDEN','Guest and allocation choices require hotel operation permission');
        const dashboard = selected?.view === "dashboard" ? await db.readPmsDashboard(id, p.businessDate) : undefined;
        const list = selected?.view === 'list' ? await db.readPmsOperationsList(id, p.businessDate, selected) : undefined;
        const rows = list?.records ?? dashboard?.records ?? (selected?.view === "choices"
            ? await db.readPmsChoices(id,selected)
            : selected?.view === "calendar"
            ? await db.readPmsCalendar(id, selected.from, selected.to)
            : selected?.view === "detail" ? await db.readPmsDetail(id, selected.id)
                : selected?.view === "setup" ? await db.readPmsConfiguration(id)
                    : selected?.view === "report" ? []
                        : await db.readAllPmsRecords(id));
        if (selected?.view === "detail" && !rows.some(row => row.id === selected.id))
            throw ports.error(404, "PMS_RECORD_NOT_FOUND", "Hotel item not found");
        const setupFacts = selected && selected.view !== 'dashboard'
            ? await db.readPmsSetupFacts(id) : undefined;
        return {
            property: p,
            records: rows,
            ...(list ? { listMeta: list.listMeta } : {}),
            ...(dashboard ? { dashboardHasStays: dashboard.hasStays } : {}),
            ...(setupFacts ? { setupFacts } : {}),
        };
    }
    /** Reservation creation needs all inventory/configuration facts and all
     * nonterminal stays, including overdue occupancy. Archived stays cannot
     * consume capacity; archived channel references still prevent duplicates.
     * This is an authoritative reader under the existing property lock. */
    async function loadReservationCommand(db:PmsUnit<Effect>,actor:WorkContext,id:string,input:unknown,expectedVersion?:number|null):Promise<Snapshot>{
        const row=await db.readPmsProperties({id,companyId:actor.companyId,lock:true});
        if(!row)throw ports.error(404,'PMS_NOT_FOUND','Hotel not found');
        const p=property(row);scope(actor,p);
        assertExpectedVersion(p,expectedVersion);
        const value=z.object({guestId:idField.optional(),source:textField.min(1),externalRef:textField.default('')}).parse(input);
        return {property:p,records:await db.readPmsReservationCommand(id,value.guestId??'',value.source,value.externalRef)};
    }
    /** Guest edits reference one guest only. This is an authoritative write reader,
     * never a bounded UI snapshot; property scope/lock/version remain unchanged. */
    const housekeepingCommands=new Set(['task.create','task.assign','task.start','task.complete','task.inspect','room.dnd','room.observe']);
    async function loadHousekeepingCommand(db:PmsUnit<Effect>,actor:WorkContext,id:string,operation:string,input:unknown,expectedVersion?:number|null):Promise<Snapshot>{
        const value=z.object({id:idField.optional(),roomId:idField.optional()}).parse(input);
        const targetId=operation==='task.create'?value.roomId:value.id;
        if(!targetId)throw ports.error(422,'PMS_RECORD_REQUIRED','Select a room or room task');
        const row=await db.readPmsProperties({id,companyId:actor.companyId,lock:true});
        if(!row)throw ports.error(404,'PMS_NOT_FOUND','Hotel not found');
        const p=property(row);scope(actor,p);
        assertExpectedVersion(p,expectedVersion);
        return {property:p,records:await db.readPmsHousekeepingCommand(id,targetId,operation.startsWith('task.')&&operation!=='task.create'?'task':'room')};
    }
    async function loadGuestCommand(db: PmsUnit<Effect>, actor: WorkContext, id: string, operation: string, input: unknown,expectedVersion?:number|null): Promise<Snapshot> {
        const value = z.object({id:idField.optional(),guestId:idField.optional()}).parse(input);
        const guestId = operation === 'guest.save' ? value.id : value.guestId;
        if(operation !== 'guest.save' && !guestId) throw ports.error(422,'PMS_GUEST_REQUIRED','Select a guest');
        const row = await db.readPmsProperties({id,companyId:actor.companyId,lock:true});
        if(!row) throw ports.error(404,'PMS_NOT_FOUND','Hotel not found');
        const p=property(row); scope(actor,p);
        assertExpectedVersion(p,expectedVersion);
        return {property:p,records:guestId ? await db.readPmsGuestCommandRecord(id,guestId) : []};
    }
    const inventoryCommands = new Set(['room_type.save','room.save','rate.save','rate.calendar','block.create','block.release']);
    /** Complete inventory/configuration plus every nonterminal stay, with no UI
     * paging/date bound. Released blocks and inactive rooms/rates remain targets;
     * overdue occupancy/segments still protect assignment and capacity. */
    async function loadInventoryCommand(db:PmsUnit<Effect>,actor:WorkContext,id:string,expectedVersion?:number|null):Promise<Snapshot>{
        const row=await db.readPmsProperties({id,companyId:actor.companyId,lock:true});
        if(!row)throw ports.error(404,'PMS_NOT_FOUND','Hotel not found');
        const p=property(row);scope(actor,p);assertExpectedVersion(p,expectedVersion);
        return {property:p,records:await db.readPmsInventoryCommand(id)};
    }
    const stayCommands = new Set(['reservation.cancel', 'reservation.no_show', 'stay.cancel', 'stay.no_show', 'stay.assign', 'stay.move', 'stay.upgrade', 'stay.guest', 'stay.route_guest_account', 'stay.check_in', 'stay.change_dates', 'stay.check_out', 'stay.preview_checkout']);
    /** All active inventory, booking siblings and complete account funding/exposure.
     * This authoritative write reader has no UI paging or date-window restriction. */
    async function loadStayCommand(db: PmsUnit<Effect>, actor: WorkContext, id: string, operation: string, input: unknown, expectedVersion?: number | null): Promise<Snapshot> {
        const row = await db.readPmsProperties({ id, companyId: actor.companyId, lock: true });
        if (!row) throw ports.error(404, 'PMS_NOT_FOUND', 'Hotel not found');
        const p = property(row); scope(actor, p); assertExpectedVersion(p, expectedVersion);
        const value = z.object({ id: idField, guestId: idField.optional(), guestAccountId: idField.optional(), roomId: idField.optional() }).parse(input);
        return { property: p, records: await db.readPmsStayCommand(id, value.id, operation.startsWith('reservation.') ? 'reservation' : 'stay', value.guestId ?? '', value.guestAccountId ?? '', value.roomId ?? '') };
    }
    const cashierCommands = new Set(['charge.add', 'payment.receive', 'payment.deposit', 'payment.apply_deposit', 'refund.request', 'refund.approve', 'refund.reject', 'refund.pay', 'guest_account.close', 'shift.open', 'shift.close', 'shift.review', 'cash.move']);
    /** Complete account funding/refund history and the referenced shift, not a UI page.
     * Property scope/version/lock and Finance/IAM guards remain authoritative. */
    async function loadCashierCommand(db: PmsUnit<Effect>, actor: WorkContext, id: string, operation: string, input: unknown, expectedVersion?: number | null): Promise<Snapshot> {
        const row = await db.readPmsProperties({ id, companyId: actor.companyId, lock: true });
        if (!row) throw ports.error(404, 'PMS_NOT_FOUND', 'Hotel not found');
        const p = property(row); scope(actor, p); assertExpectedVersion(p, expectedVersion);
        const value = z.object({ id: idField.optional(), guestAccountId: idField.optional(), shiftId: idField.optional() }).parse(input);
        const targetKind = operation === 'guest_account.close' ? 'account_close' : operation === 'shift.open' ? 'new_shift'
            : operation.startsWith('shift.') || operation === 'cash.move' ? 'shift'
                : ['refund.approve', 'refund.reject', 'refund.pay'].includes(operation) ? 'refund' : 'guest_account';
        const targetId = targetKind === 'new_shift' ? '' : targetKind === 'guest_account' ? value.guestAccountId : operation === 'cash.move' ? value.shiftId : value.id;
        if (targetId === undefined) throw ports.error(422, 'PMS_RECORD_REQUIRED', 'Select a guest account, refund or cashier shift');
        return { property: p, records: await db.readPmsCashierCommand(id, targetId, targetKind, value.shiftId ?? '', actor.userId) };
    }
    const nightCommands = new Set(['night.start', 'night.resume', 'night.step']);
    async function loadNightCommand(db: PmsUnit<Effect>, actor: WorkContext, id: string, operation: string, input: unknown, expectedVersion?: number | null): Promise<Snapshot> {
        const row = await db.readPmsProperties({ id, companyId: actor.companyId, lock: true });
        if (!row) throw ports.error(404, 'PMS_NOT_FOUND', 'Hotel not found');
        const p = property(row); scope(actor, p); assertExpectedVersion(p, expectedVersion);
        const targetId = operation === 'night.start' ? '' : z.object({ id: idField }).parse(input).id;
        return { property: p, records: await db.readPmsNightCommand(id, p.businessDate, targetId) };
    }
    const pmsPermissions = [
        "pms.read",
        "pms.configure",
        "pms.reservations",
        "pms.frontdesk",
        "pms.cashier",
        "pms.housekeeping",
        "pms.inspect",
        "pms.maintenance",
        "pms.audit",
        "pms.approve",
        "pms.accounting.prepare",
        "pms.accounting.approve",
        "pms.reports",
        "finance.approve",
    ] as const;
    function visibleSnapshot(s: ReadSnapshot, actor: WorkContext): ReadSnapshot {
        const result = structuredClone(s);
        delete result.setupFacts;
        const financial = hasPermission(actor, "pms.cashier") || hasPermission(actor, "pms.reports");
        const guest = hasPermission(actor, "pms.reservations") ||
            hasPermission(actor, "pms.frontdesk") ||
            financial;
        if (!guest)
            result.records = result.records.filter((r) => ![
                "guest",
                "reservation",
                "guest_account",
                "entry",
                "refund",
                "credit",
                "shift",
            ].includes(r.kind));
        if(!guest)result.records=result.records.filter(row=>row.kind!=='block'||(row as PmsRecord<'block'>).data.kind!=='group');
        else if (!financial)
            result.records = result.records.filter((r) => !["entry", "refund", "credit", "shift"].includes(r.kind));
        const pricing = guest || hasPermission(actor, "pms.configure") ||
            hasPermission(actor, "pms.accounting.prepare") || hasPermission(actor, "pms.accounting.approve");
        if (!pricing)
            result.records = result.records.filter(row => row.kind !== "rate")
                .map(row => row.kind === "item" ? { ...row, data: { ...row.data, price: 0 } } : row);
        if (!guest)
            result.records = result.records.map(row => row.kind === "stay"
                ? { ...row, data: { ...row.data, upgrades: undefined, bookingPolicy: undefined } } : row);
        if (!financial)
            result.records = result.records.map((r) => r.kind === "stay"
                ? { ...r, data: { ...r.data, prices: [] } }
                : r.kind === "guest_account"
                    ? {
                        ...r,
                        data: {
                            ...r.data,
                            due: 0,
                            advance: 0,
                            advanceTax: 0,
                            deposit: 0,
                        },
                    }
                    : r);
        if (!hasPermission(actor, "pms.accounting.prepare") &&
            !hasPermission(actor, "pms.accounting.approve") &&
            !hasPermission(actor, "pms.reports"))
            result.property.config = {
                ...result.property.config,
                accounts: Object.fromEntries(pmsAccountKeys.map((k) => [k, ""])) as Config["accounts"],
                chartId: "",
            };
        redactListMetadata(result);
        return result;
    }
    function redactListMetadata(snapshot: ReadSnapshot) {
        if (!snapshot.listMeta)
            return;
        const visible = new Set(snapshot.records.map(row => row.id));
        snapshot.listMeta.ids = snapshot.listMeta.ids.filter(id => visible.has(id));
        if (snapshot.listMeta.selectedId && !visible.has(snapshot.listMeta.selectedId))
            delete snapshot.listMeta.selectedId;
    }
    async function pmsWorkspace(actor: WorkContext, propertyId?: string, includeDayControls = false, frontMode = false, selection?: PmsReadSelection) {
        ports.require(actor, "pms.read");
        if (actor.departmentId || actor.beta)
            throw ports.error(403, "PMS_BRANCH_REQUIRED", "Select a company or branch area");
        const write = await ports.forActor(actor);
        const readSelection = selection ? pmsReadSelectionSchema.parse(selection) : undefined;
        const complete = !readSelection;
        const setup = readSelection?.view === 'setup';
        const report = readSelection?.view === 'report';
        const section = readSelection?.view === 'list' ? readSelection.section : undefined;
        const staffNeeded = complete || ['rooms', 'housekeeping', 'cashier', 'night-audit', 'guest-accounts'].includes(section ?? '');
        const rows = await write.readPmsProperties2({ companyId: actor.companyId, fact1: (actor.branchId ? [actor.branchId] : []), actor: actor });
        const selected = propertyId ? rows.find((r) => r.id === propertyId) : rows[0];
        if (propertyId && !selected)
            throw ports.error(404, "PMS_NOT_FOUND", "Hotel not found");
        const s = selected
            ? await (write.transactional
                ? write.transaction((tx) => loadPms(tx, actor, selected.id, false, readSelection))
                : loadPms(write, actor, selected.id, false, readSelection))
            : null;
        const configuration = !frontMode && (complete || setup || !s || section === 'inventory') && (hasPermission(actor, "pms.configure") ||
            hasPermission(actor, "pms.accounting.prepare") ||
            hasPermission(actor, "pms.accounting.approve"));
        const staff = staffNeeded ? await write.readAssignments({ companyId: actor.companyId, fact1: ports.clock.now(), fact2: ports.clock.now(), fact3: selected?.branch_id ?? actor.branchId ?? "" }) : [];
        const accounting = (complete || setup) && !frontMode && (hasPermission(actor, "pms.accounting.prepare") ||
            hasPermission(actor, "pms.accounting.approve"));
        const branches = configuration
            ? await write.readOrganizationNodes({ companyId: actor.companyId, fact1: (actor.branchId ? [actor.branchId] : []), actor: actor })
            : [];
        const accounts = accounting
            ? await write.readFinanceAccounts({ companyId: actor.companyId })
            : [];
        const charts = accounting
            ? await write.readFinanceCharts({ companyId: actor.companyId })
            : [];
        const parties = (complete || setup || report || section === 'guest-accounts') && !frontMode && (hasPermission(actor, "pms.cashier") ||
            hasPermission(actor, "pms.configure") ||
            hasPermission(actor, "pms.reports"))
            ? await write.readFinanceParties({ companyId: actor.companyId })
            : [];
        const products = configuration
            ? await write.readCommerceProducts({ companyId: actor.companyId })
            : [];
        const warehouses = configuration
            ? await write.readCommerceWarehouses({ companyId: actor.companyId, fact1: selected?.branch_id ?? actor.branchId ?? "" })
            : [];
        const docs = s && (!readSelection || readSelection.view === "list" || readSelection.view === "detail") &&
            (hasPermission(actor, "pms.cashier") || hasPermission(actor, "pms.reports"))
            ? await write.readPmsDocuments({ id: s.property.id, fact1: readSelection ? JSON.stringify(records(s, "guest_account").map(f => f.id)) : null, fact2: readSelection ? JSON.stringify(records(s, "guest_account").map(f => f.id)) : null })
            : [];
        let ledger: null | {
            debit: number;
            credit: number;
            receivable: number;
            advance: number;
            deposit: number;
        } = null;
        if ((complete || report) && !frontMode && s && hasPermission(actor, "pms.reports")) {
            const lines = await write.readFinanceJournalLines({ id: s.property.id });
            const balance = (account: string) => lines
                .filter((l) => l.account_id === account)
                .reduce((n, l) => n + Number(l.debit) - Number(l.credit), 0);
            ledger = {
                debit: lines.reduce((n, l) => n + Number(l.debit), 0),
                credit: lines.reduce((n, l) => n + Number(l.credit), 0),
                receivable: balance(s.property.config.accounts.receivable),
                advance: -balance(s.property.config.accounts.advance),
                deposit: -balance(s.property.config.accounts.deposit),
            };
        }
        const controlBalances = report && !frontMode && s && hasPermission(actor, 'pms.reports')
            ? await write.readPmsRecords({ id: s.property.id })
            : null;
        const snapshot = s ? visibleSnapshot(s, actor) : null;
        if (frontMode && snapshot) {
            const { currency, minorUnit, timezone } = snapshot.property.config;
            snapshot.property.config = {
                country: "",
                currency,
                minorUnit,
                timezone,
                checkIn: "",
                checkOut: "",
                cancellationPolicy: "",
                chartId: "",
                taxBps: 0,
                serviceBps: 0,
                taxInclusive: false,
                advanceTax: false,
                preparedBy: "",
                approvedBy: null,
                revision: 0,
                accounts: Object.fromEntries(pmsAccountKeys.map((key) => [key, ""])) as Config["accounts"],
            };
            snapshot.records = snapshot.records
                .filter((row) => {
                const rowData = row.data as {
                    requestedBy?: string;
                    userId?: string;
                };
                return ["room_type", "room", "rate", "guest", "reservation", "stay", "guest_account", "entry", "task", "block", "shift", "refund", "night"].includes(row.kind) &&
                    row.kind !== "credit" &&
                    (row.kind !== "refund" || rowData.requestedBy === actor.userId) &&
                    (row.kind !== "shift" || rowData.userId === actor.userId);
            })
                .map((row) => row.kind === "entry"
                ? { ...row, data: { ...row.data, sourceKey: "sourceKey" in row.data && typeof row.data.sourceKey === "string" && row.data.sourceKey.startsWith("pos:") ? "pos" : "front" } }
                : row);
            redactListMetadata(snapshot);
        }
        if (readSelection?.view === "detail" && snapshot && !snapshot.records.some(row => row.id === readSelection.id))
            throw ports.error(404, "PMS_RECORD_NOT_FOUND", "Hotel item not found");
        const frontPermissions = [
            "pms.read",
            "pms.reservations",
            "pms.frontdesk",
            "pms.cashier",
            "pms.housekeeping",
            "pms.inspect",
            "pms.maintenance",
            "pms.audit",
        ] as const;
        const transferRequests = (complete || section === 'guest-accounts') && s && (hasPermission(actor, "pms.cashier") || hasPermission(actor, "finance.approve"))
            ? await write.readPmsTransferRequests({ companyId: actor.companyId, id: s.property.id })
            : [];
        const chargeCorrections = (complete || section === 'guest-accounts') && s && (hasPermission(actor, "pms.cashier") || hasPermission(actor, "pms.approve"))
            ? await write.readPmsChargeCorrections({ companyId: actor.companyId, id: s.property.id, fact2: (frontMode ? [actor.userId] : []), frontMode: frontMode })
            : [];
        const readinessChecks = s ? readiness(s, s.setupFacts) : [];
        return {
            bookingDelivery: (complete || setup) && !frontMode && s && hasPermission(actor, "pms.configure")
                ? await write.readPmsBookingEmails({ id: s.property.id })
                : [],
            transferRequests: frontMode
                ? transferRequests.filter((row) => row.recorded_by === actor.userId)
                : transferRequests,
            chargeCorrections,
            stripeConnectionReady: (complete || section === 'guest-accounts') && !frontMode && hasPermission(actor, 'pms.cashier') && Boolean(await write.readPosConnections({ companyId: actor.companyId })),
            stripePayments: (complete || section === 'guest-accounts') && !frontMode && s && hasPermission(actor, 'pms.cashier') ? await write.readPmsStripePayments({ id: s.property.id }) : [],
            counterRefunds: (complete || section === 'guest-accounts') && !frontMode && s && hasPermission(actor,'pms.approve') ? await write.readPmsStripeRefunds(s.property.id) : [],
            bookingRefunds: (complete || section === 'guest-accounts') && !frontMode && s && hasPermission(actor, 'pms.approve')
                ? await write.readPmsRecords2({ companyId: actor.companyId, propertyId:s.property.id })
                : [],
            transferBankEvidence: (complete || section === 'guest-accounts') && !frontMode && s && hasPermission(actor, 'finance.approve')
                ? await write.readFinanceBankTransactions({ companyId: actor.companyId, bank: s.property.config.accounts.bank, currency: s.property.config.currency })
                : [],
            defaultDate: new Intl.DateTimeFormat("en-CA", {
                timeZone: "Asia/Bangkok",
                year: "numeric",
                month: "2-digit",
                day: "2-digit",
            }).format(new Date(ports.clock.now())),
            properties: rows.map((r) => ({
                id: r.id,
                name: r.name,
                branchId: r.branch_id,
                state: r.state,
                businessDate: r.business_date,
            })),
            snapshot,
            readiness: readinessChecks,
            frontReady: frontMode ? Boolean(s && s.property.state === "open" && readinessChecks.length === 0) : undefined,
            canReviewBookingMedia: hasPermission(actor, "records.review") || hasPermission(actor, "commerce.approve"),
            bookingReadiness: { email: Boolean(ports.environment("RESEND_API_KEY") && ports.environment("RESEND_FROM")), accounting: Boolean(s?.property.config.approvedBy), onlineAccounting: Boolean(s?.property.config.onlineAccounting?.approvedBy), fees: Boolean(s?.property.config.bookingFees?.approvedBy), connect: !frontMode && Boolean(await write.readPosConnections2({ companyId: actor.companyId, fact1: ports.environment("PMS_STRIPE_LIVE_ENABLED") ?? "false" })) },
            canManageSetup: hasPermission(actor, "pms.configure") || hasPermission(actor, "pms.accounting.prepare") || hasPermission(actor, "pms.accounting.approve"),
            permissions: (frontMode ? frontPermissions : pmsPermissions).filter((k) => hasPermission(actor, k)),
            availablePaymentMethods: ['cash'],
            userId: actor.userId,
            branches,
            staff,
            accounts: frontMode ? [] : accounts,
            charts: frontMode ? [] : charts,
            parties: frontMode ? [] : parties,
            products: frontMode ? [] : products,
            warehouses: frontMode ? [] : warehouses,
            documents: docs,
            ledger: frontMode ? null : ledger,
            controlBalances,
            dayControls: (complete || section === 'night-audit') && s && includeDayControls && (hasPermission(actor, 'pms.audit') || (!frontMode && hasPermission(actor, 'pms.reports')))
                ? await write.hospitalityDayControls(actor.companyId, s.property.branchId, s.property.id, s.property.businessDate)
                : null,
        };
    }
    /** Legacy and fresh command snapshots use the same actual native credit.
     * Read only inside the existing locked PMS unit; no evidence is serialized. */
    async function hydrateReceiptAllocationCredits(db:PmsUnit<Effect>,s:Snapshot){
        const entries=records(s,"entry").filter(row=>row.data.kind==="adjustment"&&row.data.total<0&&row.data.originalId);
        if(!entries.length)return;
        const p=s.property,c=p.config;
        const credits=await db.readReceiptAllocationCredits({companyId:p.companyId,propertyId:p.id,branchId:p.branchId,chartId:c.chartId,currency:c.currency,receivableAccountId:c.accounts.receivable,advanceAccountId:c.accounts.advance,entries:entries.map(row=>({id:row.id,date:row.data.date}))});
        for(const credit of credits){const entry=entries.find(row=>row.id===credit.entryId);if(entry&&Number.isSafeInteger(credit.amount)&&credit.amount>0&&credit.amount<=-entry.data.total)captureReceiptAllocationCredit(s,entry.id,credit.amount);}
    }
    async function validSetup(db: PmsUnit<Effect>, s: Snapshot) {
        const p = s.property, c = p.config;
        const chart = await db.readFinanceCharts2({ chartId: c.chartId, companyId: p.companyId, currency: c.currency });
        if (!chart)
            throw ports.error(422, "PMS_CHART_INVALID", "Select this company's THB chart of accounts");
        if (new Set(Object.values(c.accounts)).size !== pmsAccountKeys.length)
            throw ports.error(422, "PMS_ACCOUNTS_DISTINCT", "Configure a separate control account for each category");
        for (const [kind, id] of Object.entries(c.accounts)) {
            const account = await db.readFinanceAccounts2({ id: id, companyId: p.companyId, chartId: c.chartId, currency: c.currency });
            const category = ["revenue", "service"].includes(kind)
                ? "revenue"
                : ["advance", "deposit", "tax"].includes(kind)
                    ? "liability"
                    : ["consumption", "commission", "cashDifference"].includes(kind)
                        ? "expense"
                        : "asset";
            if (!account || account.category !== category)
                throw ports.error(422, "PMS_ACCOUNT_INVALID", `Account ${kind} must be ${category} in the same chart and currency`);
        }
        if (c.bookingFees && !await db.readFinanceAccounts3({ account: c.bookingFees.account, companyId: p.companyId, chartId: c.chartId, currency: c.currency }))
            throw ports.error(422, "PMS_FEE_ACCOUNT_INVALID", "Cancellation fees require a separate active revenue account in the company chart");
        if (c.onlineAccounting)
            for (const id of [c.onlineAccounting.refundPayable, c.onlineAccounting.unallocated]) {
                const account = await db.readFinanceAccounts4({ id: id, companyId: p.companyId, chartId: c.chartId, currency: c.currency });
                if (!account)
                    throw ports.error(422, "PMS_ONLINE_ACCOUNT_INVALID", "Online refund and unallocated accounts must be active company liability accounts");
            }
    }
    async function post(db: PmsUnit<Effect>, actor: WorkContext, s: Snapshot, effect: Posting) {
        const p = s.property, c = p.config;
        const periods = await db.readFinancePeriods({ companyId: p.companyId, date: effect.date, date2: effect.date });
        if (periods.length !== 1)
            throw ports.error(409, "PMS_PERIOD_CLOSED", "An open accounting period is required for this business date");
        const id = ports.uuid();
        const prepared = await db.prepareJournal(actor, {
            chartId: c.chartId,
            periodId: periods[0].id,
            entryDate: effect.date,
            currency: c.currency,
            sourceType: "pms",
            sourceId: p.id,
            memo: effect.source,
            isOpening: false,
            lines: effect.lines.map((l) => ({
                accountId: l.account === "cancellationFee" ? c.bookingFees!.account : l.account === "refundPayable" || l.account === "unallocated" ? c.onlineAccounting![l.account] : c.accounts[l.account],
                debit: minorToDecimal(l.debit),
                credit: minorToDecimal(l.credit),
                dimensions: {
                    branchId: p.branchId,
                    propertyId: p.id,
                    recordId: effect.recordId,
                },
                description: effect.source,
            })),
        }, id, "posted");
        await db.batch(prepared.steps);
        await db.insertPmsPostings({ id: id, id2: p.id, source: effect.source, recordId: effect.recordId, id3: prepared.result.id, date: effect.date });
        await db.deleteCommandAssertions({ id: id });
        return prepared.result.id;
    }
    async function effects(db: PmsUnit<Effect>, actor: WorkContext, s: Snapshot, e: Effects) {
        for (const effect of e.postings)
            await post(db, actor, s, effect);
        for (const move of e.stock) {
            const warehouse = await db.readCommerceWarehouses2({ warehouseId: move.warehouseId, companyId: s.property.companyId, branchId: s.property.branchId });
            if (!warehouse)
                throw ports.error(422, "PMS_WAREHOUSE_SCOPE", "Warehouse is not in this hotel");
            const stock = await db.readCommerceStock({ warehouseId: move.warehouseId, productId: move.productId });
            const q = -move.quantityMilli;
            if (!stock || Number(stock.quantity_milli) < q)
                throw ports.error(409, "PMS_STOCK_SHORT", "Insufficient inventory");
            if (await db.readInventoryLedger({ warehouseId: move.warehouseId, productId: move.productId, date: move.date }))
                throw ports.error(409, "PMS_STOCK_BACKDATED", "Movement already exists after this business date");
            const cost = q === Number(stock.quantity_milli)
                ? Number(stock.value_minor)
                : Number((BigInt(stock.value_minor) * BigInt(q) +
                    BigInt(stock.quantity_milli) / BigInt(2)) /
                    BigInt(stock.quantity_milli));
            await db.updateCommerceStock({ q: q, cost: cost, warehouseId: move.warehouseId, productId: move.productId });
            await db.insertPmsStockMoves({ id: move.id, id2: s.property.id, warehouseId: move.warehouseId, productId: move.productId, fact4: -q, fact5: -cost, date: move.date, fact7: ports.clock.now(), description: move.description });
            if (cost)
                await post(db, actor, s, {
                    source: `stock:${move.id}`,
                    recordId: move.id,
                    date: move.date,
                    lines: [
                        { account: "consumption", debit: cost, credit: 0 },
                        { account: "inventory", debit: 0, credit: cost },
                    ],
                });
        }
        for (const source of e.sources)
            await db.insertPmsSources({ id: s.property.id, key: source.key, id2: source.id });
        for (const doc of e.documents)
            await document(db, actor, s, doc);
        // Keep issued PMS receivables in Finance synchronized through the owner workflow.
        for (const f of records(s, "guest_account").filter((r) => (r.data.invoiceId || records(s, "entry").some(entry => entry.data.guestAccountId === r.id && entry.data.method === 'opening' && entry.data.kind === 'charge')) && (e.postings.length || e.documents.length))) {
            await syncAllocations(db, actor, s, f.id);
        }
    }
    async function document(db: PmsUnit<Effect>, actor: WorkContext, s: Snapshot, doc: Effects["documents"][number]) {
        const p = s.property, c = p.config, f = record(s, "guest_account", doc.guestAccountId), guest = record(s, "guest", f.data.guestId);
        const entries = records(s, "entry").filter((r) => r.data.guestAccountId === f.id);
        const now = ports.clock.now();
        let financeId: string | null = null;
        const party = f.data.partyId || `pms-guest-${guest.id}`;
        if (!f.data.partyId)
            await db.insertFinanceParties({ party: party, companyId: p.companyId, fact2: `PMS-${guest.id}`, name: guest.data.name, currency: c.currency, now: now, now2: now });
        if (doc.kind === "invoice") {
            const bill = entries.filter((e) => ["charge", "adjustment", "transfer"].includes(e.data.kind) && e.data.method !== "opening");
            const total = bill.reduce((n, e) => n + e.data.total, 0), tax = bill.reduce((n, e) => n + e.data.tax, 0);
            const openingDue = entries.filter(entry => entry.data.method === "opening" && entry.data.kind === "charge").reduce((sum, entry) => sum + entry.data.total, 0);
            const currentDue = Math.min(total, Math.max(0, f.data.due - openingDue));
            if (total < 0 || tax < 0)
                throw ports.error(409, "PMS_INVOICE_NEGATIVE", "Review adjustments before issuing the document");
            const journal = await db.readPmsPostings({ id: p.id, id2: p.id, id3: f.id });
            const credit = f.data.creditId
                ? record(s, "credit", f.data.creditId).data
                : null;
            await db.insertFinanceInvoices({ id: doc.id, companyId: p.companyId, party: party, fact3: `PMS-I-${doc.id}`, businessDate: p.businessDate, fact5: plusDays(p.businessDate, credit?.terms ?? 0), currency: c.currency, fact7: total - tax, tax: tax, total: total, currentDue: currentDue, receivable: c.accounts.receivable, revenue: c.accounts.revenue, tax2: c.accounts.tax, id2: f.id, fact15: currentDue ? "posted" : "paid", fact16: journal?.journal_id ?? null, userId: actor.userId, approvedBy: c.approvedBy, now: now, now2: now, now3: now });
            financeId = doc.id;
        }
        else if (doc.kind === "receipt" || doc.kind === "refund") {
            const item = record(s, "entry", doc.entryId!).data;
            const posting = await db.readPmsPostings2({ id: p.id, entryId: doc.entryId });
            await db.insertFinancePayments({ id: doc.id, companyId: p.companyId, party: party, fact3: doc.kind === "refund" ? "refund" : "receipt", fact4: `PMS-R-${doc.id}`, businessDate: p.businessDate, currency: c.currency, amount: doc.amount, fact8: item.method === 'transfer' ? c.accounts.bank : item.method === 'cash' ? c.accounts.cash : c.accounts.edc, fact9: posting?.journal_id ?? null, userId: actor.userId, userId2: actor.userId, now: now, now2: now });
            financeId = doc.id;
        }
        await db.insertPmsDocuments({ id: doc.id, id2: p.id, id3: f.id, kind: doc.kind, financeId: financeId, fact5: JSON.stringify({
                property: {
                    name: p.name,
                    country: c.country,
                    currency: c.currency,
                    branchId: p.branchId,
                },
                guest: guest.data,
                guestAccount: f.data,
                entries: doc.entryId
                    ? entries.filter((e) => e.id === doc.entryId)
                    : entries,
                amount: doc.amount,
                businessDate: p.businessDate,
                issuedBy: actor.userId,
                accountingRevision: c.revision,
            }), now: now });
    }
    async function syncAllocations(db: PmsUnit<Effect>, actor: WorkContext, s: Snapshot, guestAccountId: string) {
        const f = record(s, "guest_account", guestAccountId);
        const opening = (await db.readPmsImportBalances({ id: s.property.id, guestAccountId: guestAccountId })).results;
        if (!f.data.invoiceId && !opening.length)
            return;
        const entries = records(s, "entry").filter((e) => e.data.guestAccountId === guestAccountId), refunds = new Map<string, number>(), applied = new Map<string, number>();
        for (const e of entries) {
            const target = (e.data.kind === "refund" && e.data.description !== "payment.refund_settle") || e.data.description === "payment.refund_liability"
                ? refunds
                : e.data.reference === "deposit-application"
                    ? applied
                    : null;
            if (target)
                for (const part of e.data.funding ?? []) {
                    if ((e.data.kind === "refund" || e.data.description === "payment.refund_liability") &&
                        record(s, "entry", part.entryId).data.kind === "deposit" &&
                        (e.data.refundPocket ?? record(s, "refund", e.data.originalId).data.pocket) === "deposit")
                        continue;
                    target.set(part.entryId, (target.get(part.entryId) ?? 0) + part.amount);
                }
        }
        const receipts = await db.readPmsDocuments2({ id: s.property.id, guestAccountId: guestAccountId });
        let remaining = Math.max(0, entries
            .filter((e) => ["charge", "adjustment", "transfer"].includes(e.data.kind))
            .reduce((n, e) => n + e.data.total, 0) - f.data.due);
        const currentTotal = entries.filter(e => ["charge", "adjustment", "transfer"].includes(e.data.kind) && e.data.method !== "opening").reduce((sum, e) => sum + e.data.total, 0);
        const targets = [...opening, ...(f.data.invoiceId ? [{ id: f.data.invoiceId, total: currentTotal }] : [])].map(target => {
            const paid = Math.min(remaining, Number(target.total));
            remaining -= paid;
            return { ...target, total: Number(target.total), paid, remaining: paid };
        });
        for (const target of targets) {
            await db.deleteFinanceAllocations({ id: target.id, companyId: s.property.companyId });
            const outstanding = target.total - target.paid;
            await db.updateFinanceInvoices({ outstanding: outstanding, outstanding2: outstanding, outstanding3: outstanding, outstanding4: outstanding, fact4: ports.clock.now(), id: target.id, companyId: s.property.companyId });
        }
        for (const r of receipts) {
            const source = parseGuestAccountJson(r.snapshot_json).entries?.[0];
            if (!source || !["payment", "deposit"].includes(source.data.kind))
                continue;
            const available = source.data.kind === "deposit"
                ? (applied.get(source.id) ?? 0)
                : Number(source.data.total);
            const held = (f.data.refundHolds ?? []).filter(p => p.entryId === source.id && p.pocket === "advance").reduce((n, p) => n + p.amount, 0);
            let budget = Math.max(0, available - (refunds.get(source.id) ?? 0) - held);
            for (const target of targets) {
                const n = Math.min(target.remaining, budget);
                if (n <= 0)
                    continue;
                await db.insertFinanceAllocations({ fact0: ports.uuid(), companyId: s.property.companyId, id: r.id, id2: target.id, n: n, userId: actor.userId, fact6: ports.clock.now() });
                target.remaining -= n;
                budget -= n;
            }
        }
        if (targets.some(target => target.remaining > 0))
            throw ports.error(409, "PMS_ALLOCATION_TRACE", "Allocation does not match payment evidence");
    }
    async function validateReferences(db: PmsUnit<Effect>, s: Snapshot, operation: string, input: unknown) {
        // Guest mutations introduce no party, stock or accounting references.
        if(operation === 'guest.save' || operation === 'guest.service_request') return;
        const value = input as Record<string, unknown> | null;
        if (operation === "task.assign" && value?.assigneeId) {
            if (!(await db.readAssignments2({ assigneeId: value.assigneeId, companyId: s.property.companyId, fact2: ports.clock.now(), fact3: ports.clock.now(), branchId: s.property.branchId })))
                throw ports.error(422, "PMS_ASSIGNEE_SCOPE", "Owner is not authorized for this hotel");
        }
        if(housekeepingCommands.has(operation))return; // Only room/task/occupancy facts; no economic references.
        for (const partyId of new Set([
            ...records(s, "rate").map((r) => r.data.partyId),
            ...records(s, "credit").map((r) => r.data.partyId),
        ].filter(Boolean))) {
            if (!(await db.readFinanceParties2({ partyId: partyId, companyId: s.property.companyId, currency: s.property.config.currency })))
                throw ports.error(422, "PMS_PARTY_INVALID", "Company/agent must be an active customer in this company");
        }
        for (const item of records(s, "item"))
            if (!(await db.readCommerceProducts2({ warehouseId: item.data.warehouseId, productId: item.data.productId, companyId: s.property.companyId, branchId: s.property.branchId })))
                throw ports.error(422, "PMS_ITEM_SCOPE", "Product and warehouse must belong to this company and branch");
    }
    async function pmsCommand(db: PmsRepository<Effect>, actor: WorkContext, raw: unknown, key: string, worker = false) {
        const command = commandSchema.parse(canonicalGuestAccount(raw)), op = command.operation;
        if (op === "charge.import_pos")
            throw ports.error(403, "PMS_INTERNAL_COMMAND", "Restaurant charges must come from a POS transaction");
        if (ports.environment("NODE_ENV") === 'production' && (op === 'payment.receive' || op === 'payment.deposit' || op === 'refund.pay')) {
            const method = z.object({ method: z.string() }).parse(command.input).method;
            if (method !== 'cash')
                throw ports.error(403, 'PMS_BANK_VERIFICATION_REQUIRED', 'Hotel bank and card payments or refunds require verified evidence before posting');
        }
        ports.require(actor, operationPermission(op));
        if (op === "accounting.bootstrap")
            ports.require(actor, "finance.write");
        if (op === "night.step" && !worker)
            throw ports.error(403, "PMS_WORKER_ONLY", "This command is only used by day-close work");
        if (actor.departmentId || actor.beta)
            throw ports.error(403, "PMS_BRANCH_REQUIRED", "Select a company or branch area");
        const target = op === 'property.create' ? `property-${ports.hash(key).slice(0,32)}`
            : command.propertyId??`property-${ports.hash(key).slice(0,32)}`;
        try {
            return await db.transaction(async (tx) => {
                await tx.assertProductionModuleEnabled(actor.companyId, 'pms');
                // Existing properties install the transaction-local write flag in
                // their scoped lock query. Creation has no Property row yet.
                if (op === "property.create") await tx.readContext({});
                const old = await tx.replay<{
                    id: string;
                    propertyId: string;
                    version: number;
                    entryId?: string;
                    state?: string;
                    transferVersion?: number;
                    bankTransactionId?: string | null;
                }>(actor, key, `pms.${op}`, target, command);
                if (old)
                    return old;
                let s: Snapshot;
                if (op === "property.create") {
                    const v = z
                        .object({
                        name: z.string().trim().min(1).max(200),
                        branchId: idField,
                        businessDate: dateField,
                    })
                        .parse(command.input);
                    if (actor.branchId && actor.branchId !== v.branchId)
                        throw ports.error(404, "PMS_BRANCH_NOT_FOUND", "Branch not found");
                    if (!(await tx.readOrganizationNodes2({ branchId: v.branchId, companyId: actor.companyId })))
                        throw ports.error(422, "PMS_BRANCH_NOT_FOUND", "Select a branch of this company");
                    s = newProperty(target, actor.companyId, v.branchId, v.name, v.businessDate);
                    await tx.insertPmsProperties({ target: target, companyId: actor.companyId, branchId: v.branchId, name: v.name, businessDate: v.businessDate, fact5: JSON.stringify(s.property.config), userId: actor.userId, fact7: ports.clock.now() });
                }
                else {
                    if (!command.propertyId)
                        throw ports.error(422, "PMS_PROPERTY_REQUIRED", "Select a hotel");
                    s = ['guest.save','guest.service_request'].includes(op)
                        ? await loadGuestCommand(tx,actor,command.propertyId,op,command.input,worker?undefined:command.expectedVersion??null)
                        : inventoryCommands.has(op)?await loadInventoryCommand(tx,actor,command.propertyId,worker?undefined:command.expectedVersion??null)
                        : op==='reservation.create'?await loadReservationCommand(tx,actor,command.propertyId,command.input,worker?undefined:command.expectedVersion??null)
                        : stayCommands.has(op)?await loadStayCommand(tx,actor,command.propertyId,op,command.input,worker?undefined:command.expectedVersion??null)
                        : cashierCommands.has(op)?await loadCashierCommand(tx,actor,command.propertyId,op,command.input,worker?undefined:command.expectedVersion??null)
                        : nightCommands.has(op)?await loadNightCommand(tx,actor,command.propertyId,op,command.input,worker?undefined:command.expectedVersion??null)
                        : housekeepingCommands.has(op)?await loadHousekeepingCommand(tx,actor,command.propertyId,op,command.input,worker?undefined:command.expectedVersion??null):await loadPms(tx, actor, command.propertyId, true,undefined,worker?undefined:command.expectedVersion??null);
                    if (op === "charge.reverse") {
                        const entryId = z.object({ id: idField }).parse(command.input).id;
                        const entry = records(s, "entry").find((r) => r.id === entryId);
                        if (entry?.data.sourceKey?.startsWith("pos:"))
                            throw ports.error(409, "PMS_POS_REVERSAL_REQUIRED", "Reverse restaurant charges from the linked POS bill");
                    }
                    if(!worker)assertExpectedVersion(s.property,command.expectedVersion??null);
                }
                if (op === "charge.reverse")
                    throw ports.error(409, "PMS_CORRECTION_REQUEST_REQUIRED", "Request a charge correction and have another authorized person review it.");
                if (op === "charge.correction.request") {
                    const input = z.object({ id: idField, reason: z.string().trim().min(3).max(2000) }).strict().parse(command.input);
                    if (s.property.state !== "open")
                        throw ports.error(409, "PMS_DAY_CLOSING", "Charge corrections require an open hotel day.");
                    const entry = records(s, "entry").find(row => row.id === input.id);
                    if (!entry || !["charge", "transfer"].includes(entry.data.kind) || entry.data.total <= 0)
                        throw ports.error(409, "PMS_CORRECTION_ENTRY_INVALID", "Only a positive hotel guest account charge or transfer can be corrected.");
                    if (entry.data.sourceKey?.startsWith("pos:"))
                        throw ports.error(409, "PMS_POS_REVERSAL_REQUIRED", "Reverse restaurant charges from the linked POS bill.");
                    if (records(s, "entry").some(row => row.data.originalId === entry.id))
                        throw ports.error(409, "PMS_ALREADY_REVERSED", "This guest account entry already has a reversal.");
                    if (await tx.readPmsChargeCorrections2({ id: entry.id }))
                        throw ports.error(409, "PMS_CORRECTION_ALREADY_PENDING", "Another correction request for this entry is awaiting review.");
                    const id = `pms-correction-${ports.hash(key).slice(0, 32)}`;
                    const now = ports.clock.now(), nextVersion = s.property.version + 1;
                    const result = { id, propertyId: s.property.id, version: nextVersion, status: "pending", correctionVersion: 1 };
                    return tx.execute(actor, key, `pms.${op}`, command, target, nextVersion, result, [
                        tx.insertPmsChargeCorrections({ id: id, companyId: actor.companyId, id2: s.property.id, id3: entry.id, guestAccountId: entry.data.guestAccountId, reason: input.reason, userId: actor.userId, now: now, now2: now }),
                        tx.insertPmsChargeCorrectionEvents({ fact0: `pms-correction-event-${key}`, id: id, companyId: actor.companyId, id2: s.property.id, id3: entry.id, userId: actor.userId, reason: input.reason, now: now }),
                        tx.updatePmsProperties({ nextVersion: nextVersion, now: now, id: s.property.id, companyId: actor.companyId, version: s.property.version }),
                    ], { companyId: s.property.companyId, branchId: s.property.branchId, departmentId: null });
                }
                if (op === "charge.correction.review") {
                    const input = z.object({ id: idField, expectedVersion: z.number().int().positive(), action: z.enum(["approve", "reject"]), reason: z.string().trim().min(3).max(2000) }).strict().parse(command.input);
                    if (s.property.state !== "open")
                        throw ports.error(409, "PMS_DAY_CLOSING", "Review charge corrections before closing the hotel day.");
                    const correction = await tx.readPmsChargeCorrections3({ id: input.id, companyId: actor.companyId, id2: s.property.id });
                    if (!correction)
                        throw ports.error(404, "PMS_CORRECTION_NOT_FOUND", "Charge correction request not found in this hotel.");
                    if (correction.status !== "pending" || Number(correction.version) !== input.expectedVersion)
                        throw ports.error(409, "PMS_CORRECTION_CHANGED", "This correction request changed. Reload before deciding.");
                    if (samePerson(actor, correction.requested_by))
                        throw ports.error(403, "PMS_INDEPENDENT_REVIEW_REQUIRED", "A different person must review the charge correction.");
                    const entry = records(s, "entry").find(row => row.id === correction.entry_id);
                    if (!entry || entry.data.guestAccountId !== correction.guest_account_id || entry.data.sourceKey?.startsWith("pos:") || records(s, "entry").some(row => row.data.originalId === correction.entry_id))
                        throw ports.error(409, "PMS_CORRECTION_SOURCE_CHANGED", "The guest account entry changed or already has a reversal.");
                    const propertyVersion = s.property.version, now = ports.clock.now(), correctionVersion = Number(correction.version) + 1;
                    let reversalEntryId: string | null = null, propertyVersionAfter = propertyVersion;
                    if (input.action === "approve") {
                        const original = new Map(s.records.map(row => [row.id, JSON.stringify(row)]));
                        let outcome;
                        try {
                            await hydrateReceiptAllocationCredits(tx,s);
                            outcome = executePms(s, "charge.reverse", { id: entry.id, reason: input.reason }, { userId: actor.userId, now, uuid: ports.uuid, independent: other => !samePerson(actor, other) });
                        }
                        catch (error) {
                            if (error instanceof PmsError)
                                throw ports.error(error.status, error.code, error.message);
                            throw error;
                        }
                        reversalEntryId = outcome.id;
                        propertyVersionAfter = s.property.version;
                        await validSetup(tx, s);
                        await persistPmsRecords(tx, s, original);
                        await effects(tx, actor, s, outcome.effects);
                    }
                    else
                        propertyVersionAfter++;
                    const result = { id: correction.id, propertyId: s.property.id, version: propertyVersionAfter, status: input.action === "approve" ? "approved" : "rejected", correctionVersion, reversalEntryId };
                    return tx.execute(actor, key, `pms.${op}`, command, target, propertyVersionAfter, result, [
                        tx.updatePmsChargeCorrections({ status: result.status, correctionVersion: correctionVersion, userId: actor.userId, reason: input.reason, reversalEntryId: reversalEntryId, now: now, now2: now, id: correction.id, companyId: actor.companyId, id2: s.property.id, expectedVersion: input.expectedVersion }),
                        tx.insertPmsChargeCorrectionEvents2({ fact0: `pms-correction-event-${key}`, id: correction.id, companyId: actor.companyId, id2: s.property.id, entry_id: correction.entry_id, status: result.status, userId: actor.userId, correctionVersion: correctionVersion, reason: input.reason, reversalEntryId: reversalEntryId, now: now }),
                        tx.updatePmsProperties2({ propertyVersionAfter: propertyVersionAfter, now: now, id: s.property.id, companyId: actor.companyId, propertyVersion: propertyVersion }),
                    ], { companyId: s.property.companyId, branchId: s.property.branchId, departmentId: null });
                }
                let attachmentBinding: PmsAttachmentBinding<Effect> | null = null;
                if (command.attachmentSetIds.length) {
                    if (!["task.complete", "lost.release", "ota.settle"].includes(op))
                        throw ports.error(422, "PMS_ATTACHMENT_ACTION", "Files can be bound only to room-task completion, lost-property release, or OTA settlement.");
                    const targetId = op === "ota.settle"
                        ? s.property.id
                        : z.object({ id: idField }).parse(command.input).id;
                    const targetRecord = op === "task.complete"
                        ? records(s, "task").find((row) => row.id === targetId)
                        : op === "lost.release"
                            ? records(s, "lost").find((row) => row.id === targetId)
                            : undefined;
                    const descriptor = op === "task.complete"
                        ? {
                            targetType: "pms_task",
                            purposes: new Set(["room_photo", "damage_photo"]),
                            bindingType: "pms_task_completion",
                            expectedTargetVersion: targetRecord?.version,
                        }
                        : op === "lost.release"
                            ? {
                                targetType: "pms_lost_property",
                                purposes: new Set(["handover_evidence", "lost_property_photo"]),
                                bindingType: "pms_lost_property_release",
                                expectedTargetVersion: targetRecord?.version,
                            }
                            : op === "ota.settle"
                                ? {
                                    targetType: "pms_property",
                                    purposes: new Set(["ota_reconciliation"]),
                                    bindingType: "pms_ota_settlement",
                                    expectedTargetVersion: s.property.version,
                                }
                                : null;
                    if (!descriptor)
                        throw ports.error(422, "PMS_ATTACHMENT_ACTION", "Files can be bound only to room-task completion, lost-property release, or OTA settlement.");
                    if (descriptor.expectedTargetVersion === undefined)
                        throw ports.error(404, "PMS_ATTACHMENT_TARGET", "Attachment target not found in this hotel");
                    const sets = (await tx.readAttachmentSets({ companyId: actor.companyId, attachmentSetIds: command.attachmentSetIds })).results;
                    if (sets.length !== new Set(command.attachmentSetIds).size ||
                        sets.some((set) => set.component_kind !== "action" ||
                            set.component_ref !== op ||
                            !descriptor.purposes.has(set.purpose_key)))
                        throw ports.error(422, "PMS_ATTACHMENT_PURPOSE", "Choose only evidence created for this PMS action.");
                    attachmentBinding = await tx.prepareAttachmentBinding(actor, {
                        attachmentSetIds: command.attachmentSetIds,
                        bindingContract: {
                            targetType: descriptor.targetType,
                            bindingType: descriptor.bindingType,
                            contexts: [...descriptor.purposes].map(purposeKey => ({ purposeKey, componentKind: "action" as const, componentRef: op })),
                        },
                        targetId,
                        expectedTargetVersion: descriptor.expectedTargetVersion,
                        commandId: key,
                        bindingId: `${targetId}:${op}:${descriptor.expectedTargetVersion + 1}`,
                    });
                    await tx.batch(attachmentBinding.guards);
                }
                if (op === 'payment.request_transfer') {
                    if (s.property.state !== 'open')
                        throw ports.error(409, 'PMS_DAY_CLOSING', 'The hotel day is not open for a new transfer');
                    const v = z.object({ guestAccountId: idField, shiftId: idField, amount: z.number().int().positive().max(99999999999), reference: z.string().trim().min(2).max(150), kind: z.enum(['receipt', 'deposit']).default('receipt') }).parse(command.input);
                    const guestAccount = records(s, 'guest_account').find(row => row.id === v.guestAccountId);
                    if (!guestAccount || guestAccount.data.state === 'closed' || guestAccount.data.invoiceId)
                        throw ports.error(409, 'PMS_GUEST_ACCOUNT_CLOSED', 'The selected guest account cannot receive a transfer');
                    const shift = records(s, 'shift').find(row => row.id === v.shiftId);
                    if (!shift || shift.data.state !== 'open' || shift.data.userId !== actor.userId || shift.data.date !== s.property.businessDate)
                        throw ports.error(409, 'PMS_SHIFT_REQUIRED', 'Open your hotel cashier shift before recording a transfer');
                    const id = ports.uuid(), now = ports.clock.now();
                    const result = { id, propertyId: s.property.id, version: s.property.version, state: 'pending', transferVersion: 1 };
                    return tx.execute(actor, key, `pms.${op}`, command, target, s.property.version, result, [
                        tx.insertPmsTransferRequests({ id: id, companyId: actor.companyId, id2: s.property.id, guestAccountId: v.guestAccountId, shiftId: v.shiftId, kind: v.kind, businessDate: s.property.businessDate, currency: s.property.config.currency, amount: v.amount, reference: v.reference, userId: actor.userId, now: now, now2: now }),
                    ], { companyId: s.property.companyId, branchId: s.property.branchId, departmentId: null });
                }
                if (op === 'payment.cancel_transfer') {
                    const v = z.object({ id: idField, expectedTransferVersion: z.number().int().positive() }).parse(command.input);
                    const row = await tx.readPmsTransferRequests2({ id: v.id, companyId: actor.companyId, id2: s.property.id });
                    if (!row)
                        throw ports.error(404, 'PMS_TRANSFER_NOT_FOUND', 'Transfer request not found in this hotel');
                    if (Number(row.version) !== v.expectedTransferVersion || row.state !== 'pending')
                        throw ports.error(409, 'PMS_TRANSFER_CHANGED', 'Transfer state changed. Reload before cancelling');
                    if (row.recorded_by !== actor.userId && !hasPermission(actor, 'pms.approve'))
                        throw ports.error(403, 'PMS_TRANSFER_OWNER', 'Only the recording cashier or a hotel manager can cancel this request');
                    const result = { id: row.id, propertyId: s.property.id, version: s.property.version, state: 'cancelled', transferVersion: Number(row.version) + 1 };
                    return tx.execute(actor, key, `pms.${op}`, command, target, s.property.version, result, [
                        tx.updatePmsTransferRequests({ userId: actor.userId, fact1: ports.clock.now(), id: row.id, version: row.version }),
                    ], { companyId: s.property.companyId, branchId: s.property.branchId, departmentId: null });
                }
                let verifiedTransfer: TransferRequestRow | null = null;
                let matchedBankId: string | null = null;
                let engineOp = op, engineInput = command.input;
                if (op === "accounting.bootstrap") {
                    if (records(s, "entry").length)
                        throw ports.error(409, "PMS_ACCOUNTING_IN_USE", "Accounting is frozen after financial activity.");
                    engineInput = await tx.preparePmsAccounting(actor, s, command.input);
                    engineOp = "accounting.save";
                }
                if (op === 'reservation.create' && command.input && typeof command.input === 'object') {
                    const requestedCurrency = (command.input as Record<string, unknown>).currency;
                    if (typeof requestedCurrency === 'string') {
                        if (requestedCurrency !== s.property.config.currency)
                            throw ports.error(422, 'PMS_CURRENCY_SETUP_REQUIRED', `The selected ${requestedCurrency} currency needs a hotel rate plan and supported ledger/payment accounts before booking.`);
                        if (requestedCurrency !== 'USD' && !await tx.readCompanyFxRates({ companyId: actor.companyId, requestedCurrency: requestedCurrency }))
                            throw ports.error(422, 'MANUAL_RATE_REQUIRED', `Confirm a manual USD/${requestedCurrency} rate before booking.`);
                    }
                    const campaignCode = (command.input as Record<string, unknown>).campaignCode;
                    if (typeof campaignCode === 'string' && campaignCode.trim()) {
                        const campaign = await tx.readMarketingCampaigns({ companyId: actor.companyId, fact1: campaignCode.trim() });
                        if (!campaign)
                            throw ports.error(422, 'PMS_CAMPAIGN_UNKNOWN', 'Select a campaign code registered for this company.');
                    }
                }
                if (op === 'payment.verify_transfer') {
                    if (s.property.state !== 'open')
                        throw ports.error(409, 'PMS_DAY_CLOSING', 'The hotel day must be open to verify a transfer');
                    const v = z.object({ id: idField, expectedTransferVersion: z.number().int().positive(), bankTransactionId: idField, bankVersion: z.number().int().positive() }).parse(command.input);
                    const row = await tx.readPmsTransferRequests2({ id: v.id, companyId: actor.companyId, id2: s.property.id });
                    if (!row)
                        throw ports.error(404, 'PMS_TRANSFER_NOT_FOUND', 'Transfer request not found in this hotel');
                    if (Number(row.version) !== v.expectedTransferVersion || row.state !== 'pending')
                        throw ports.error(409, 'PMS_TRANSFER_CHANGED', 'Transfer state changed. Reload before verifying');
                    if (samePerson(actor, row.recorded_by))
                        throw ports.error(403, 'PMS_INDEPENDENT_VERIFICATION', 'A different Finance user must verify the bank transfer');
                    const bank = await tx.updateFinanceBankTransactions({ bankTransactionId: v.bankTransactionId, companyId: actor.companyId });
                    if (!bank || Number(bank.version) !== v.bankVersion || !['unmatched', 'proposed'].includes(bank.match_status)
                        || bank.bank_account !== s.property.config.accounts.bank || bank.currency !== row.currency
                        || Number(bank.amount_minor) !== Number(row.amount_minor) || bank.external_id.trim() !== row.reference
                        || bank.transaction_date !== row.business_date || s.property.businessDate !== row.business_date)
                        throw ports.error(409, 'PMS_BANK_EVIDENCE_MISMATCH', 'Match the exact imported bank account, reference, business date, currency, and amount');
                    verifiedTransfer = row;
                    matchedBankId = bank.id;
                    engineOp = row.kind === 'deposit' ? 'payment.deposit' : 'payment.receive';
                    engineInput = { guestAccountId: row.guest_account_id, shiftId: row.shift_id, amount: Number(row.amount_minor), method: 'transfer', reference: row.reference };
                }
                if (op === 'night.start') {
                    const controls = await tx.hospitalityDayControls(actor.companyId, s.property.branchId, s.property.id, s.property.businessDate);
                    if (controls.blockers.length)
                        throw ports.error(409, 'PMS_DAY_RECONCILIATION_BLOCKED', controls.blockers.join(' · '));
                }
                const original = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
                if (op === 'accounting.save' || op === 'accounting.bootstrap')
                    if (await tx.readPmsBookingPayments({ id: s.property.id }))
                        throw ports.error(409, 'PMS_PAYMENT_PENDING', 'Accounting is frozen while online payment is pending');
                if (op === 'guest_account.merge') {
                    const v = command.input as {
                        id: string;
                        toGuestAccountId: string;
                    };
                    if (await tx.readPmsBookingAllocations({ id: v.id, toGuestAccountId: v.toGuestAccountId }))
                        throw ports.error(409, 'PMS_ONLINE_RECEIPT_FROZEN', 'Keep online receipt guest accounts separate to preserve their immutable allocation');
                }
                if (op.startsWith("stay.") || op.startsWith("reservation.") || op.startsWith("guest_account.")) {
                    const targetId = command.input && typeof command.input === "object" ? (command.input as {
                        id?: string;
                    }).id : undefined;
                    const stay = records(s, "stay").find(row => row.id === targetId);
                    const guestAccount = records(s, "guest_account").find(row => row.id === targetId);
                    const reservationId = stay?.data.reservationId ?? guestAccount?.data.reservationId ?? targetId;
                    if (reservationId && await tx.readPmsBookingPayments2({ reservationId: reservationId }))
                        throw ports.error(409, "PMS_PAYMENT_PENDING", "Room and price details are frozen while online payment is pending");
                    if (["stay.change_dates", "stay.cancel", "reservation.cancel"].includes(op) && (op !== "stay.change_dates" || stay?.data.state !== "in_house") && reservationId &&
                        await tx.readPmsBookingPayments3({ reservationId: reservationId }))
                        throw ports.error(409, "PMS_BOOKING_REVISION_REQUIRED", "Review the booking change and its payment difference before confirming it");
                }
                await hydrateReceiptAllocationCredits(tx,s);
                const outcome = op === "property.create"
                    ? {
                        id: s.property.id,
                        effects: {
                            postings: [],
                            stock: [],
                            documents: [],
                            sources: [],
                        } as Effects,
                    }
                    : executePms(s, engineOp, engineInput, {
                        userId: actor.userId,
                        now: ports.clock.now(),
                        uuid: ports.uuid,
                        independent: (other) => !samePerson(actor, other),
                        verifiedBank: Boolean(verifiedTransfer),
                    });
                if (op === "booking.save" && s.property.config.booking?.published) {
                    if (!ports.environment("RESEND_API_KEY") || !ports.environment("RESEND_FROM"))
                        throw ports.error(422, "PMS_EMAIL_REQUIRED", "Configure booking email delivery before publishing");
                    const booking = s.property.config.booking;
                    if (booking.rateIds.some(id => (record(s, "rate", id).data.collection?.cancellationFeePercent ?? 0) > 0) && !s.property.config.bookingFees?.approvedBy)
                        throw ports.error(422, "PMS_FEE_ACCOUNT_REQUIRED", "Finance must approve cancellation fee accounting before publishing these rates");
                    for (const [targetType, targetId, images] of [["pms_property", s.property.id, booking.images ?? []], ...booking.roomContent.map(room => ["pms_room_type", room.typeId, room.images ?? []])] as Array<[
                        string,
                        string,
                        import("@/server/modules/pms/public/rules/pms-model").BookingImage[]
                    ]>) {
                        for (const image of images)
                            if (!await tx.readAttachmentPublications({ publicationId: image.publicationId, companyId: actor.companyId, targetType: targetType, targetId: targetId }))
                                throw ports.error(422, "PMS_IMAGE_NOT_PUBLIC", "Choose independently approved booking images from this hotel or room type");
                    }
                    if (booking.rateIds.some(id => record(s, "rate", id).data.collection?.method && record(s, "rate", id).data.collection?.method !== "pay_at_hotel")) {
                        if (!s.property.config.onlineAccounting?.approvedBy)
                            throw ports.error(422, "PMS_ONLINE_ACCOUNTING_REQUIRED", "Finance must approve online liability accounts before publishing prepaid rates");
                        const connection = await tx.readPosConnections3({ companyId: actor.companyId });
                        if (!connection || connection.livemode && ports.environment("PMS_STRIPE_LIVE_ENABLED") !== "true")
                            throw ports.error(422, "PMS_PAYMENT_UNAVAILABLE", "Complete Company Stripe sandbox setup before publishing prepaid rates");
                    }
                }
                await validateReferences(tx, s, engineOp, engineInput);
                if ([
                    "accounting.save",
                    "accounting.bootstrap",
                    "accounting.approve",
                    "online_accounting.prepare",
                    "online_accounting.approve",
                    "property.enable",
                    "night.start",
                    "night.resume",
                ].includes(op) ||
                    outcome.effects.postings.length ||
                    outcome.effects.stock.length)
                    await validSetup(tx, s);
                if (op === "booking.retry_email") {
                    const emailId = z.object({ id: idField }).parse(command.input).id;
                    const retry = await tx.updatePmsBookingEmails({ fact0: ports.clock.now(), emailId: emailId, id: s.property.id });
                    if (!retry)
                        throw ports.error(409, "PMS_EMAIL_NOT_FAILED", "Only failed hotel emails can be retried");
                }
                const reservationQuote = op === 'reservation.create' && records(s, 'reservation').some(row => row.id === outcome.id && row.data.state === 'confirmed')
                    ? [{ sourceType: 'pms_reservation', sourceId: outcome.id, amountKind: 'room_total',
                        transactionDate: s.property.businessDate, currency: s.property.config.currency, currencyScale: 2,
                        amountUnits: String(records(s, 'stay').filter(row => row.data.reservationId === outcome.id)
                            .reduce((sum, row) => sum + row.data.prices.reduce((nightSum, night) => nightSum + night.total, 0), 0)) }]
                    : [];
                await persistPmsRecords(tx, s, original, reservationQuote);
                if (op === "guest_account.merge") {
                    const v = command.input as {
                        id: string;
                        toGuestAccountId: string;
                    };
                    await tx.updatePmsDocuments({ toGuestAccountId: v.toGuestAccountId, id: s.property.id, id2: v.id });
                }
                await effects(tx, actor, s, outcome.effects);
                if (verifiedTransfer && matchedBankId) {
                    const receipt = outcome.effects.documents.find(doc => doc.kind === 'receipt' && doc.entryId === outcome.id);
                    if (!receipt)
                        throw ports.error(409, 'PMS_RECEIPT_MISSING', 'The verified transfer did not produce a receipt');
                    await tx.updatePmsTransferRequests2({ userId: actor.userId, matchedBankId: matchedBankId, id: outcome.id, id2: receipt.id, fact4: ports.clock.now(), id3: verifiedTransfer.id });
                    await tx.updateFinanceBankTransactions2({ id: receipt.id, matchedBankId: matchedBankId });
                }
                await tx.updatePmsProperties3({ name: s.property.name, businessDate: s.property.businessDate, state: s.property.state, fact3: JSON.stringify(s.property.config), version: s.property.version, fact5: ports.clock.now(), id: s.property.id });
                if (outcome.effects.nightJob)
                    await tx.insertPmsJobs({ nightJob: outcome.effects.nightJob, id: s.property.id, businessDate: s.property.businessDate, fact3: JSON.stringify(actor), fact4: ports.clock.now() });
                if (op === "night.step")
                    await tx.updatePmsJobs({ fact0: s.property.state === "open" ? "completed" : "pending", fact1: ports.clock.now(), id: outcome.id });
                const result = verifiedTransfer
                    ? { id: verifiedTransfer.id, propertyId: s.property.id, version: s.property.version, state: 'verified', transferVersion: Number(verifiedTransfer.version) + 1, entryId: outcome.id, bankTransactionId: matchedBankId }
                    : { id: outcome.id, propertyId: s.property.id, version: s.property.version };
                return tx.execute(actor, key, `pms.${op}`, command, target, s.property.version, result, attachmentBinding?.steps ?? [], {
                    companyId: s.property.companyId,
                    branchId: s.property.branchId,
                    departmentId: null,
                });
            },target,{ actor, operation: op, input: command.input, key });
        }
        catch (error) {
            if (error instanceof PmsError)
                throw ports.error(error.status, error.code, error.message);
            if (["40001", "40P01"].includes((error as {code?:string}).code??''))
                throw ports.error(409,'PMS_RETRYABLE_CONFLICT','Concurrent hotel work did not commit. Reload and retry with the same Idempotency-Key.');
            if (["23505", "23514"].includes((error as {
                code?: string;
            }).code ?? ""))
                throw ports.error(409, "PMS_CONFLICT", "Data or balances changed. Load the latest version");
            throw error;
        }
    }
    async function drainPms(db: PmsRepository<Effect>, propertyId?: string, jobId?: string) {
        const jobs = await db.readPmsJobs({ fact0: propertyId ?? null, fact1: propertyId ?? null, fact2: jobId ?? null, fact3: jobId ?? null });
        for (const job of jobs) {
            try {
                const actor = parseGuestAccountJson(job.actor_json) as WorkContext;
                // User-originated jobs retain their original authority. Session/role revocation
                // blocks posting; a current Night Auditor can explicitly resume the frozen day.
                await pmsCommand(db, actor, {
                    operation: "night.step",
                    propertyId: job.property_id,
                    input: { id: job.id },
                }, ports.uuid(), true);
            }
            catch (error) {
                if (ports.isBusinessError(error) && error.code === "PMS_CONFLICT")
                    continue;
                const code = ports.isBusinessError(error) ? error.code : "PMS_JOB_FAILED";
                await db.transaction(async (tx) => {
                    await tx.readPmsProperties3({ property_id: job.property_id });
                    await tx.updatePmsJobs2({ code: code, fact1: ports.clock.now(), id: job.id });
                    await tx.updatePmsRecords({ code: code, fact1: ports.clock.now(), id: job.id });
                });
            }
        }
    }
    async function pmsWork(actor: WorkContext): Promise<WorkItem[]> {
        if (!(hasPermission(actor, "pms.read") || hasPermission(actor, "finance.read") && hasPermission(actor, "pms.reports")) || actor.departmentId || actor.beta)
            return [];
        const write = await ports.forActor(actor);
        return write.pmsOperationalWork(actor);
    }
    /** Same owner transaction and scoped Property lock. Source identity is global
     * within the hotel; money/refund/funding history of each target account is
     * complete, including archived rows and reversals outside that account. */
    async function loadPosCommand(tx:PmsUnit<Effect>,actor:WorkContext,id:string,expectedVersion:number,stayId:string,sourceKey:string):Promise<Snapshot>{
        const row=await tx.readPmsProperties({id,companyId:actor.companyId,lock:true});
        if(!row)throw ports.error(404,'PMS_NOT_FOUND','Hotel not found');
        const p=property(row);scope(actor,p);assertExpectedVersion(p,expectedVersion);
        return {property:p,records:await tx.readPmsPosCommand(id,stayId,sourceKey)};
    }
    /** Called inside the POS command transaction: no second command or ledger owner. */
    async function applyPosRoomCharge(tx: PmsUnit<Effect>, actor: WorkContext, raw: z.infer<typeof roomChargeSchema>, expectedPropertyVersion: number) {
        ports.require(actor, 'pos.sell');
        await tx.assertProductionModuleEnabled(actor.companyId, 'pms');
        const s = await loadPosCommand(tx,actor,raw.propertyId,expectedPropertyVersion,raw.stayId,`pos:${raw.sourceId}:${raw.sourceLineId}`);
        let prepared;
        try {
            prepared = prepareRoomCharge(s, raw);
        }
        catch (error) {
            if (error instanceof PmsError)
                throw ports.error(error.status, error.code, error.message);
            throw error;
        }
        if (prepared.duplicate)
            return { entryId: prepared.entryId, propertyVersion: s.property.version };
        const original = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
        let outcome;
        try {
            await hydrateReceiptAllocationCredits(tx,s);
            outcome = executePms(s, "charge.import_pos", {
                guestAccountId: prepared.guestAccountId,
                stayId: raw.stayId,
                sourceKey: prepared.sourceKey,
                description: raw.description,
                net: raw.net,
                service: raw.service,
                tax: raw.tax,
                total: raw.total,
            }, {
                userId: actor.userId,
                now: ports.clock.now(),
                uuid: ports.uuid,
                independent: (other) => !samePerson(actor, other),
            });
        }
        catch (error) {
            if (error instanceof PmsError)
                throw ports.error(error.status, error.code, error.message);
            throw error;
        }
        await validSetup(tx, s);
        await persistPmsRecords(tx, s, original);
        await effects(tx, actor, s, outcome.effects);
        await tx.updatePmsProperties4({ version: s.property.version, fact1: ports.clock.now(), id: s.property.id });
        return { entryId: outcome.id, propertyVersion: s.property.version };
    }
    /** POS owns the correction, so the guestAccount reversal shares its transaction. */
    async function reversePosRoomCharge(tx: PmsUnit<Effect>, actor: WorkContext, input: {
        propertyId: string;
        sourceKey: string;
        expectedPropertyVersion: number;
        reason: string;
        total: number;
    }) {
        ports.require(actor, 'pms.approve');
        const s = await loadPosCommand(tx,actor,input.propertyId,input.expectedPropertyVersion,'',input.sourceKey);
        const entry = records(s, "entry").find((r) => r.data.sourceKey === input.sourceKey);
        if (!entry || entry.data.total !== input.total)
            throw ports.error(409, "PMS_CHARGE_CHANGED", "The linked hotel charge changed. Review the bill again");
        const original = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
        let outcome;
        try {
            await hydrateReceiptAllocationCredits(tx,s);
            outcome = executePms(s, "charge.reverse", { id: entry.id, reason: input.reason }, {
                userId: actor.userId, now: ports.clock.now(), uuid: ports.uuid,
                independent: (other) => !samePerson(actor, other),
            });
        }
        catch (error) {
            if (error instanceof PmsError)
                throw ports.error(error.status, error.code, error.message);
            throw error;
        }
        await validSetup(tx, s);
        await persistPmsRecords(tx, s, original);
        await effects(tx, actor, s, outcome.effects);
        await tx.updatePmsProperties5({ version: s.property.version, fact1: ports.clock.now(), id: s.property.id });
        return { entryId: entry.id, reversalEntryId: outcome.id, businessDate: s.property.businessDate };
    }
    async function persistPmsRecords(tx: PmsUnit<Effect>, s: Snapshot, original: Map<string, string>, additionalQuotes: Parameters<PmsUnit<Effect>["captureMoneySnapshots"]>[1] = []) {
        const changed = s.records.filter(row => original.get(row.id) !== JSON.stringify(row))
            .map(row => ({ id: row.id, id2: s.property.id, kind: row.kind, fact3: JSON.stringify(row.data), version: row.version, updatedAt: row.updatedAt }));
        // Keep the original input order and every row trigger, while shortening
        // the locked transaction's round trips. All chunks and money evidence
        // still share the owner's transaction; no partial batch can commit.
        for (let offset = 0; offset < changed.length; offset += 500)
            await tx.insertPmsRecordsBatch(changed.slice(offset, offset + 500));
        await capturePmsMoneyRecords(tx, s, original, additionalQuotes);
        // Provider confirmation can persist several guestAccounts in one transaction. The
        // next flush compares with this committed-in-transaction state, not its first
        // in-memory snapshot, so earlier source evidence is never inserted twice.
        for (const row of s.records)
            original.set(row.id, JSON.stringify(row));
    }
    /** Owner rows and shared money evidence commit together, including imports and workers. */
    async function capturePmsMoneyRecords(tx: PmsUnit<Effect>, s: Snapshot, original: Map<string, string>, additionalQuotes: Parameters<PmsUnit<Effect>["captureMoneySnapshots"]>[1]) {
        const company = { companyId: s.property.companyId };
        const quotes: Array<Parameters<PmsUnit<Effect>["captureMoneySnapshots"]>[1][number]> = [];
        let collecting = true;
        const capture = async (sourceType: string, sourceId: string, amountKind: string, amount: number, date = s.property.businessDate) => {
            const input = { sourceType, sourceId, amountKind, amountUnits: String(amount),
                transactionDate: date, currency: s.property.config.currency, currencyScale: s.property.config.minorUnit };
            if (collecting) { quotes.push(input); return; }
            const value = await tx.snapshotMoney(company, input);
            await value.statement.run();
        };
        // Versioned prices remain the quote selected when booked or explicitly repriced.
        for (const stay of records(s, "stay")) {
            const previous = original.has(stay.id) ? parseGuestAccountJson(original.get(stay.id)!).data.prices : undefined;
            if (JSON.stringify(previous) === JSON.stringify(stay.data.prices))
                continue;
            for (const price of stay.data.prices) {
                // An upgrade keeps already accepted past nights and their original FX evidence.
                const unchanged = Array.isArray(previous) && previous.some((night: unknown) =>
                    JSON.stringify(night) === JSON.stringify(price));
                if (!unchanged)
                    await capture("pms_stay_price", `${stay.id}:${stay.version}`, price.date, price.total);
            }
        }
        for (const row of s.records) {
            if (original.get(row.id) === JSON.stringify(row) || row.kind === "entry")
                continue;
            const previous = original.has(row.id) ? parseGuestAccountJson(original.get(row.id)!).data as Record<string, unknown> : undefined;
            const values: Array<[
                string,
                number
            ]> = [];
            if (row.kind === "rate") {
                const data = record(s, "rate", row.id).data;
                values.push(["price", data.price], ["extra_adult", data.extraAdult]);
                for (const [index, band] of (data.childBands ?? []).entries())
                    values.push([`child_band:${index}`, band.price]);
                for (const date of data.calendar ?? [])
                    values.push([`calendar:${date.date}`, date.price]);
            }
            else if (row.kind === "item")
                values.push(["price", record(s, "item", row.id).data.price]);
            else if (row.kind === "credit")
                values.push(["limit", record(s, "credit", row.id).data.limit]);
            else if (row.kind === "refund")
                values.push(["requested", record(s, "refund", row.id).data.amount]);
            else if (row.kind === "shift") {
                const data = record(s, "shift", row.id).data;
                values.push(["opening", data.opening], ["counted", data.counted], ["variance", data.variance]);
                for (const [index, movement] of data.movements.entries())
                    values.push([`movement:${index}`, movement.amount]);
            }
            // Cash totals and guest account balances are projections of these immutable entries,
            // not fresh transactions to quote again on every room or shift update.
            for (const [kind, amount] of values) {
                const before = row.kind === "rate" && kind.startsWith("child_band:")
                    ? (previous?.childBands as Array<{
                        price: number;
                    }> | undefined)?.[Number(kind.split(":")[1])]?.price
                    : row.kind === "rate" && kind.startsWith("calendar:")
                        ? (previous?.calendar as Array<{
                            date: string;
                            price: number;
                        }> | undefined)?.find(value => value.date === kind.slice(9))?.price
                        : row.kind === "shift" && kind.startsWith("movement:")
                            ? (previous?.movements as Array<{
                                amount: number;
                            }> | undefined)?.[Number(kind.split(":")[1])]?.amount
                            : previous?.[kind === "extra_adult" ? "extraAdult" : kind === "requested" ? "amount" : kind];
                if (before !== amount)
                    await capture(`pms_${row.kind}`, `${row.id}:${row.version}`, kind, amount);
            }
        }
        quotes.push(...additionalQuotes);
        if (quotes.length) await tx.captureMoneySnapshots(company, quotes);
        // Source-sensitive carries/reversals below keep their original ordered reads and writes.
        collecting = false;
        for (const row of records(s, "entry")) {
            if (original.has(row.id))
                continue; // Entries are immutable economic source records.
            const input = { sourceType: "pms_entry", sourceId: row.id, amountKind: "total", transactionDate: row.data.date };
            if (row.data.total < 0) {
                if (row.data.funding?.length) {
                    if (row.data.funding.reduce((sum, value) => sum + value.amount, 0) !== -row.data.total)
                        throw ports.error(409, "PMS_REFUND_FUNDING_CHANGED", "The refund must retain all of its original payment sources.");
                    for (const source of row.data.funding) {
                        const value = await tx.snapshotMoneyReversal(company, { ...input, amountKind: `funding:${source.entryId}`,
                            amountUnits: String(source.amount), original: { sourceType: "pms_entry", sourceId: source.entryId, amountKind: "total" } });
                        await value.statement.run();
                    }
                }
                else {
                    const value = await tx.snapshotMoneyReversal(company, { ...input, amountUnits: String(-row.data.total),
                        original: { sourceType: "pms_entry", sourceId: row.data.originalId, amountKind: "total" } });
                    await value.statement.run();
                }
                continue;
            }
            if (row.data.sourceKey.startsWith("online-payment:") || row.data.sourceKey.startsWith("counter-payment:")) {
                const online = row.data.sourceKey.startsWith("online-payment:"), sourceId = row.data.sourceKey.slice(online ? 15 : 16);
                const value = await tx.snapshotMoneyCarry(company, { ...input, amountUnits: String(row.data.total),
                    allocationOffsetUnits: String(row.data.providerPaymentOffset ?? 0),
                    original: { sourceType: online ? "pms_booking_payment" : "pms_stripe_payment", sourceId, amountKind: "requested" } });
                await value.statement.run();
            }
            else if (row.data.kind === "transfer" && row.data.originalId) {
                const counter = record(s, "entry", row.data.originalId);
                const value = await tx.snapshotMoneyCarry(company, { ...input, amountUnits: String(row.data.total),
                    original: { sourceType: "pms_entry", sourceId: counter.data.originalId, amountKind: "total" } });
                await value.statement.run();
            }
            else if (row.data.sourceKey.startsWith("night:")) {
                const date = row.data.sourceKey.slice(-10), stay = record(s, "stay", row.data.stayId);
                const candidates = (await tx.readMoneyTransactionSnapshots({ companyId: company.companyId, fact1: `${stay.id}:%`, date: date, fact3: String(row.data.total) })).results.sort((a, b) => Number(b.source_id.split(":").at(-1)) - Number(a.source_id.split(":").at(-1)));
                if (!candidates[0])
                    throw ports.error(409, "PMS_PRICE_SNAPSHOT_REQUIRED", "The room charge must retain its reviewed booking price.");
                const value = await tx.snapshotMoneyCarry(company, { ...input, amountUnits: String(row.data.total),
                    original: { sourceType: "pms_stay_price", sourceId: candidates[0].source_id, amountKind: date } });
                await value.statement.run();
            }
            else if (row.data.funding?.length) {
                if (row.data.funding.reduce((sum, value) => sum + value.amount, 0) !== row.data.total)
                    throw ports.error(409, "PMS_ALLOCATION_FUNDING_CHANGED", "The allocation must retain its original payment sources.");
                for (const source of row.data.funding) {
                    const value = await tx.snapshotMoneyCarry(company, { ...input, amountKind: `funding:${source.entryId}`,
                        amountUnits: String(source.amount), original: { sourceType: "pms_entry", sourceId: source.entryId, amountKind: "total" } });
                    await value.statement.run();
                }
            }
            else
                await capture(input.sourceType, input.sourceId, input.amountKind, row.data.total, row.data.date);
        }
    }
    return { hydrateReceiptAllocationCredits, loadPms, pmsWorkspace, effects, pmsCommand, drainPms, pmsWork, applyPosRoomCharge, reversePosRoomCharge, persistPmsRecords, pmsPermissions };
}
