import { canonicalGuestAccount, legacyGuestAccount } from "@/lib/guest-account-compat";
import { z } from 'zod';
import { type PosAccounts, settingsSchema, catalogSchema, money, qty, saleAmounts, recipeQuantities, portion, businessDate as evaluateBusinessDate, accountKeys } from "../domain/pos";
import { currencyDefinition } from '@/lib/company-money';
import { scaledAmount } from '@/lib/commerce-math';
import { samePerson } from '@/lib/iam';
import { hasPermission } from '@/lib/permissions';
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { posCommerceAccountDefaults, posGeneratedAccountSpecs } from '@/lib/pos-bootstrap';
import type { Clock } from "../../shared/application/ports";
export type PosRow = Record<string, string | number | boolean | null>;
export type PosRows<Row> = {
    results: Row[];
};
export type PosEffect = {
    run(): Promise<PosRows<Record<string, unknown>>>;
};
export type PosAttachmentBinding<Effect> = {
    guards: Effect[];
    steps: Effect[];
};
export type PosUnit<Effect extends PosEffect> = {
    reversalRequest(id: string): Promise<PosRow | null>;
    pendingReversal(saleId: string): Promise<PosRow | null>;
    saveReversalRequest(row: PosRow): Promise<void>;
    reviewReversalRequest(id: string, state: string, actorId: string, reason: string, now: number): Promise<void>;
    prepareAccounting(companyId: string, accounts: PosAccounts, preparedBy: string): Promise<void>;
    approveAccounting(companyId: string, approvedBy: string): Promise<void>;
    saveOperationalSettings(companyId: string, enabled: boolean, discount: number): Promise<void>;
    assertHotelDay(companyId: string, registerId: unknown, date: string): Promise<void>;
    assertFulfillmentHotelDay(companyId: string, fulfillmentId: unknown, date: string): Promise<void>;
    linkRegister(id: string, propertyId: string | null): Promise<void>;
    readPosSales1(facts: {
        saleId: string;
    }): Promise<PosRow | null>;
    readPosSettings1(facts: {
        companyId: string;
    }): Promise<PosRow | null>;
    readFinancePeriods1(facts: {
        companyId: string;
        date: string;
        date2: string;
    }): Promise<PosRow[]>;
    readPosPostings1(facts: {
        sourceKey: string;
    }): Promise<PosRow | null>;
    readFinanceCharts1(facts: {
        chartId: string;
        companyId: string;
    }): Promise<PosRow | null>;
    readFinanceAccounts1(facts: {
        account: string;
        companyId: string;
        chartId: string;
    }): Promise<PosRow | null>;
    writeFinanceJournals1(facts: {
        journal: `${string}-${string}-${string}-${string}-${string}`;
        companyId: string;
        chartId: string;
        period: string;
        fact4: string;
        date: string;
        currency: string;
        sourceId: string;
        sourceKey: string;
        actorId: string;
        actorId2: string;
        now: number;
        now2: number;
        now3: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writeFinanceJournalLines1(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        journal: `${string}-${string}-${string}-${string}-${string}`;
        account: string;
        debit: number;
        credit: number;
        debit2: number;
        credit2: number;
        fact7: string;
        i: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosPostings1(facts: {
        sourceKey: string;
        companyId: string;
        journal: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosAudit1(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        companyId: string;
        branchId: string | null;
        actorId: string;
        operation: string;
        recordId: string;
        fact6: string;
        fact7: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosJobs1(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        companyId: string;
        kind: string;
        recordId: string;
        fact4: string;
        fact5: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosSales1(facts: {
        saleId: string;
        saleId2: string;
        saleId3: string;
        saleId4: string;
        fact4: number;
        saleId5: string;
        clearApproval: boolean;
    }): Promise<PosRows<Record<string, unknown>>>;
    readCommerceStock1(facts: {
        warehouse_id: string | number | boolean | null;
        productId: string;
    }): Promise<PosRow | null>;
    readInventoryActiveReservations1(facts: {
        warehouse_id: string | number | boolean | null;
        productId: string;
    }): Promise<PosRow | null>;
    writePosReservations1(facts: {
        id: string | number | boolean | null;
        warehouse_id: string | number | boolean | null;
        productId: string;
        quantityMilli: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosFulfillments1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readCommerceStock2(facts: {
        warehouse_id: string | number | boolean | null;
        productId: string;
    }): Promise<PosRow | null>;
    readInventoryLedger1(facts: {
        warehouse_id: string | number | boolean | null;
        productId: string;
        business_date: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosReservations2(facts: {
        id: string | number | boolean | null;
        productId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writeCommerceStock1(facts: {
        quantityMilli: number;
        value: number;
        warehouse_id: string | number | boolean | null;
        productId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosStockMoves1(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        warehouse_id: string | number | boolean | null;
        productId: string;
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
        fact6: string;
        fact7: "kitchen" | "sale";
        fact8: number;
        fact9: number;
        business_date: string | number | boolean | null;
        fact11: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosFulfillments2(facts: {
        cost: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSales2(facts: {
        sale_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writeFinancePayments1(facts: {
        financeId: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        party_id: string | number | boolean | null;
        fact3: string;
        when: string;
        currency: string | number | boolean | null;
        amount_minor: string | number | boolean | null;
        account: string;
        journal: string | null;
        actorId: string;
        actorId2: string;
        fact11: number;
        fact12: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosPayments1(facts: {
        financeId: `${string}-${string}-${string}-${string}-${string}`;
        fact1: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales2(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosLines1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPosPayments2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments3(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosFulfillments1(facts: {
        itemId: string;
    }): Promise<PosRow | null>;
    writePosFulfillments3(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writeFinanceParties1(facts: {
        party: string;
        company_id: string | number | boolean | null;
        currency: string | number | boolean | null;
        fact3: number;
        fact4: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writeFinanceInvoices1(facts: {
        invoice: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        party: string | number | true;
        number: string | number | boolean | null;
        when: string;
        when2: string;
        currency: string | number | boolean | null;
        subtotal_minor: string | number | boolean | null;
        tax_minor: string | number | boolean | null;
        total_minor: string | number | boolean | null;
        receivable: string;
        revenue: string;
        outputTax: string;
        id: string | number | boolean | null;
        journal: string | null;
        actorId: string;
        actorId2: string;
        fact17: number;
        fact18: number;
        fact19: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments4(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    writeFinancePayments2(facts: {
        party: string | number | true;
        finance_payment_id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writeFinanceAllocations1(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        finance_payment_id: string | number | boolean | null;
        invoice: `${string}-${string}-${string}-${string}-${string}`;
        fact4: number;
        actorId: string;
        fact6: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales3(facts: {
        invoice: `${string}-${string}-${string}-${string}-${string}`;
        party: string | number | true;
        fact2: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readAssignments1(facts: {
        assignmentId: string;
        userId: string;
        companyId: string;
        role: string;
        fact4: number;
        fact5: number;
        branchId: string | null;
    }): Promise<PosRow | null>;
    writeContext1(facts: {
        companyId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosReceipts1(facts: {
        key: string;
    }): Promise<PosRow | null>;
    writeCommandAssertions1(facts: {
        key: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readAttachmentSets1(facts: {
        companyId: string;
        fact1: Array<string>;
        placeholders: string;
    }): Promise<PosRows<{
        id: string;
        purpose_key: string;
        component_kind: string;
    }>>;
    writePosReceipts1(facts: {
        key: string;
        companyId: string;
        hash: string;
        fact3: string;
        fact4: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writeCommandAssertions2(facts: {
        key: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readOrganizationNodes1(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readPosSettings2(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readPosRegisters1(facts: {
        company: string;
        mode: "retail" | "restaurant";
    }): Promise<PosRow[]>;
    readCommerceSettings1(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readFinanceCharts2(facts: {
        chartId: string;
        company: string;
    }): Promise<PosRow | null>;
    readFinanceAccounts2(facts: {
        accountId: string;
        company: string;
        chartId: string;
        category: "revenue" | "asset" | "liability" | "expense";
        currency: string;
    }): Promise<PosRow | null>;
    readCommerceWarehouses1(facts: {
        warehouseId: string;
        company: string;
        branchId: string;
    }): Promise<PosRow | null>;
    readCommerceWarehouses2(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readFinanceAccounts3(facts: {
        company: string;
        chartId: string;
        currency: string;
    }): Promise<PosRow[]>;
    readFinanceAccounts4(facts: {
        chartId: string;
        code: string;
    }): Promise<PosRow | null>;
    writeFinanceAccounts1(facts: {
        accountId: `${string}-${string}-${string}-${string}-${string}`;
        chartId: string;
        company: string;
        code: string;
        name: "POS bank deposits" | "POS EDC clearing" | "POS Stripe clearing" | "POS customer advances" | "POS work in progress" | "POS waste expense" | "POS customer refunds payable" | "POS fees and losses" | "POS cash variance" | "POS cash transfers" | "POS disputes pending";
        category: "asset" | "liability" | "expense";
        fact6: string | null;
        fact7: 0 | 1;
        controlKind: "bank" | "none";
        now: number;
        now2: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosRegisters2(facts: {
        company: string;
        branchId: string;
        registerName: string;
    }): Promise<PosRow | null>;
    writePosSettings1(facts: {
        company: string;
        currency: string;
        fact2: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRegisters1(facts: {
        registerId: string;
        company: string;
        branchId: string;
        warehouseId: string;
        registerName: string;
        mode: "retail" | "restaurant";
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosOauthStates1(facts: {
        state: `${string}-${string}-${string}-${string}-${string}`;
        company: string;
        user: string;
        fact3: number;
        livemode: boolean;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSettings3(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readFinanceAccounts5(facts: {
        fact0: string;
        company: string;
        chartId: string;
    }): Promise<PosRow | null>;
    readFinanceCharts3(facts: {
        chartId: string;
        company: string;
    }): Promise<PosRow | null>;
    readCommerceSettings2(facts: {
        company: string;
    }): Promise<PosRow | null>;
    writePosSettings2(facts: {
        company: string;
        enabled: boolean;
        currency: string;
        timezone: "Asia/Bangkok";
        fact4: string;
        cashierDiscountBps: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readCommerceWarehouses3(facts: {
        warehouseId: string;
    }): Promise<PosRow | null>;
    readPosRegisters3(facts: {
        id: string;
    }): Promise<PosRow | null>;
    readPosShifts1(facts: {
        id: string;
    }): Promise<PosRow | null>;
    writePosRegisters2(facts: {
        warehouseId: string;
        branch_id: string | number | boolean | null;
        name: string;
        mode: "retail" | "restaurant";
        active: boolean;
        rid: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRegisters3(facts: {
        rid: string;
        company: string;
        branch_id: string | number | boolean | null;
        warehouseId: string;
        name: string;
        mode: "retail" | "restaurant";
        active: boolean;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosRegisters4(facts: {
        registerId: string;
    }): Promise<PosRow | null>;
    readPosTables1(facts: {
        registerId: string;
        name: string;
    }): Promise<PosRow | null>;
    writePosTables1(facts: {
        tid: `${string}-${string}-${string}-${string}-${string}`;
        registerId: string;
        name: string;
        seats: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosCatalog1(facts: {
        productId: string;
        company: string;
    }): Promise<PosRow | null>;
    readPosRegisterCatalog1(facts: {
        productId: string;
        branchId: string;
        company: string;
    }): Promise<PosRow | null>;
    writePosCatalog1(facts: {
        imageId: string;
        productId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readCommerceProducts1(facts: {
        productId: string;
        company: string;
    }): Promise<PosRow | null>;
    readCommerceProducts2(facts: {
        productId: string;
        company: string;
    }): Promise<PosRow | null>;
    readPosCatalog2(facts: {
        productId: string;
    }): Promise<PosRow | null>;
    readPosRegisters5(facts: {
        company: string;
        branchId: string | null;
        branchId2: string | null;
        kind: "stock" | "recipe" | "service";
    }): Promise<PosRow[]>;
    readPosRegisters6(facts: {
        rid: string;
    }): Promise<PosRow | null>;
    writePosCatalog2(facts: {
        productId: string;
        company: string;
        fact2: string | null;
        category: string;
        kind: "stock" | "recipe" | "service";
        station: string;
        fact6: string;
        fact7: string;
        taxInclusive: boolean;
        enabled: boolean;
        fact10: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRegisterCatalog1(facts: {
        productId: string;
        company: string;
        branchId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRegisterCatalog2(facts: {
        productId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRegisterCatalog3(facts: {
        rid: string;
        productId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosRegisters7(facts: {
        registerId: string;
    }): Promise<PosRow | null>;
    readPmsProperties1(facts: {
        company: string;
        branch_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosShifts1(facts: {
        sid: `${string}-${string}-${string}-${string}-${string}`;
        id: string | number | boolean | null;
        company: string;
        branch_id: string | number | boolean | null;
        user: string;
        fact5: number;
        openingMinor: number;
        fact7: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosShifts2(facts: {
        id: string;
    }): Promise<PosRow | null>;
    writePosCashMovements1(facts: {
        mid: `${string}-${string}-${string}-${string}-${string}`;
        id: string | number | boolean | null;
        amountMinor: number;
        reason: string;
        user: string;
        null: null;
        fact6: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosShifts2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosCashMovements1(facts: {
        movementId: string | undefined;
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosCashMovements2(facts: {
        user: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSales3(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosCashMovements2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments5(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosRefundPayments1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosCashMovements3(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosShifts3(facts: {
        countedMinor: number;
        expected: number;
        fact2: number;
        fact3: number;
        user: string;
        fact5: string | null;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosShifts4(facts: {
        user: string;
        reason: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosShifts5(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosShifts3(facts: {
        shiftId: string;
    }): Promise<PosRow | null>;
    readCompanyFxRates1(facts: {
        company: string;
        currency: string;
    }): Promise<PosRow | null>;
    readPosRegisters8(facts: {
        register_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosTables2(facts: {
        tableId: string;
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readFinanceParties1(facts: {
        partyId: string;
        company: string;
    }): Promise<PosRow | null>;
    writePosSales4(facts: {
        sid: `${string}-${string}-${string}-${string}-${string}`;
        company: string;
        branch_id: string | number | boolean | null;
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
        fact5: string;
        currency: string | number | boolean | null;
        fact7: string;
        fact8: string | null;
        fact9: string | null;
        guests: number;
        serviceMode: "retail" | "dine_in" | "takeaway";
        user: string;
        fact13: number;
        fact14: number;
        accounts_json: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readOrganizationNodes2(facts: {
        company: string;
    }): Promise<PosRow | null>;
    writePosSales5(facts: {
        fact0: string;
        sid: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosDayCloses1(facts: {
        company_id: string | number | boolean | null;
        branch_id: string | number | boolean | null;
        business_date: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosAudit1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosRoomChargeReversals1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments6(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPmsSources1(facts: {
        fact0: string;
        company: string;
        branch_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosLines2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPosLines3(facts: {
        fulfillment_id: string | number | boolean | null;
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readCommerceStock3(facts: {
        warehouse_id: string | number | boolean | null;
        product_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writeCommerceStock2(facts: {
        quantity_milli: string | number | boolean | null;
        cost: number;
        warehouse_id: string | number | boolean | null;
        product_id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosStockMoves2(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company: string;
        warehouse_id: string | number | boolean | null;
        product_id: string | number | boolean | null;
        fulfillment_id: string | number | boolean | null;
        id: string | number | boolean | null;
        number: string | number | boolean | null;
        fact7: "return";
        quantity_milli: string | number | boolean | null;
        cost: number;
        date: string;
        fact11: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRoomChargeReversals1(facts: {
        id: string | number | boolean | null;
        company: string;
        propertyId: string;
        entryId: string;
        reversalEntryId: string;
        reason: string;
        fact6: string;
        restored: number;
        wasted: number;
        user: string;
        fact10: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales6(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosRegisters9(facts: {
        register_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosShifts4(facts: {
        shift_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments7(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosLines4(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPosLines5(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPmsProperties2(facts: {
        propertyId: string;
        company: string;
    }): Promise<PosRow | null>;
    readPosFulfillments2(facts: {
        itemId: string;
    }): Promise<PosRow | null>;
    writePosFulfillments4(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales7(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales8(facts: {
        user: string;
        reason: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales9(facts: {
        user: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readCommerceProducts3(facts: {
        register_id: string | number | boolean | null;
        productId: string;
        company: string;
    }): Promise<PosRow | null>;
    readPosRegisters10(facts: {
        register_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosFulfillments5(facts: {
        fid: `${string}-${string}-${string}-${string}-${string}`;
        company: string;
        branch_id: string | number | boolean | null;
        warehouse_id: string | number | boolean | null;
        productId: string;
        quantityMilli: number;
        kind: string;
        fact7: string;
        note: string;
        fact9: string;
        accounts_json: string | number | boolean | null;
        currency: string | number | boolean | null;
        business_date: string | number | boolean | null;
        fact13: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosLines1(facts: {
        lid: `${string}-${string}-${string}-${string}-${string}`;
        id: string | number | boolean | null;
        fid: `${string}-${string}-${string}-${string}-${string}`;
        productId: string;
        name: string | number | boolean | null;
        quantityMilli: number;
        price: number;
        tax_bps: string | number | boolean | null;
        fact8: boolean;
        discountMinor: number;
        subtotal: number;
        tax: number;
        fact12: string;
        note: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readFinanceParties2(facts: {
        partyId: string;
        company: string;
    }): Promise<PosRow | null>;
    writePosSales10(facts: {
        partyId: string | null;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosLines6(facts: {
        lineId: string;
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosFulfillments6(facts: {
        quantityMilli: number;
        note: string;
        fulfillment_id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosLines2(facts: {
        quantityMilli: number;
        discount: number;
        subtotal: number;
        tax: number;
        note: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales11(facts: {
        user: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosLines7(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    writePosLines3(facts: {
        discount: number;
        extra: number;
        subtotal: number;
        tax: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales12(facts: {
        user: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosLines8(facts: {
        lineId: string;
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosReservations3(facts: {
        fulfillment_id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosLines4(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales13(facts: {
        fact0: "held" | "open";
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosTables3(facts: {
        tableId: string;
        register_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosSales14(facts: {
        tableId: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosLines5(facts: {
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales15(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosLines9(facts: {
        lineId: string;
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosFulfillments3(facts: {
        fid: string;
    }): Promise<PosRow | null>;
    writePosSales16(facts: {
        target: `${string}-${string}-${string}-${string}-${string}`;
        fact1: string;
        user: string;
        fact3: number;
        fact4: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosLines6(facts: {
        target: `${string}-${string}-${string}-${string}-${string}`;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosFulfillments7(facts: {
        newFid: string;
        quantityMilli: number;
        fact2: number;
        fid: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosFulfillments8(facts: {
        quantityMilli: number;
        fid: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosLines7(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        target: `${string}-${string}-${string}-${string}-${string}`;
        newFid: string;
        quantityMilli: number;
        discount: number;
        billDiscount: number;
        amount: number;
        tax: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosLines8(facts: {
        quantityMilli: number;
        amount: number;
        tax: number;
        discount: number;
        billDiscount: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales17(facts: {
        issuer_json: string | number | boolean | null;
        user: string;
        target: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosFulfillments4(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPosLines10(facts: {
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosReservations4(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosFulfillments9(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales18(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSales4(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosFulfillments5(facts: {
        id: string;
    }): Promise<PosRow | null>;
    readPosLines11(facts: {
        id: string | number | boolean | null;
        user: string;
    }): Promise<PosRow | null>;
    readPosAudit2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosLines12(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosReservations5(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosFulfillments10(facts: {
        state: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosFulfillments11(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosShifts5(facts: {
        shift_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosLines13(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments8(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosFulfillments6(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPosConnections1(facts: {
        company: string;
    }): Promise<PosRow | null>;
    writePosPayments2(facts: {
        pid: `${string}-${string}-${string}-${string}-${string}`;
        id: string | number | boolean | null;
        company: string;
        method: "cash" | "edc" | "transfer" | "stripe_card" | "stripe_promptpay";
        amountMinor: number;
        tenderedMinor: number;
        reference: string;
        fact7: string | number | boolean | null;
        fact8: string | number | boolean;
        user: string;
        fact10: number;
        fact11: number;
        fact12: string | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosPayments3(facts: {
        fact0: number;
        pid: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments9(facts: {
        pid: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PosRow | null>;
    writePosSales19(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments10(facts: {
        id: string;
        company: string;
    }): Promise<PosRow | null>;
    readFinanceBankTransactions1(facts: {
        bankTransactionId: string;
        company: string;
    }): Promise<PosRow | null>;
    readPosPayments11(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writeFinanceBankTransactions1(facts: {
        fact0: string | number | boolean | null;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments12(facts: {
        id: string;
        company: string;
    }): Promise<PosRow | null>;
    writePosPayments4(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales20(facts: {
        fact0: number;
        sale_id: string | number | boolean | null;
        sale_id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments13(facts: {
        id: string;
    }): Promise<PosRow | null>;
    readPosJobs1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosPayments5(facts: {
        payerEmail: string;
        fact1: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosJobs2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosConnections2(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readPosSettlements1(facts: {
        id: string;
        company: string;
    }): Promise<PosRow | null>;
    readFinanceJournals1(facts: {
        journalId: string;
        company: string;
        currency: string | number | boolean | null;
        stripe: string;
    }): Promise<PosRow | null>;
    readPosSettlements2(facts: {
        journalId: string;
    }): Promise<PosRow | null>;
    readFinanceJournalLines1(facts: {
        journalId: string;
        bank: string;
    }): Promise<PosRow | null>;
    writePosSettlements1(facts: {
        fact0: "paid" | "mapped";
        journalId: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSettlements3(facts: {
        id: string;
        company: string;
    }): Promise<PosRow | null>;
    readFinanceBankMatchTargets1(facts: {
        journal_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readFinanceBankTransactions2(facts: {
        bankTransactionId: string;
        company: string;
    }): Promise<PosRow | null>;
    readFinanceJournalLines2(facts: {
        journal_id: string | number | boolean | null;
        bank_account: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writeFinanceBankTransactions2(facts: {
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSettlements2(facts: {
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosJobs2(facts: {
        id: string;
        company: string;
    }): Promise<PosRow | null>;
    readPosRefundPayments2(facts: {
        providerId: string;
        record_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments14(facts: {
        providerId: string;
        record_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosJobs3(facts: {
        fact0: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosJobs3(facts: {
        id: string;
        company: string;
    }): Promise<PosRow | null>;
    readPosRefundPayments3(facts: {
        record_id: string | number | boolean | null;
        company: string;
        company2: string;
    }): Promise<PosRow | null>;
    writePosRefunds1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosRefundPayments4(facts: {
        record_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosJobs4(facts: {
        fact0: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosDayCloses2(facts: {
        company_id: string | number | boolean | null;
        branch_id: string | number | boolean | null;
        business_date: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPmsSources2(facts: {
        fact0: string;
    }): Promise<PosRow | null>;
    readPosLines14(facts: {
        lineId: string;
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosRefunds2(facts: {
        rid: `${string}-${string}-${string}-${string}-${string}`;
        id: string | number | boolean | null;
        company_id: string | number | boolean | null;
        branch_id: string | number | boolean | null;
        fact4: string | null;
        fact5: "advance" | "sale";
        reason: string;
        fact7: string;
        amount: number;
        user: string;
        fact10: number;
        fact11: string;
        tax: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosRefunds1(facts: {
        id: string;
    }): Promise<PosRow | null>;
    writePosRefunds3(facts: {
        user: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments15(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPosLines15(facts: {
        lineId: string;
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosFulfillments7(facts: {
        fulfillment_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosStockMoves1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writeCommerceStock3(facts: {
        warehouse_id: string | number | boolean | null;
        product_id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writeCommerceStock4(facts: {
        quantityMilli: number;
        restoredCost: number;
        warehouse_id: string | number | boolean | null;
        product_id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosStockMoves3(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        warehouse_id: string | number | boolean | null;
        product_id: string | number | boolean | null;
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
        number: string | number | boolean | null;
        fact7: "return";
        quantityMilli: number;
        restoredCost: number;
        date: string;
        fact11: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosLines9(facts: {
        quantityMilli: number;
        fact1: number;
        taxMinor: number;
        restoredCost: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRefundPayments1(facts: {
        rpid: `${string}-${string}-${string}-${string}-${string}`;
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
        amount: number;
        fact4: "cash" | "transfer" | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosPayments6(facts: {
        amount: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosShifts6(facts: {
        payout_shift_id: string | number | boolean | null;
        companyId: string;
        branch_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosRefunds4(facts: {
        user: string;
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosRefundPayments5(facts: {
        refundPaymentId: string;
    }): Promise<PosRow | null>;
    readPosRefunds2(facts: {
        refund_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments16(facts: {
        payment_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosSales5(facts: {
        sale_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writeFinancePayments3(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        party_id: string | number | boolean | null;
        fact3: string;
        fact4: string | number | boolean | null;
        currency: string | number | boolean | null;
        amount_minor: string | number | boolean | null;
        account: string;
        journal: string | null;
        actorId: string;
        actorId2: string;
        fact11: number;
        fact12: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRefundPayments2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRefunds5(facts: {
        id: string | number | boolean | null;
        id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments17(facts: {
        saleId: string;
    }): Promise<PosRow | null>;
    readPosRefundPayments6(facts: {
        saleId: string;
    }): Promise<PosRow | null>;
    writePosReservations6(facts: {
        saleId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosFulfillments12(facts: {
        saleId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSales21(facts: {
        saleId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    snapshotMoneyCarry(arg0: {
        companyId: string;
    }, arg1: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
        amountUnits: string;
        original: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
        };
    }): Promise<{
        amountUnits: string;
        usdUnits: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        statement: Effect;
    }>;
    capturePosReceipt(arg0: string): Promise<void>;
    snapshotMoney(arg0: {
        companyId: string;
    }, arg1: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
        currency: string;
        currencyScale: number;
        amountUnits: string;
    }): Promise<{
        amountUnits: string;
        usdUnits: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        statement: Effect;
    }>;
    snapshotMoneyReversal(arg0: {
        companyId: string;
    }, arg1: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
        amountUnits: string;
        original: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
        };
    }): Promise<{
        currency: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        amountUnits: string;
        statement: Effect;
    }>;
    assertProductionModuleEnabled(arg0: string, arg1: "pos"): Promise<void>;
    workContextGuards(arg0: WorkContext, arg1: string): Effect[];
    iamMutationGuards(arg0: WorkContext, arg1: string): Effect[];
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
    }): Promise<PosAttachmentBinding<Effect>>;
    posDayCommand(arg0: WorkContext, arg1: string, arg2: unknown): Promise<{
        id: string | number | boolean | null;
        state: string;
    }>;
    createPosCustomer(arg0: WorkContext, arg1: unknown): Promise<{
        id: `${string}-${string}-${string}-${string}-${string}`;
        name: string;
    }>;
    requirePosImageSource(arg0: WorkContext, arg1: string, arg2: string, arg3: string): Promise<{
        id: string;
        product_id: string;
        object_key: string;
        thumbnail_key: string;
        mime_type: string;
        sha256: string;
        thumbnail_checksum_sha256: string;
        attachment_source_id: string;
        attachment_link_id: string;
        attachment_set_id: string;
        set_state: string;
        link_state: string;
        expected_target_version: number;
        component_ref: string | null;
    }>;
    assertPosDayWritable(arg0: string, arg1: string, arg2: string): Promise<void>;
    reversePosRoomCharge(arg0: WorkContext, arg1: {
        propertyId: string;
        sourceKey: string;
        expectedPropertyVersion: number;
        reason: string;
        total: number;
    }): Promise<{
        entryId: string;
        reversalEntryId: string;
        businessDate: string;
    }>;
    applyPosRoomCharge(arg0: WorkContext, arg1: {
        source: "pos";
        sourceId: string;
        sourceLineId: string;
        companyId: string;
        branchId: string;
        propertyId: string;
        stayId: string;
        currency: string;
        revenueOwner: "pms";
        sourceJournalId: null;
        description: string;
        net: number;
        service: number;
        tax: number;
        total: number;
    }, arg2: number): Promise<{
        entryId: string;
        propertyVersion: number;
    }>;
    batch(effects: Effect[]): Promise<unknown>;
};
export type PosRepository<Effect extends PosEffect> = PosUnit<Effect> & {
    transaction<Result>(work: (unit: PosUnit<Effect>) => Promise<Result>): Promise<Result>;
};
export function createPosApplication<Effect extends PosEffect>(ports: {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    isBusinessError(error: unknown): error is Error & {
        status: number;
        code: string;
    };
    environment(key: "NODE_ENV" | "POS_STRIPE_LIVE_ENABLED" | "ERPX_POS_ROOM_CHARGE_PILOT_ENABLED"): string | undefined;
}) {
    const businessDate = (timezone = "Asia/Bangkok", date = new Date(ports.clock.now())) => evaluateBusinessDate(timezone, date);
    const id = () => ports.uuid();
    const num = (row: PosRow, key: string) => Number(row[key] ?? 0);
    const str = (row: PosRow, key: string) => String(row[key] ?? '');
    function json<T>(value: unknown): T { return JSON.parse(String(value)) as T; }
    function fail(code: string, message: string, status = 409): never { throw ports.error(status, code, message); }
    function scopeRow(actor: WorkContext, row: PosRow | null): PosRow {
        if (!row || row.company_id !== actor.companyId || (actor.branchId && row.branch_id !== actor.branchId) || actor.departmentId)
            fail('POS_NOT_FOUND', 'Record not found in this workspace', 404);
        return row;
    }
    async function saleRow(db: PosUnit<Effect>, actor: WorkContext, saleId: string) { return scopeRow(actor, await db.readPosSales1({ saleId: saleId })); }
    function version(row: PosRow, input: {
        expectedVersion?: number;
    }) {
        if (num(row, 'version') !== input.expectedVersion)
            fail('POS_VERSION_CONFLICT', 'Record changed. Load the latest data');
    }
    async function config(db: PosUnit<Effect>, companyId: string) {
        const row = await db.readPosSettings1({ companyId: companyId });
        if (!row)
            fail('POS_SETUP_REQUIRED', 'Configure accounts and sales points before use');
        return row;
    }
    async function openPeriod(db: PosUnit<Effect>, companyId: string, date: string) {
        const periods = await db.readFinancePeriods1({ companyId: companyId, date: date, date2: date });
        if (periods.length !== 1)
            fail('POS_PERIOD_CLOSED', 'An open accounting period is required for this date');
        return str(periods[0], 'id');
    }
    async function posting(db: PosUnit<Effect>, companyId: string, branchId: string | null, accounts: PosAccounts, date: string, currency: string, sourceKey: string, sourceId: string, actorId: string, entries: Array<{
        account: string;
        debit: number;
        credit: number;
    }>) {
        const existing = await db.readPosPostings1({ sourceKey: sourceKey });
        if (existing)
            return str(existing, 'journal_id');
        const lines = entries.filter(e => e.debit || e.credit);
        if (!lines.length)
            return null;
        if (lines.some(e => !Number.isSafeInteger(e.debit) || !Number.isSafeInteger(e.credit) || e.debit < 0 || e.credit < 0 || Boolean(e.debit) === Boolean(e.credit)) || lines.reduce((n, e) => n + e.debit - e.credit, 0) !== 0)
            fail('POS_UNBALANCED', 'Journal is unbalanced', 422);
        const period = await openPeriod(db, companyId, date), journal = id(), now = ports.clock.now();
        const chart = await db.readFinanceCharts1({ chartId: accounts.chartId, companyId: companyId });
        if (!chart)
            fail('POS_CHART_INVALID', 'Company chart of accounts not found');
        for (const line of lines)
            if (!await db.readFinanceAccounts1({ account: line.account, companyId: companyId, chartId: accounts.chartId }))
                fail('POS_ACCOUNT_INVALID', 'Configured account is not available');
        await db.writeFinanceJournals1({ journal: journal, companyId: companyId, chartId: accounts.chartId, period: period, fact4: `POS-GL-${journal}`, date: date, currency: currency, sourceId: sourceId, sourceKey: sourceKey, actorId: actorId, actorId2: actorId, now: now, now2: now, now3: now });
        for (const [i, line] of lines.entries())
            await db.writeFinanceJournalLines1({ fact0: id(), journal: journal, account: line.account, debit: line.debit, credit: line.credit, debit2: line.debit, credit2: line.credit, fact7: JSON.stringify({ branchId, sourceId }), i: i });
        await db.writePosPostings1({ sourceKey: sourceKey, companyId: companyId, journal: journal });
        return journal;
    }
    const pair = (debit: string, credit: string, amount: number) => [{ account: debit, debit: amount, credit: 0 }, { account: credit, debit: 0, credit: amount }];
    async function audit(db: PosUnit<Effect>, companyId: string, branchId: string | null, actorId: string, operation: string, recordId: string, details: unknown) { await db.writePosAudit1({ fact0: id(), companyId: companyId, branchId: branchId, actorId: actorId, operation: operation, recordId: recordId, fact6: JSON.stringify(details), fact7: ports.clock.now() }); }
    async function enqueue(db: PosUnit<Effect>, companyId: string, kind: string, recordId: string, payload: unknown = {}) { await db.writePosJobs1({ fact0: id(), companyId: companyId, kind: kind, recordId: recordId, fact4: JSON.stringify(payload), fact5: ports.clock.now() }); }
    async function editable(db: PosUnit<Effect>, sale: PosRow) {
        if (!['open', 'held'].includes(str(sale, 'state')))
            fail('POS_SALE_LOCKED', 'This bill cannot be edited');
        if (await db.readPosPayments1({ id: sale.id }))
            fail('POS_PAYMENT_STARTED', 'This bill already has a payment');
    }
    async function refreshSale(db: PosUnit<Effect>, saleId: string, clearApproval = true) { await db.writePosSales1({ saleId: saleId, saleId2: saleId, saleId3: saleId, saleId4: saleId, fact4: ports.clock.now(), saleId5: saleId, clearApproval: clearApproval }); }
    async function ingredients(db: PosUnit<Effect>, f: PosRow) {
        return recipeQuantities(json<Array<{
            productId: string;
            quantityMilli: number;
        }>>(f.recipe_json), num(f, 'quantity_milli'));
    }
    async function reserve(db: PosUnit<Effect>, f: PosRow) {
        if (f.reserved || ['preparing', 'ready', 'delivered'].includes(str(f, 'state')))
            return;
        for (const part of await ingredients(db, f)) {
            const stock = await db.readCommerceStock1({ warehouse_id: f.warehouse_id, productId: part.productId });
            const held = await db.readInventoryActiveReservations1({ warehouse_id: f.warehouse_id, productId: part.productId });
            if (!stock || num(stock, 'quantity_milli') - num(held!, 'n') < part.quantityMilli)
                fail('POS_STOCK_UNAVAILABLE', 'Insufficient product/ingredients for this item');
            await db.writePosReservations1({ id: f.id, warehouse_id: f.warehouse_id, productId: part.productId, quantityMilli: part.quantityMilli });
        }
        await db.writePosFulfillments1({ id: f.id });
    }
    async function consume(db: PosUnit<Effect>, f: PosRow, actorId: string, kitchen: boolean) {
        await reserve(db, f);
        let cost = 0;
        for (const part of await ingredients(db, f)) {
            const stock = await db.readCommerceStock1({ warehouse_id: f.warehouse_id, productId: part.productId });
            if (!stock || num(stock, 'quantity_milli') < part.quantityMilli)
                fail('POS_STOCK_UNAVAILABLE', 'Stock changed. Review it');
            if (await db.readInventoryLedger1({ warehouse_id: f.warehouse_id, productId: part.productId, business_date: f.business_date }))
                fail('POS_BACKDATED_STOCK', 'Stock movement exists after this transaction date');
            const value = part.quantityMilli === num(stock, 'quantity_milli') ? num(stock, 'value_minor') : scaledAmount(num(stock, 'value_minor'), part.quantityMilli, num(stock, 'quantity_milli'));
            cost += value;
            await db.writePosReservations2({ id: f.id, productId: part.productId });
            await db.writeCommerceStock1({ quantityMilli: part.quantityMilli, value: value, warehouse_id: f.warehouse_id, productId: part.productId });
            await db.writePosStockMoves1({ fact0: id(), company_id: f.company_id, warehouse_id: f.warehouse_id, productId: part.productId, id: f.id, id2: f.id, fact6: `POS-${f.id}`, fact7: kitchen ? 'kitchen' : 'sale', fact8: -part.quantityMilli, fact9: -value, business_date: f.business_date, fact11: ports.clock.now() });
        }
        const a = json<PosAccounts>(f.accounts_json);
        await posting(db, str(f, 'company_id'), str(f, 'branch_id'), a, str(f, 'business_date'), str(f, 'currency'), `consume:${f.id}`, str(f, 'id'), actorId, pair(kitchen ? a.wip : a.cogs, a.inventory, cost));
        await db.writePosFulfillments2({ cost: cost, id: f.id });
        return cost;
    }
    async function receivePayment(db: PosUnit<Effect>, payment: PosRow, actorId: string, date?: string) {
        if (payment.state === 'succeeded')
            return;
        const sale = (await db.readPosSales2({ sale_id: payment.sale_id }))!;
        if (['cancelled', 'merged'].includes(str(sale, 'state')))
            fail('POS_PAYMENT_EXCEPTION', 'Payment for a cancelled bill requires review');
        const a = json<PosAccounts>(sale.accounts_json), when = date ?? str(sale, 'business_date');
        const account = payment.method === 'cash' ? a.cash : payment.method === 'transfer' ? a.bank : payment.method === 'edc' ? a.edc : a.stripe;
        const financeId = id();
        const journal = await posting(db, str(sale, 'company_id'), str(sale, 'branch_id'), a, when, str(sale, 'currency'), `receipt:${payment.id}`, str(sale, 'id'), actorId, pair(account, sale.invoice_id ? a.receivable : a.advance, num(payment, 'amount_minor')));
        await db.writeFinancePayments1({ financeId: financeId, company_id: sale.company_id, party_id: sale.party_id, fact3: `POS-R-${payment.id}`, when: when, currency: sale.currency, amount_minor: payment.amount_minor, account: account, journal: journal, actorId: actorId, actorId2: actorId, fact11: ports.clock.now(), fact12: ports.clock.now() });
        const received = await db.snapshotMoneyCarry({ companyId: str(sale, 'company_id') }, { sourceType: 'pos_payment', sourceId: str(payment, 'id'), amountKind: 'received', transactionDate: when, amountUnits: String(payment.amount_minor), original: { sourceType: 'pos_payment', sourceId: str(payment, 'id'), amountKind: 'requested' } });
        await received.statement.run();
        await db.writePosPayments1({ financeId: financeId, fact1: ports.clock.now(), id: payment.id });
        await db.writePosSales2({ fact0: ports.clock.now(), id: sale.id });
    }
    async function completeSale(db: PosUnit<Effect>, sale: PosRow, actorId: string, date?: string) {
        await db.assertHotelDay(str(sale,'company_id'), sale.register_id, date ?? str(sale,'business_date'));
        if (sale.state === 'completed')
            return { id: sale.id, state: 'completed', invoiceId: sale.invoice_id };
        if (!['open', 'held', 'payment_pending'].includes(str(sale, 'state')))
            fail('POS_SALE_LOCKED', 'This bill cannot be closed');
        const lines = await db.readPosLines1({ id: sale.id });
        if (!lines.length)
            fail('POS_EMPTY_SALE', 'Add items before closing the bill', 422);
        const paid = await db.readPosPayments2({ id: sale.id });
        if (num(paid!, 'amount') !== num(sale, 'total_minor'))
            fail('POS_NOT_PAID', 'Payment is incomplete');
        if (await db.readPosPayments3({ id: sale.id }))
            fail('POS_PAYMENT_PENDING', 'Wait for the payment result before closing the bill');
        const a = json<PosAccounts>(sale.accounts_json), when = date ?? str(sale, 'business_date');
        await openPeriod(db, str(sale, 'company_id'), when);
        for (const itemId of new Set(lines.map(l => str(l, 'fulfillment_id')))) {
            const f = (await db.readPosFulfillments1({ itemId: itemId }))!;
            if (f.state === 'cancelled')
                fail('POS_ITEM_CANCELLED', 'Remove cancelled items before closing the bill');
            if (f.kind === 'recipe' && f.state === 'draft')
                fail('POS_SEND_KITCHEN', 'Send items to the kitchen before closing the bill');
            if (f.kind !== 'recipe' && f.state !== 'delivered') {
                await consume(db, { ...f, business_date: when }, actorId, false);
                await db.writePosFulfillments3({ id: f.id });
            }
        }
        let party = sale.party_id;
        if (!party) {
            party = `pos-walkin-${sale.company_id}`;
            await db.writeFinanceParties1({ party: party, company_id: sale.company_id, currency: sale.currency, fact3: ports.clock.now(), fact4: ports.clock.now() });
        }
        const invoice = id();
        const journal = await posting(db, str(sale, 'company_id'), str(sale, 'branch_id'), a, when, str(sale, 'currency'), `sale:${sale.id}`, str(sale, 'id'), actorId, [{ account: a.receivable, debit: num(sale, 'total_minor'), credit: 0 }, { account: a.revenue, debit: 0, credit: num(sale, 'subtotal_minor') }, { account: a.outputTax, debit: 0, credit: num(sale, 'tax_minor') }]);
        await db.writeFinanceInvoices1({ invoice: invoice, company_id: sale.company_id, party: party, number: sale.number, when: when, when2: when, currency: sale.currency, subtotal_minor: sale.subtotal_minor, tax_minor: sale.tax_minor, total_minor: sale.total_minor, receivable: a.receivable, revenue: a.revenue, outputTax: a.outputTax, id: sale.id, journal: journal, actorId: actorId, actorId2: actorId, fact17: ports.clock.now(), fact18: ports.clock.now(), fact19: ports.clock.now() });
        await posting(db, str(sale, 'company_id'), str(sale, 'branch_id'), a, when, str(sale, 'currency'), `allocate:${sale.id}`, str(sale, 'id'), actorId, pair(a.advance, a.receivable, num(sale, 'total_minor')));
        for (const payment of await db.readPosPayments4({ id: sale.id })) {
            await db.writeFinancePayments2({ party: party, finance_payment_id: payment.finance_payment_id });
            await db.writeFinanceAllocations1({ fact0: id(), company_id: sale.company_id, finance_payment_id: payment.finance_payment_id, invoice: invoice, fact4: num(payment, 'amount_minor') - num(payment, 'refunded_minor'), actorId: actorId, fact6: ports.clock.now() });
        }
        await capturePosMoney(db, sale, 'pos_sale', str(sale, 'id'), 'total', str(sale, 'business_date'), String(sale.total_minor));
        await db.writePosSales3({ invoice: invoice, party: party, fact2: ports.clock.now(), id: sale.id });
        await db.capturePosReceipt(str(sale, 'id'));
        return { id: sale.id, state: 'completed', invoiceId: invoice };
    }
    async function capturePosMoney(db: PosUnit<Effect>, sale: PosRow, sourceType: string, sourceId: string, amountKind: string, transactionDate: string, amountUnits: string) {
        const evidence = await db.snapshotMoney({ companyId: str(sale, 'company_id') }, { sourceType, sourceId, amountKind, transactionDate, currency: str(sale, 'currency'), currencyScale: 2, amountUnits });
        await evidence.statement.run();
    }
    async function capturePosReversal(db: PosUnit<Effect>, sale: PosRow, sourceType: string, sourceId: string, amountKind: string, transactionDate: string, amountUnits: string, original: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
    }) {
        const evidence = await db.snapshotMoneyReversal({ companyId: str(sale, 'company_id') }, { sourceType, sourceId, amountKind, transactionDate, amountUnits, original });
        await evidence.statement.run();
    }
    const keySchema = z.string().regex(/^[a-zA-Z0-9:_.-]{8,180}$/);
    const base = z.object({ id: z.string().min(1), expectedVersion: z.number().int().positive() });
    const permissionFor = (op: string) =>
        op === 'room_charge_reversal.approve_pms' || op === 'room_charge_reversal.reject_pms' ? 'pms.approve'
        : op === 'room_charge_reversal.approve_pos' || op === 'room_charge_reversal.reject_pos' ? 'pos.approve'
        : op === 'room_charge_reversal.request' ? 'pos.sell'
        : op === 'accounting.prepare' || op === 'settings.bootstrap' || op === 'settings.save' ? 'pos.accounting.prepare'
        : op === 'accounting.approve' ? 'pos.accounting.approve'
        : op === 'sale.reverse_room_charge' ? 'finance.approve'
        : op === 'day.confirm' || op === 'payment.verify_transfer' || op.startsWith('settlement.') || op === 'exception.attach_provider' ? 'finance.approve'
        : op === 'day.prepare' ? 'pos.approve'
        : op === 'settings.operations' || op.startsWith('register') || op.startsWith('catalog') || op.startsWith('table.') ? 'pos.configure'
        : op === 'kitchen.send' || op === 'kitchen.deliver' ? 'pos.sell' : op.startsWith('kitchen.') ? 'pos.kitchen'
        : op === 'refund.approve' || op === 'refund.reject' || op === 'sale.approve_discount' || op === 'shift.review' || op === 'cash.approve' || op.startsWith('exception.') || op.startsWith('settlement.') ? 'pos.approve'
        : op.startsWith('connection.') ? 'pos.payments.configure' : 'pos.sell';
    async function posCommand(db: PosRepository<Effect>, actor: WorkContext, operation: string, input: unknown, key: string) {
        input = canonicalGuestAccount(input);
        keySchema.parse(key);

        if (operation.startsWith('connection.')) {
            if (!hasPermission(actor, 'pos.payments.configure') && !hasPermission(actor, 'finance.approve'))
                fail('POS_CONNECTION_FORBIDDEN', 'Company Finance access is required', 403);
        }
        else if (operation === 'exception.retry') {
            if(!hasPermission(actor,'pos.approve') && !hasPermission(actor,'finance.approve')) ports.require(actor,'pos.approve');
        } else ports.require(actor, permissionFor(operation));
        if (operation === 'settings.bootstrap') {
            ports.require(actor,'pos.configure');
            ports.require(actor, 'finance.write');
        }
        if (actor.departmentId)
            fail('POS_BRANCH_REQUIRED', 'Select a company or branch for POS', 403);
        if (ports.environment("NODE_ENV") === 'production' && operation.startsWith('connection.') && ports.environment("POS_STRIPE_LIVE_ENABLED") !== 'true')
            fail('POS_PROVIDER_COMING_SOON', 'Enable the Stripe production gate before connecting live accounts', 403);
        const hash = ports.hash(JSON.stringify({ user: actor.userId, assignment: actor.assignmentId, company: actor.companyId, branch: actor.branchId, operation, input }));
        try {
            return await db.transaction(async (tx) => {
                if (operation !== 'sale.reverse_room_charge' && !operation.startsWith('connection.'))
                    await tx.assertProductionModuleEnabled(actor.companyId, 'pos');
                const grant = await tx.readAssignments1({ assignmentId: actor.assignmentId, userId: actor.userId, companyId: actor.companyId, role: actor.role, fact4: ports.clock.now(), fact5: ports.clock.now(), branchId: actor.branchId });
                if (!grant)
                    fail('POS_ACCESS_CHANGED', 'Permissions changed. Select the role again', 403);
                await tx.batch([...tx.workContextGuards(actor, key), ...tx.iamMutationGuards(actor, key)]);
                // All user commands in one Company serialize with preparation/confirmation.
                // Stock reservations retain their own locks against other modules and jobs.
                await tx.writeContext1({ companyId: actor.companyId });
                const old = await tx.readPosReceipts1({ key: key });
                if (old) {
                    if (old.request_hash !== hash && old.request_hash !== ports.hash(JSON.stringify({ user: actor.userId, assignment: actor.assignmentId, company: actor.companyId, branch: actor.branchId, operation, input: legacyGuestAccount(input) })))
                        fail('IDEMPOTENCY_CONFLICT', 'Command number is already used for other data');
                    await tx.writeCommandAssertions1({ key: key });
                    return canonicalGuestAccount(json<Record<string, unknown>>(old.result_json));
                }
                const attachmentInput = z.object({ id: z.string().min(1), expectedVersion: z.number().int().positive(), attachmentSetIds: z.array(z.string().min(1)).max(20).default([]) }).passthrough().safeParse(input);
                const attachmentDescriptor = operation === 'sale.cancel' ? { targetType: 'pos_sale', purposeKey: 'void_evidence', bindingType: 'pos_void', componentRef: 'void' }
                    : operation === 'refund.approve' || operation === 'refund.reject' ? { targetType: 'pos_refund', purposeKey: 'refund_evidence', bindingType: 'pos_refund_decision', componentRef: 'refund-decision' }
                        : operation === 'shift.review' ? { targetType: 'pos_shift', purposeKey: 'cash_variance', bindingType: 'pos_cash_variance_review', componentRef: 'variance-review' } : null;
                const attachmentData = attachmentInput.success ? attachmentInput.data : null, attachmentSetIds = attachmentData?.attachmentSetIds ?? [];
                if (attachmentSetIds.length && !attachmentDescriptor)
                    fail('POS_ATTACHMENT_ACTION', 'Files can be bound only to a void, refund decision, or cash-variance review.', 422);
                if (attachmentSetIds.length && !attachmentData)
                    fail('POS_ATTACHMENT_INPUT', 'Choose evidence for the current record revision.', 422);
                if (attachmentSetIds.length) {
                    const placeholders = attachmentSetIds.map(() => '?').join(','), sets = (await tx.readAttachmentSets1({ companyId: actor.companyId, fact1: attachmentSetIds, placeholders: placeholders })).results;
                    if (sets.length !== new Set(attachmentSetIds).size || sets.some(set => set.component_kind !== 'action' || set.purpose_key !== attachmentDescriptor!.purposeKey))
                        fail('POS_ATTACHMENT_PURPOSE', 'Choose only the evidence purpose for this POS action.', 422);
                }
                const attachmentBinding = attachmentSetIds.length ? await tx.prepareAttachmentBinding(actor, { attachmentSetIds, bindingContract: { targetType: attachmentDescriptor!.targetType, bindingType: attachmentDescriptor!.bindingType, contexts: [{ purposeKey: attachmentDescriptor!.purposeKey, componentKind: 'action', componentRef: attachmentDescriptor!.componentRef }] }, targetId: attachmentData!.id, expectedTargetVersion: attachmentData!.expectedVersion, commandId: key, bindingId: `${attachmentData!.id}:${operation}:${attachmentData!.expectedVersion + 1}` }) : null;
                if (attachmentBinding)
                    await tx.batch(attachmentBinding.guards);
                const result = await perform(tx, actor, operation, input, key);
                if (attachmentBinding)
                    await tx.batch(attachmentBinding.steps);
                await tx.writePosReceipts1({ key: key, companyId: actor.companyId, hash: hash, fact3: JSON.stringify(result), fact4: ports.clock.now() });
                await audit(tx, actor.companyId, actor.branchId, actor.userId, operation, String(result.id ?? key), { input, result, assignmentId: actor.assignmentId });
                await tx.writeCommandAssertions1({ key: key });
                return result;
            });
        }
        catch (error) {
            if (error instanceof Error && error.message === 'QUALITY_HOLD')
                fail('QUALITY_HOLD', 'This product is quarantined. Quality inspection and independent release are required');
            if (['23505', '23514', '40001', '40P01'].includes((error as {
                code?: string;
            }).code ?? ''))
                fail('POS_CONFLICT', 'Data or reserved amounts changed. Load the latest data and try again');
            throw error;
        }
    }
    async function bootstrapRetail(db: PosUnit<Effect>, actor: WorkContext, input: unknown) {
        const p = z.object({ registerId: z.string().min(1).optional(), warehouseId: z.string().min(1).optional(), bankAccountId: z.string().min(1).optional(), mode: z.enum(['retail', 'restaurant']).default('retail'), registerName: z.string().trim().min(1).max(200).optional() }).strict().parse(input);
        const company = actor.companyId;
        if (actor.branchId)
            fail('POS_COMPANY_SETTINGS', 'Run retail setup from the Company workspace', 403);
        if (!await db.readOrganizationNodes1({ company: company }))
            fail('POS_COMPANY_SETTINGS', 'Company workspace not found', 404);
        if (await db.readPosSettings2({ company: company }))
            fail('POS_ALREADY_CONFIGURED', 'POS is already configured. Review the detailed POS settings instead.');
        const existingRetailRegisters = await db.readPosRegisters1({ company: company, mode: p.mode });
        const commerce = await db.readCommerceSettings1({ company: company });
        if (!commerce)
            fail('POS_COMMERCE_SETUP_REQUIRED', 'Set up Commerce and Finance before configuring POS', 409);
        const chartId = str(commerce, 'chart_id'), currency = str(commerce, 'currency');
        const chart = await db.readFinanceCharts2({ chartId: chartId, company: company });
        if (!chart)
            fail('POS_COMMERCE_CHART_REQUIRED', 'Select an active Commerce chart of accounts in Finance first', 409);
        if (str(chart, 'base_currency') !== currency)
            fail('POS_CURRENCY_MISMATCH', 'Commerce currency must match the chart base currency', 422);
        const accounts: Record<string, string> = { chartId };
        for (const source of posCommerceAccountDefaults) {
            const accountId = str(commerce, source.field);
            const account = accountId ? await db.readFinanceAccounts2({ accountId: accountId, company: company, chartId: chartId, category: source.category, currency: currency }) : null;
            if (!account)
                fail('POS_COMMERCE_ACCOUNT_INVALID', `Review the Commerce ${source.label} account in Finance before setting up POS`, 409);
            accounts[source.key] = accountId;
        }
        let registerId = '', registerName = '', warehouseId = '', branchId = '', reuseRegister = false;
        if (existingRetailRegisters.length) {
            const activeRegisters = existingRetailRegisters.filter(row => Boolean(row.active));
            if (!activeRegisters.length)
                fail('POS_RETAIL_REGISTER_EXISTS', 'An existing retail register is inactive. Reactivate it in Advanced POS settings first.');
            if (activeRegisters.length > 1 && !p.registerId)
                fail('POS_REGISTER_SELECTION_REQUIRED', 'Choose which active retail register will use the new POS settings', 422);
            const existingRegister = p.registerId ? activeRegisters.find(row => str(row, 'id') === p.registerId) : activeRegisters.length === 1 ? activeRegisters[0] : null;
            if (!existingRegister)
                fail('POS_REGISTER_INVALID', 'Choose an active retail register in this Company', 422);
            registerId = str(existingRegister, 'id');
            registerName = str(existingRegister, 'name');
            warehouseId = str(existingRegister, 'warehouse_id');
            branchId = str(existingRegister, 'branch_id');
            reuseRegister = true;
            if (p.warehouseId && p.warehouseId !== warehouseId)
                fail('POS_WAREHOUSE_INVALID', 'The selected register already uses a different warehouse', 422);
            const warehouse = await db.readCommerceWarehouses1({ warehouseId: warehouseId, company: company, branchId: branchId });
            if (!warehouse)
                fail('POS_WAREHOUSE_INVALID', 'The selected register must use an active warehouse in its branch', 409);
        }
        else {
            if (p.registerId)
                fail('POS_REGISTER_INVALID', 'Choose an active retail register in this Company', 422);
            const warehouses = await db.readCommerceWarehouses2({ company: company });
            if (!warehouses.length)
                fail('POS_WAREHOUSE_REQUIRED', 'Add an active warehouse in Commerce before setting up POS', 409);
            warehouseId = p.warehouseId ?? (warehouses.length === 1 ? str(warehouses[0], 'id') : '');
            const warehouse = warehouses.find(row => str(row, 'id') === warehouseId);
            if (warehouses.length > 1 && !p.warehouseId)
                fail('POS_WAREHOUSE_SELECTION_REQUIRED', 'Choose the warehouse this retail register will use', 422);
            if (!warehouse)
                fail('POS_WAREHOUSE_INVALID', 'Choose an active warehouse in this Company', 422);
            branchId = str(warehouse, 'branch_id');
        }
        const bankAccounts = await db.readFinanceAccounts3({ company: company, chartId: chartId, currency: currency });
        let bankAccountId = '';
        if (bankAccounts.length > 1 && !p.bankAccountId)
            fail('POS_BANK_ACCOUNT_SELECTION_REQUIRED', 'Choose which bank account POS transfers will use', 422);
        if (bankAccounts.length) {
            bankAccountId = p.bankAccountId ?? str(bankAccounts[0], 'id');
            if (!bankAccounts.some(row => str(row, 'id') === bankAccountId))
                fail('POS_BANK_ACCOUNT_INVALID', 'Choose an active bank account in the Commerce chart and currency', 422);
        }
        else if (p.bankAccountId) {
            fail('POS_BANK_ACCOUNT_INVALID', 'No eligible Finance bank account is available for POS transfers', 422);
        }
        const now = ports.clock.now(), createdAccounts: Array<{
            id: string;
            code: string;
            name: string;
            category: string;
        }> = [];
        for (const spec of posGeneratedAccountSpecs) {
            if (spec.key === 'bank' && bankAccountId) {
                accounts.bank = bankAccountId;
                continue;
            }
            let code: string = spec.code, suffix = 2;
            while (await db.readFinanceAccounts4({ chartId: chartId, code: code }))
                code = `${spec.code}-${suffix++}`;
            const accountId = id();
            await db.writeFinanceAccounts1({ accountId: accountId, chartId: chartId, company: company, code: code, name: spec.name, category: spec.category, fact6: spec.key === 'bank' ? currency : null, fact7: spec.controlKind === 'bank' ? 1 : 0, controlKind: spec.controlKind, now: now, now2: now });
            accounts[spec.key] = accountId;
            createdAccounts.push({ id: accountId, code, name: spec.name, category: spec.category });
        }
        if (!reuseRegister) {
            registerId = id();
            registerName = p.registerName ?? `Main ${p.mode} register`;
            let nameSuffix = 2;
            while (await db.readPosRegisters2({ company: company, branchId: branchId, registerName: registerName }))
                registerName = `${p.registerName ?? `Main ${p.mode} register`} ${nameSuffix++}`;
        }
        await db.writePosSettings1({ company: company, currency: currency, fact2: JSON.stringify(accounts) });
        if (!reuseRegister)
            await db.writePosRegisters1({ registerId: registerId, company: company, branchId: branchId, warehouseId: warehouseId, registerName: registerName, mode: p.mode });
        await db.prepareAccounting(company, accounts as PosAccounts, actor.userId);
        return { id: company, mode: p.mode, registerId, registerName, warehouseId, bankAccountId: accounts.bank, registerReused: reuseRegister, accountsCreated: createdAccounts };
    }
    async function perform(db: PosUnit<Effect>, actor: WorkContext, op: string, input: unknown, commandKey: string): Promise<Record<string, unknown>> {
        const company = actor.companyId, user = actor.userId;
        let approvedReversalId: string | null = null;
        let approvedReversalReason: string | null = null;
        if (op.startsWith('room_charge_reversal.')) {
            if (!["room_charge_reversal.request","room_charge_reversal.approve_pos","room_charge_reversal.reject_pos","room_charge_reversal.approve_pms","room_charge_reversal.reject_pms"].includes(op)) fail('POS_ACTION_UNKNOWN','Reversal command not found',404);
            if (op === 'room_charge_reversal.request') {
                const q = base.extend({ propertyId: z.string().min(1), expectedPropertyVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(1000), restockLineIds: z.array(z.string()).max(100).default([]) }).strict().parse(input);
                const sale = await saleRow(db, actor, q.id);
                version(sale, q);
                const source = await db.readPmsSources1({ fact0: `pos:${sale.id}:${sale.id}`, company, branch_id: sale.branch_id });
                const property = await db.readPmsProperties2({ propertyId: q.propertyId, company });
                if (!source || source.property_id !== q.propertyId || !property || property.branch_id !== sale.branch_id)
                    fail('POS_ROOM_CHARGE_MISSING', 'The linked hotel charge was not found', 404);
                if (num(property, 'version') !== q.expectedPropertyVersion)
                    fail('PMS_VERSION_CONFLICT', 'Hotel data changed. Reload and request again.');
                if (sale.state !== 'completed' || await db.readPosRoomChargeReversals1({ id: sale.id }) || await db.readPosPayments6({ id: sale.id }))
                    fail('POS_ROOM_CHARGE_REVERSED', 'Only an unreversed full room charge can be requested.');
                if (await db.pendingReversal(q.id))
                    fail('POS_REVERSAL_PENDING', 'Review or reject the existing reversal request first.');
                const lines = await db.readPosLines2({ id: sale.id });
                if (new Set(q.restockLineIds).size !== q.restockLineIds.length || q.restockLineIds.some(lineId => !lines.some(line => line.id === lineId && line.kind === 'stock' && line.fulfillment_state === 'delivered')))
                    fail('POS_RETURN_NOT_STOCK', 'Select each delivered stock item at most once.', 422);
                const requestId = id(), now = ports.clock.now();
                await db.saveReversalRequest({ id: requestId, sale_id: q.id, company_id: company, branch_id: sale.branch_id, property_id: q.propertyId, sale_version: q.expectedVersion, property_version: q.expectedPropertyVersion, amount_minor: sale.total_minor, reason: q.reason, restock_line_ids_json: JSON.stringify(q.restockLineIds), requested_by: user, created_at: now });
                return { id: requestId, saleId: q.id, state: 'requested', version: 1 };
            }
            const q = base.extend({ reason: z.string().trim().min(3).max(1000) }).strict().parse(input);
            const request = scopeRow(actor, await db.reversalRequest(q.id));
            version(request, q);
            const pms = op.endsWith('_pms'), reject = op.includes('.reject_');
            if ((!pms && request.state !== 'requested') || (pms && request.state !== 'pos_approved'))
                fail('POS_REVERSAL_STAGE', 'Complete POS approval before PMS approval.');
            const sale = await saleRow(db, actor, str(request, 'sale_id'));
            const chargeAudit = await db.readPosAudit1({ id: sale.id });
            if (samePerson(actor, request.requested_by) || samePerson(actor, sale.created_by) || chargeAudit && samePerson(actor, chargeAudit.actor_id) || pms && samePerson(actor, request.pos_approved_by))
                fail('POS_INDEPENDENT_REVIEW', 'The requester, original operator and two approvers must be different people.');
            if (reject) {
                await db.reviewReversalRequest(q.id, 'rejected', user, q.reason, ports.clock.now());
                return { id: q.id, saleId: sale.id, state: 'rejected', version: q.expectedVersion + 1 };
            }
            if (sale.version !== request.sale_version || sale.total_minor !== request.amount_minor)
                fail('POS_REVERSAL_STALE', 'The bill changed. Reject this request and request again.');
            const property = await db.readPmsProperties2({ propertyId: str(request, 'property_id'), company });
            if (!property || property.version !== request.property_version || property.state !== 'open')
                fail('POS_REVERSAL_STALE', 'Hotel data changed. Reject this request and request again.');
            if (!pms) {
                await db.reviewReversalRequest(q.id, 'pos_approved', user, q.reason, ports.clock.now());
                return { id: q.id, saleId: sale.id, state: 'pos_approved', version: q.expectedVersion + 1 };
            }
            approvedReversalId = q.id;
            approvedReversalReason = q.reason;
            input = { id: sale.id, expectedVersion: request.sale_version, propertyId: request.property_id, expectedPropertyVersion: request.property_version, reason: request.reason, restockLineIds: json<string[]>(request.restock_line_ids_json) };
            op = 'sale.reverse_room_charge';
        }
        if (op === 'accounting.approve') {
            const q = z.object({ expectedVersion: z.number().int().positive() }).strict().parse(input), settings = await config(db, company);
            if (actor.branchId) fail('POS_COMPANY_SETTINGS', 'Review accounting at Company scope', 403);
            version(settings, q);
            if (!settings.accounting_prepared_by || samePerson(actor, settings.accounting_prepared_by))
                fail('POS_INDEPENDENT_REVIEW', 'Another accounting reviewer must approve the mapping.');
            await db.approveAccounting(company, user);
            return { id: company, state: 'approved', version: q.expectedVersion + 1 };
        }
        if (op === 'settings.operations') {
            const q = z.object({ expectedVersion: z.number().int().positive(), enabled: z.boolean(), cashierDiscountBps: z.number().int().min(0).max(10000) }).strict().parse(input), settings = await config(db, company);
            if (actor.branchId) fail('POS_COMPANY_SETTINGS', 'Configure company policy at Company scope', 403);
            version(settings, q);
            await db.saveOperationalSettings(company, q.enabled, q.cashierDiscountBps);
            return { id: company, version: q.expectedVersion + 1 };
        }
        if (op.startsWith('day.'))
            return db.posDayCommand(actor, op, input);
        if (op === 'customer.create')
            return db.createPosCustomer(actor, input);
        if (op === 'connection.create' || op === 'connection.refresh') {
            if (actor.branchId)
                fail('POS_COMPANY_SETTINGS', 'Connect Stripe from the company area', 403);
            const p = z.object({ livemode: z.boolean().default(false) }).parse(input), job = id();
            await enqueue(db, company, op === 'connection.create' ? 'connect' : 'connect_refresh', job, { livemode: p.livemode, userId: user, holdingSlug: actor.holdingSlug, assignmentId: actor.assignmentId });
            return { id: job };
        }
        if (op === 'connection.oauth') {
            if (actor.branchId)
                fail('POS_COMPANY_SETTINGS', 'Connect Stripe from the company area', 403);
            const p = z.object({ livemode: z.boolean().default(false) }).parse(input), state = id();
            await db.writePosOauthStates1({ state: state, company: company, user: user, fact3: ports.clock.now() + 600000, livemode: p.livemode });
            return { id: state, livemode: p.livemode };
        }
        if (op === 'settings.bootstrap')
            return bootstrapRetail(db, actor, input);
        if (op === 'settings.save' || op === 'accounting.prepare') {
            if (actor.branchId)
                fail('POS_COMPANY_SETTINGS', 'Configure accounts from the company area', 403);
            const p = settingsSchema.parse(input), old = await db.readPosSettings3({ company: company });
            if ((old ? num(old, 'version') : 0) !== p.expectedVersion)
                fail('POS_VERSION_CONFLICT', 'Settings changed');
            for (const k of accountKeys)
                if (!await db.readFinanceAccounts5({ fact0: p.accounts[k], company: company, chartId: p.accounts.chartId }))
                    fail('POS_ACCOUNT_INVALID', `Review account ${k}`, 422);
            const chart = await db.readFinanceCharts3({ chartId: p.accounts.chartId, company: company });
            if (chart?.base_currency !== p.currency)
                fail('POS_CURRENCY_MISMATCH', "Currency must match the chart's base currency", 422);
            const commerce = await db.readCommerceSettings2({ company: company });
            if (commerce?.currency !== p.currency)
                fail('POS_CURRENCY_MISMATCH', 'POS currency must match Commerce and the company accounts', 422);
            await db.writePosSettings2({ company: company, enabled: hasPermission(actor,"pos.configure") ? p.enabled : Boolean(old?.enabled), currency: p.currency, timezone: p.timezone, fact4: JSON.stringify(p.accounts), cashierDiscountBps: hasPermission(actor,"pos.configure") ? p.cashierDiscountBps : num(old ?? {},"cashier_discount_bps") });
            await db.prepareAccounting(company, p.accounts, user);
            return { id: company, state: 'prepared' };
        }
        if (op === 'register.save') {
            const p = z.object({ id: z.string().optional(), propertyId: z.string().min(1).nullable().optional(), warehouseId: z.string(), name: z.string().trim().min(1).max(100), mode: z.enum(['retail', 'restaurant']), active: z.boolean().default(true), expectedVersion: z.number().int().min(0).default(0) }).parse(input);
            const warehouse = scopeRow(actor, await db.readCommerceWarehouses3({ warehouseId: p.warehouseId }));
            const rid = p.id ?? id();
            if (p.propertyId) {
                const property = await db.readPmsProperties2({ propertyId: p.propertyId, company });
                if (p.mode !== 'restaurant' || !property || property.branch_id !== warehouse.branch_id) fail('POS_PROPERTY_SCOPE', 'Link a hotel in this restaurant branch.', 422);
            }
            if (p.id) {
                const previous = scopeRow(actor, await db.readPosRegisters3({ id: p.id }));
                version(previous, p);
                if (await db.readPosShifts1({ id: p.id }))
                    fail('POS_REGISTER_OPEN', 'Close the shift before editing the sales point');
                await db.writePosRegisters2({ warehouseId: p.warehouseId, branch_id: warehouse.branch_id, name: p.name, mode: p.mode, active: p.active, rid: rid });
            }
            else
                await db.writePosRegisters3({ rid: rid, company: company, branch_id: warehouse.branch_id, warehouseId: p.warehouseId, name: p.name, mode: p.mode, active: p.active });
            if (p.propertyId !== undefined) await db.linkRegister(rid, p.propertyId);
            return { id: rid };
        }
        if (op === 'table.create') {
            const p = z.object({ registerId: z.string(), name: z.string().trim().min(1).max(40), seats: z.number().int().min(1).max(100) }).parse(input);
            scopeRow(actor, await db.readPosRegisters4({ registerId: p.registerId }));
            if (await db.readPosTables1({ registerId: p.registerId, name: p.name }))
                fail('POS_TABLE_NAME_EXISTS', 'A table with this name already exists at this register', 422);
            const tid = id();
            await db.writePosTables1({ tid: tid, registerId: p.registerId, name: p.name, seats: p.seats });
            return { id: tid };
        }
        if (op === 'catalog.image') {
            const p = z.object({ productId: z.string().min(1), imageId: z.string().min(1), expectedVersion: z.number().int().positive() }).parse(input);
            const c = await db.readPosCatalog1({ productId: p.productId, company: company });
            if (!c)
                fail('POS_PRODUCT_INVALID', 'Catalog not found', 404);
            version(c, p);
            if (actor.branchId && !await db.readPosRegisterCatalog1({ productId: p.productId, branchId: actor.branchId, company: company }))
                fail('POS_PRODUCT_INVALID', 'Catalog is outside this Branch', 404);
            await db.requirePosImageSource(actor, p.imageId, p.productId, commandKey);
            await db.writePosCatalog1({ imageId: p.imageId, productId: p.productId });
            return { id: p.productId, imageId: p.imageId };
        }
        if (op === 'catalog.save') {
            const p = catalogSchema.parse(input);
            if (!await db.readCommerceProducts1({ productId: p.productId, company: company }))
                fail('POS_PRODUCT_INVALID', 'Select a company product', 422);
            if (p.kind !== 'recipe' && p.modifiers.some(m => m.ingredients.length))
                fail('POS_RECIPE_REQUIRED', 'Ingredient modifiers require a recipe menu', 422);
            if (p.kind === 'recipe' && !p.recipe.length)
                fail('POS_RECIPE_REQUIRED', 'Enter recipe ingredients', 422);
            if (new Set(p.modifiers.map(m => m.id)).size !== p.modifiers.length)
                fail('POS_MODIFIER_DUPLICATE', 'Duplicate modifier ID', 422);
            for (const part of [...p.recipe, ...p.modifiers.flatMap(m => m.ingredients)])
                if (!await db.readCommerceProducts2({ productId: part.productId, company: company }))
                    fail('POS_INGREDIENT_INVALID', 'Ingredients must be company products', 422);
            const old = await db.readPosCatalog2({ productId: p.productId });
            if ((old ? num(old, 'version') : 0) !== p.expectedVersion)
                fail('POS_VERSION_CONFLICT', 'Product data changed');
            if (p.imageId)
                await db.requirePosImageSource(actor, p.imageId, p.productId, commandKey);
            const registerIds = p.registerIds ?? (old ? undefined : (await db.readPosRegisters5({ company: company, branchId: actor.branchId, branchId2: actor.branchId, kind: p.kind })).map(r => str(r, 'id')));
            if (registerIds) {
                for (const rid of registerIds) {
                    const register = scopeRow(actor, await db.readPosRegisters6({ rid: rid }));
                    if (p.kind === 'recipe' && register.mode !== 'restaurant')
                        fail('POS_RESTAURANT_REQUIRED', 'Recipes require restaurant registers', 422);
                }
            }
            await db.writePosCatalog2({ productId: p.productId, company: company, fact2: p.barcode || null, category: p.category, kind: p.kind, station: p.station, fact6: JSON.stringify(p.recipe), fact7: JSON.stringify(p.modifiers), taxInclusive: p.taxInclusive, enabled: p.enabled, fact10: p.imageId === undefined ? old?.image_id ?? null : p.imageId });
            if (registerIds) {
                if (actor.branchId)
                    await db.writePosRegisterCatalog1({ productId: p.productId, company: company, branchId: actor.branchId });
                else
                    await db.writePosRegisterCatalog2({ productId: p.productId });
                for (const rid of new Set(registerIds))
                    await db.writePosRegisterCatalog3({ rid: rid, productId: p.productId });
            }
            return { id: p.productId };
        }
        if (op === 'shift.open') {
            const p = z.object({ registerId: z.string(), openingMinor: money }).parse(input), register = scopeRow(actor, await db.readPosRegisters7({ registerId: p.registerId })), settings = await config(db, company);
            if (!settings.accounting_approved_by) fail('POS_ACCOUNTING_REVIEW_REQUIRED', 'Finance must approve the posting accounts before selling.');
            if (!settings.enabled)
                fail('POS_DISABLED', 'POS transactions are not enabled');
            const hotel = register.property_id ? await db.readPmsProperties1({ company: company, branch_id: register.branch_id }) : null;
            if (hotel && (hotel.state !== 'open' || hotel.business_date !== businessDate()))
                fail('POS_HOTEL_DAY_CLOSED', 'Finish or recover the hotel day close before opening a restaurant cash session');
            await db.assertPosDayWritable(company, str(register, 'branch_id'), businessDate());
            await openPeriod(db, company, businessDate());
            const sid = id();
            await db.writePosShifts1({ sid: sid, id: register.id, company: company, branch_id: register.branch_id, user: user, fact5: ports.clock.now(), openingMinor: p.openingMinor, fact7: businessDate() });
            return { id: sid };
        }
        if (op.startsWith('shift.') || op.startsWith('cash.')) {
            const p = base.extend({ amountMinor: z.number().int().min(-99999999999).max(99999999999).optional(), countedMinor: money.optional(), denominations: z.record(z.string(), z.number().int().nonnegative().max(100000)).optional(), reason: z.string().trim().min(3).max(1000).optional(), movementId: z.string().optional() }).parse(input);
            const shift = scopeRow(actor, await db.readPosShifts2({ id: p.id }));
            version(shift, p);
            await db.assertPosDayWritable(company, str(shift, 'branch_id'), str(shift, 'business_date'));
            const settings = await config(db, company), a = json<PosAccounts>(settings.accounts_json);
            if (op === 'cash.create') {
                if (shift.state !== 'open' || !p.amountMinor || !p.reason)
                    fail('POS_CASH_INVALID', 'Enter the amount and reason in an open cash session', 422);
                const mid = id();
                await db.writePosCashMovements1({ mid: mid, id: shift.id, amountMinor: p.amountMinor, reason: p.reason, user: user, null: null, fact6: ports.clock.now() });
                await db.writePosShifts2({ id: shift.id });
                return { id: mid };
            }
            if (op === 'cash.approve') {
                if (shift.state !== 'open')
                    fail('POS_SHIFT_CLOSED', 'Cash session is closed');
                const m = await db.readPosCashMovements1({ movementId: p.movementId, id: shift.id });
                if (!m || m.approved_by || samePerson(actor, m.created_by))
                    fail('POS_INDEPENDENT_REVIEW', 'A different approver must review the record');
                await posting(db, company, str(shift, 'branch_id'), a, businessDate(), str(settings, 'currency'), `cash:${m.id}`, str(shift, 'id'), user, pair(num(m, 'amount_minor') > 0 ? a.cash : a.cashTransfer, num(m, 'amount_minor') > 0 ? a.cashTransfer : a.cash, Math.abs(num(m, 'amount_minor'))));
                await db.writePosCashMovements2({ user: user, id: m.id });
            }
            else if (op === 'shift.close') {
                if (shift.state !== 'open' || p.countedMinor === undefined)
                    fail('POS_SHIFT_INVALID', 'Enter the actual counted amount', 422);
                if (await db.readPosSales3({ id: shift.id }))
                    fail('POS_OPEN_SALES', 'Close or cancel open bills before closing the cash session');
                if (await db.readPosCashMovements2({ id: shift.id }))
                    fail('POS_CASH_REVIEW_PENDING', 'There are unapproved cash movements');
                const cash = await db.readPosPayments5({ id: shift.id });
                const refunds = await db.readPosRefundPayments1({ id: shift.id });
                const movements = await db.readPosCashMovements3({ id: shift.id });
                const expected = num(shift, 'opening_minor') + num(cash!, 'n') - num(refunds!, 'n') + num(movements!, 'n');
                if (p.denominations) {
                    const denominations = [100000, 50000, 10000, 5000, 2000, 1000, 500, 200, 100, 50, 25];
                    if (Object.keys(p.denominations).some(k => !denominations.includes(Number(k))) || Object.entries(p.denominations).reduce((n, [k, v]) => n + Number(k) * v, 0) !== p.countedMinor)
                        fail('POS_CASH_COUNT', 'Denomination counts must match the counted amount', 422);
                }
                await db.writePosShifts3({ countedMinor: p.countedMinor, expected: expected, fact2: p.countedMinor - expected, fact3: ports.clock.now(), user: user, fact5: p.denominations ? JSON.stringify(p.denominations) : null, id: shift.id });
            }
            else if (op === 'shift.review') {
                if (shift.state !== 'closed' || samePerson(actor, shift.opened_by) || samePerson(actor, shift.closed_by) || !p.reason)
                    fail('POS_INDEPENDENT_REVIEW', 'A different manager must review the total with a reason');
                const variance = num(shift, 'variance_minor');
                await posting(db, company, str(shift, 'branch_id'), a, businessDate(), str(settings, 'currency'), `shift:${shift.id}`, str(shift, 'id'), user, pair(variance > 0 ? a.cash : a.cashDifference, variance > 0 ? a.cashDifference : a.cash, Math.abs(variance)));
                await db.writePosShifts4({ user: user, reason: p.reason, id: shift.id });
            }
            else
                fail('POS_ACTION_UNKNOWN', 'Command not found', 404);
            await db.writePosShifts2({ id: shift.id });
            return { id: shift.id };
        }
        if (op === 'sale.create') {
            const p = z.object({ shiftId: z.string(), tableId: z.string().nullable().optional(), guests: z.number().int().min(1).max(100).default(1), partyId: z.string().nullable().optional(), serviceMode: z.enum(['retail', 'dine_in', 'takeaway']), currency: z.string().regex(/^[A-Z]{3}$/).refine(value => Boolean(currencyDefinition(value))).optional() }).parse(input);
            const shift = scopeRow(actor, await db.readPosShifts3({ shiftId: p.shiftId })), settings = await config(db, company);
            if (!settings.accounting_approved_by) fail('POS_ACCOUNTING_REVIEW_REQUIRED', 'Finance must approve the posting accounts before selling.');
            if (!settings.enabled)
                fail('POS_DISABLED', 'New transactions are temporarily paused');
            if (p.currency && p.currency !== settings.currency)
                fail('POS_CURRENCY_SETUP_REQUIRED', `The selected ${p.currency} currency needs a POS register, supported payment accounts and pricing setup before a new sale can start.`, 422);
            if (p.currency && p.currency !== 'USD' && !await db.readCompanyFxRates1({ company: company, currency: p.currency }))
                fail('MANUAL_RATE_REQUIRED', `Confirm a manual USD/${p.currency} rate before starting this sale.`, 422);
            const register = (await db.readPosRegisters8({ register_id: shift.register_id }))!;
            await db.assertHotelDay(company, shift.register_id, businessDate());
            await db.assertPosDayWritable(company, str(shift, 'branch_id'), businessDate());
            if (shift.business_date !== businessDate())
                fail('POS_SHIFT_DATE', 'Close the previous business day cash session before selling');
            if (!register)
                fail('POS_REGISTER_DISABLED', 'Sales point is disabled');
            if (p.serviceMode === 'retail' && register.mode !== 'retail' || p.serviceMode !== 'retail' && register.mode !== 'restaurant')
                fail('POS_MODE_INVALID', 'Mode does not match the sales point', 422);
            if (p.tableId && !await db.readPosTables2({ tableId: p.tableId, id: register.id }))
                fail('POS_TABLE_INVALID', 'Table is not at this sales point', 422);
            if (p.partyId && !await db.readFinanceParties1({ partyId: p.partyId, company: company }))
                fail('POS_CUSTOMER_INVALID', 'Customer is not available', 422);
            const sid = id();
            await db.writePosSales4({ sid: sid, company: company, branch_id: shift.branch_id, id: register.id, id2: shift.id, fact5: `POS-${businessDate().replaceAll('-', '')}-${sid.slice(0, 8).toUpperCase()}`, currency: settings.currency, fact7: businessDate(), fact8: p.partyId ?? null, fact9: p.tableId ?? null, guests: p.guests, serviceMode: p.serviceMode, user: user, fact13: ports.clock.now(), fact14: ports.clock.now(), accounts_json: settings.accounts_json });
            const issuer = await db.readOrganizationNodes2({ company: company });
            await db.writePosSales5({ fact0: JSON.stringify({ companyName: issuer?.display_name ?? company, registerName: register.name, branchId: shift.branch_id }), sid: sid });
            return { id: sid };
        }
        if (op.startsWith('sale.')) {
            const p = base.passthrough().parse(input), sale = await saleRow(db, actor, p.id);
            version(sale, p);
            if (op === 'sale.reverse_room_charge') {
                if (!approvedReversalId) fail('POS_REVERSAL_WORKFLOW_REQUIRED','Independent POS and PMS approvals are required.');
                if (await db.readPosDayCloses1({company_id:sale.company_id,branch_id:sale.branch_id,business_date:sale.business_date})) fail('POS_DAY_REVIEW_PENDING','Finance must confirm the original day close before correction.');
            }
            await db.assertHotelDay(company, sale.register_id, op === 'sale.reverse_room_charge' ? businessDate() : str(sale, 'business_date'));
            await db.assertPosDayWritable(company, str(sale, 'branch_id'), op === 'sale.reverse_room_charge' ? businessDate() : str(sale, 'business_date'));
            if (op === 'sale.reverse_room_charge') {
                if (await db.readPosDayCloses1({ company_id: sale.company_id, branch_id: sale.branch_id, business_date: sale.business_date }))
                    fail('POS_DAY_REVIEW_PENDING', 'Finance must confirm the original day close before a room-charge correction can change its source records');

                const q = base.extend({ propertyId: z.string().min(1), expectedPropertyVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(1000), restockLineIds: z.array(z.string()).max(100).default([]) }).parse(input);
                if (sale.state !== 'completed' || await db.readPosRoomChargeReversals1({ id: sale.id }))
                    fail('POS_ROOM_CHARGE_REVERSED', 'This room charge is already reversed or the bill is not complete');
                if (await db.readPosPayments6({ id: sale.id }))
                    fail('POS_ROOM_CHARGE_PAYMENT', 'This bill has POS payments and cannot use room-charge reversal');
                const source = await db.readPmsSources1({ fact0: `pos:${sale.id}:${sale.id}`, company: company, branch_id: sale.branch_id });
                if (!source || source.property_id !== q.propertyId)
                    fail('POS_ROOM_CHARGE_MISSING', 'The linked hotel charge was not found');
                if (new Set(q.restockLineIds).size !== q.restockLineIds.length)
                    fail('POS_DUPLICATE_LINE', 'Select each restocked line once', 422);
                const lines = await db.readPosLines2({ id: sale.id });
                if (q.restockLineIds.some(lineId => !lines.some(line => line.id === lineId && line.kind === 'stock' && line.fulfillment_state === 'delivered')))
                    fail('POS_RETURN_NOT_STOCK', 'Only delivered stock items can be restocked', 422);
                const reversal = await db.reversePosRoomCharge(actor, { propertyId: q.propertyId, sourceKey: `pos:${sale.id}:${sale.id}`, expectedPropertyVersion: q.expectedPropertyVersion, reason: q.reason, total: num(sale, 'total_minor') });
                const accounts = json<PosAccounts>(sale.accounts_json), date = reversal.businessDate;
                let restored = 0, wasted = 0;
                await db.assertHotelDay(company, sale.register_id, op === 'sale.reverse_room_charge' ? businessDate() : str(sale, 'business_date'));
            await db.assertPosDayWritable(company, str(sale, 'branch_id'), date);
                await openPeriod(db, company, date);
                for (const line of lines) {
                    const prior = await db.readPosLines3({ fulfillment_id: line.fulfillment_id, id: line.id });
                    const cost = portion(num(line, 'fulfillment_cost'), num(prior!, 'n'), num(line, 'quantity_milli'), num(line, 'fulfillment_quantity'));
                    if (q.restockLineIds.includes(str(line, 'id'))) {
                        const stock = await db.readCommerceStock3({ warehouse_id: line.warehouse_id, product_id: line.product_id });
                        if (!stock)
                            fail('POS_STOCK_MISSING', 'The original warehouse stock is unavailable');
                        await db.writeCommerceStock2({ quantity_milli: line.quantity_milli, cost: cost, warehouse_id: line.warehouse_id, product_id: line.product_id });
                        await db.writePosStockMoves2({ fact0: id(), company: company, warehouse_id: line.warehouse_id, product_id: line.product_id, fulfillment_id: line.fulfillment_id, id: sale.id, number: sale.number, fact7: 'return', quantity_milli: line.quantity_milli, cost: cost, date: date, fact11: ports.clock.now() });
                        restored += cost;
                    }
                    else
                        wasted += cost;
                }
                await posting(db, company, str(sale, 'branch_id'), accounts, date, str(sale, 'currency'), `room-reversal:${sale.id}`, str(sale, 'id'), user, [...pair(accounts.inventory, accounts.cogs, restored), ...pair(accounts.waste, accounts.cogs, wasted)]);
                await db.writePosRoomChargeReversals1({ id: sale.id, company: company, propertyId: q.propertyId, entryId: reversal.entryId, reversalEntryId: reversal.reversalEntryId, reason: q.reason, fact6: JSON.stringify(q.restockLineIds), restored: restored, wasted: wasted, user: user, fact10: ports.clock.now() });
                await capturePosReversal(db, sale, 'pos_room_charge_reversal', str(sale, 'id'), 'total', date, String(sale.total_minor), { sourceType: 'pos_sale', sourceId: str(sale, 'id'), amountKind: 'total' });
                await db.writePosSales6({ fact0: ports.clock.now(), id: sale.id });
                await db.reviewReversalRequest(approvedReversalId!, 'approved', user, approvedReversalReason!, ports.clock.now());
                return { id: approvedReversalId, saleId: sale.id, state: 'room_charge_reversed', reversalEntryId: reversal.reversalEntryId, restoredCostMinor: restored, wastedCostMinor: wasted };
            }
            if (op === 'sale.complete')
                return completeSale(db, sale, user);
            if (op === 'sale.charge_to_room') {
                if (ports.environment("NODE_ENV") === 'production' && ports.environment("ERPX_POS_ROOM_CHARGE_PILOT_ENABLED") !== 'true')
                    fail('POS_ROOM_CHARGE_PILOT_GATE', 'Room charges require pilot acceptance before production activation', 403);
                const p = base.extend({ propertyId: z.string().min(1), stayId: z.string().min(1), expectedPropertyVersion: z.number().int().positive() }).parse(input);
                if (sale.state !== 'open' && sale.state !== 'held')
                    fail('POS_SALE_LOCKED', 'Only an unpaid bill can be charged to a room');
                const linkedRegister = await db.readPosRegisters9({ register_id: sale.register_id });
                if (!linkedRegister || linkedRegister.property_id !== p.propertyId)
                    fail('POS_ROOM_CHARGE_REGISTER', 'Only restaurant bills can be charged to a room');
                if (!await db.readPosShifts4({ shift_id: sale.shift_id }))
                    fail('POS_SHIFT_CLOSED', 'Cash session is closed');
                const settings = await config(db, company);
                if (!settings.accounting_approved_by) fail('POS_ACCOUNTING_REVIEW_REQUIRED', 'Finance must approve the posting accounts before selling.');
            if (!settings.enabled)
                    fail('POS_DISABLED', 'New transactions are temporarily paused');
                if (await db.readPosPayments7({ id: sale.id }))
                    fail('POS_PAYMENT_STARTED', 'Room charge requires the full unpaid bill');
                const lines = await db.readPosLines1({ id: sale.id });
                if (!lines.length)
                    fail('POS_EMPTY_SALE', 'Add items before closing the bill', 422);
                const gross = await db.readPosLines5({ id: sale.id });
                if (num(sale, 'discount_minor') > scaledAmount(num(gross!, 'n'), num(settings, 'cashier_discount_bps'), 10000) && !sale.discount_approved_by)
                    fail('POS_DISCOUNT_APPROVAL', 'This discount must be approved before room charge');
                await openPeriod(db, company, str(sale, 'business_date'));
                const property = await db.readPmsProperties2({ propertyId: p.propertyId, company: company });
                if (!property || property.branch_id !== sale.branch_id)
                    fail('POS_PROPERTY_SCOPE', 'Select a hotel in this restaurant branch', 404);
                if (property.business_date !== sale.business_date)
                    fail('POS_BUSINESS_DATE', 'Restaurant and hotel business dates must match');
                for (const itemId of new Set(lines.map(l => str(l, 'fulfillment_id')))) {
                    const f = (await db.readPosFulfillments1({ itemId: itemId }))!;
                    if (f.state === 'cancelled')
                        fail('POS_ITEM_CANCELLED', 'Remove cancelled items before closing the bill');
                    if (f.kind === 'recipe' && f.state !== 'delivered')
                        fail('POS_SEND_KITCHEN', 'Deliver kitchen items before charging the room');
                    if (f.kind !== 'recipe' && f.state !== 'delivered') {
                        await consume(db, f, user, false);
                        await db.writePosFulfillments3({ id: f.id });
                    }
                }
                const result = await db.applyPosRoomCharge(actor, {
                    source: 'pos', sourceId: str(sale, 'id'), sourceLineId: str(sale, 'id'), companyId: company,
                    branchId: str(sale, 'branch_id'), propertyId: p.propertyId, stayId: p.stayId,
                    currency: str(sale, 'currency'), revenueOwner: 'pms', sourceJournalId: null,
                    description: `Restaurant bill ${str(sale, 'number')}`, net: num(sale, 'subtotal_minor'),
                    service: 0, tax: num(sale, 'tax_minor'), total: num(sale, 'total_minor'),
                }, p.expectedPropertyVersion);
                await capturePosMoney(db, sale, 'pos_sale', str(sale, 'id'), 'total', str(sale, 'business_date'), String(sale.total_minor));
                await db.writePosSales7({ fact0: ports.clock.now(), id: sale.id });
                await db.capturePosReceipt(str(sale, 'id'));
                return { id: sale.id, state: 'completed', tender: 'room_charge', propertyId: p.propertyId, stayId: p.stayId, guestAccountEntryId: result.entryId };
            }
            if (op === 'sale.approve_discount') {
                const q = z.object({ reason: z.string().trim().min(3).max(1000) }).parse(p);
                if (samePerson(actor, sale.discount_requested_by ?? sale.created_by))
                    fail('POS_INDEPENDENT_REVIEW', 'A different manager must approve the discount');
                await db.writePosSales8({ user: user, reason: q.reason, id: sale.id });
                return { id: sale.id };
            }
            await editable(db, sale);
            if (op === 'sale.add') {
                await db.writePosSales9({ user: user, id: sale.id });
                const q = z.object({ productId: z.string(), quantityMilli: qty, modifierIds: z.array(z.string()).max(30).default([]), discountMinor: money.default(0), note: z.string().max(500).default('') }).parse(p);
                const product = await db.readCommerceProducts3({ register_id: sale.register_id, productId: q.productId, company: company });
                if (!product)
                    fail('POS_PRODUCT_INVALID', 'สินค้านี้ไม่ได้เปิดขายใน catalog ของจุดขาย กรุณาให้ผู้ดูแลเปิดขายก่อน', 422);
                const validation = z.object({ expectedCatalogVersion: z.number().int().positive().optional(), expectedPriceMinor: money.optional() }).parse(p);
                if (validation.expectedCatalogVersion !== undefined && validation.expectedCatalogVersion !== num(product, 'catalog_version') || validation.expectedPriceMinor !== undefined && validation.expectedPriceMinor !== num(product, 'price_minor'))
                    fail('POS_CATALOG_CHANGED', 'ราคา/ตัวเลือกเปลี่ยน กรุณาตรวจตะกร้าก่อนส่งใหม่');
                const modifiers = json<z.infer<typeof catalogSchema>['modifiers']>(product.modifiers_json ?? '[]').filter(m => q.modifierIds.includes(m.id));
                if (modifiers.length !== q.modifierIds.length)
                    fail('POS_MODIFIER_INVALID', 'Invalid modifiers', 422);
                const kind = str(product, 'kind') || 'stock', price = num(product, 'price_minor') + modifiers.reduce((n, m) => n + m.priceMinor, 0), amounts = saleAmounts(q.quantityMilli, price, num(product, 'tax_bps'), Boolean(product.tax_inclusive), q.discountMinor);
                if (kind === 'recipe' && sale.service_mode === 'retail')
                    fail('POS_RESTAURANT_REQUIRED', 'Prepared menus require a restaurant sales point', 422);
                const recipe = kind === 'stock' ? [{ productId: q.productId, quantityMilli: 1000 }] : kind === 'recipe' ? json<Array<{
                    productId: string;
                    quantityMilli: number;
                }>>(product.recipe_json) : [];
                recipe.push(...modifiers.flatMap(m => m.ingredients));
                const register = (await db.readPosRegisters10({ register_id: sale.register_id }))!, fid = id(), lid = id();
                await db.writePosFulfillments5({ fid: fid, company: company, branch_id: sale.branch_id, warehouse_id: register.warehouse_id, productId: q.productId, quantityMilli: q.quantityMilli, kind: kind, fact7: str(product, 'station') || 'Kitchen', note: q.note, fact9: JSON.stringify(recipe), accounts_json: sale.accounts_json, currency: sale.currency, business_date: sale.business_date, fact13: ports.clock.now() });
                await db.writePosLines1({ lid: lid, id: sale.id, fid: fid, productId: q.productId, name: product.name, quantityMilli: q.quantityMilli, price: price, tax_bps: product.tax_bps, fact8: Boolean(product.tax_inclusive), discountMinor: q.discountMinor, subtotal: amounts.subtotal, tax: amounts.tax, fact12: JSON.stringify(modifiers), note: q.note });
            }
            else if (op === 'sale.customer') {
                const q = z.object({ partyId: z.string().nullable() }).parse(p);
                if (q.partyId && !await db.readFinanceParties2({ partyId: q.partyId, company: company }))
                    fail('POS_CUSTOMER_INVALID', 'Customer is not available', 422);
                await db.writePosSales10({ partyId: q.partyId, id: sale.id });
            }
            else if (op === 'sale.update_line') {
                const q = z.object({ lineId: z.string(), quantityMilli: qty, discountMinor: money, note: z.string().max(500).default('') }).parse(p);
                const line = await db.readPosLines6({ lineId: q.lineId, id: sale.id });
                if (!line)
                    fail('POS_LINE_INVALID', 'Line is not on this bill', 422);
                if (q.quantityMilli !== num(line, 'quantity_milli') && line.fulfillment_state !== 'draft')
                    fail('POS_ITEM_IN_KITCHEN', 'รายการที่ส่งครัวแล้วต้องแยกบิล หรือให้ผู้อนุมัติยกเลิกพร้อมบันทึกของเสีย');
                if (line.fulfillment_state === 'draft')
                    await db.writePosFulfillments6({ quantityMilli: q.quantityMilli, note: q.note, fulfillment_id: line.fulfillment_id });
                const amounts = saleAmounts(q.quantityMilli, num(line, 'price_minor'), num(line, 'tax_bps'), Boolean(line.tax_inclusive), q.discountMinor + num(line, 'bill_discount_minor'));
                await db.writePosLines2({ quantityMilli: q.quantityMilli, discount: amounts.discount, subtotal: amounts.subtotal, tax: amounts.tax, note: q.note, id: line.id });
                await db.writePosSales9({ user: user, id: sale.id });
            }
            else if (op === 'sale.discount') {
                const q = z.object({ amountMinor: money }).parse(p), lines = await db.readPosLines7({ id: sale.id });
                const weights = lines.map(l => scaledAmount(num(l, 'price_minor'), num(l, 'quantity_milli'), 1000) - num(l, 'discount_minor') + num(l, 'bill_discount_minor'));
                const total = weights.reduce((n, w) => n + w, 0);
                if (q.amountMinor > total)
                    fail('POS_DISCOUNT_AMOUNT', 'Discount exceeds the bill amount', 422);
                let previous = 0;
                for (const [i, l] of lines.entries()) {
                    const extra = weights[i] && q.amountMinor ? portion(q.amountMinor, previous, weights[i], total) : 0;
                    previous += weights[i];
                    const discount = num(l, 'discount_minor') - num(l, 'bill_discount_minor') + extra;
                    const amounts = saleAmounts(num(l, 'quantity_milli'), num(l, 'price_minor'), num(l, 'tax_bps'), Boolean(l.tax_inclusive), discount);
                    await db.writePosLines3({ discount: discount, extra: extra, subtotal: amounts.subtotal, tax: amounts.tax, id: l.id });
                }
                await db.writePosSales9({ user: user, id: sale.id });
            }
            else if (op === 'sale.remove') {
                const q = z.object({ lineId: z.string() }).parse(p), line = await db.readPosLines8({ lineId: q.lineId, id: sale.id });
                if (!line || !['draft', 'cancelled'].includes(str(line, 'fulfillment_state')))
                    fail('POS_ITEM_IN_KITCHEN', 'Cancel the kitchen item before removing it from the bill');
                await db.writePosReservations3({ fulfillment_id: line.fulfillment_id });
                await db.writePosLines4({ id: line.id });
            }
            else if (op === 'sale.hold' || op === 'sale.resume') {
                await db.writePosSales13({ fact0: op === 'sale.hold' ? 'held' : 'open', id: sale.id });
            }
            else if (op === 'sale.move_table') {
                const q = z.object({ tableId: z.string() }).parse(p);
                if (!await db.readPosTables3({ tableId: q.tableId, register_id: sale.register_id }))
                    fail('POS_TABLE_INVALID', 'Table is not at this sales point', 422);
                await db.writePosSales14({ tableId: q.tableId, id: sale.id });
            }
            else if (op === 'sale.merge') {
                const q = z.object({ targetId: z.string(), targetVersion: z.number().int().positive() }).parse(p);
                if (q.targetId === sale.id)
                    fail('POS_MERGE_INVALID', 'Select a different bill', 422);
                const target = await saleRow(db, actor, q.targetId);
                version(target, { expectedVersion: q.targetVersion });
                await editable(db, target);
                if (target.register_id !== sale.register_id || target.shift_id !== sale.shift_id || target.party_id !== sale.party_id)
                    fail('POS_MERGE_INVALID', 'Bills must use the same sales point, cash session, and customer');
                await db.writePosLines5({ id: target.id, id2: sale.id });
                await db.writePosSales15({ id: sale.id });
                await refreshSale(db, str(target, 'id'));
            }
            else if (op === 'sale.split') {
                const q = z.object({ lineId: z.string(), quantityMilli: qty }).parse(p), line = await db.readPosLines9({ lineId: q.lineId, id: sale.id });
                if (!line || q.quantityMilli > num(line, 'quantity_milli'))
                    fail('POS_SPLIT_INVALID', 'Invalid split quantity', 422);
                const target = id(), fid = str(line, 'fulfillment_id'), f = (await db.readPosFulfillments3({ fid: fid }))!;
                let newFid = fid;
                await db.writePosSales16({ target: target, fact1: `POS-${target.slice(0, 8).toUpperCase()}`, user: user, fact3: ports.clock.now(), fact4: ports.clock.now(), id: sale.id });
                if (q.quantityMilli === num(line, 'quantity_milli'))
                    await db.writePosLines6({ target: target, id: line.id });
                else {
                    if (f.kind !== 'recipe' || f.state === 'draft') {
                        if (f.reserved)
                            fail('POS_SPLIT_RESERVED', 'Reserved items cannot be split yet');
                        newFid = id();
                        await db.writePosFulfillments7({ newFid: newFid, quantityMilli: q.quantityMilli, fact2: ports.clock.now(), fid: fid });
                        await db.writePosFulfillments8({ quantityMilli: q.quantityMilli, fid: fid });
                    }
                    const amount = portion(num(line, 'subtotal_minor'), 0, q.quantityMilli, num(line, 'quantity_milli')), tax = portion(num(line, 'tax_minor'), 0, q.quantityMilli, num(line, 'quantity_milli')), discount = portion(num(line, 'discount_minor'), 0, q.quantityMilli, num(line, 'quantity_milli'));
                    const billDiscount = portion(num(line, 'bill_discount_minor'), 0, q.quantityMilli, num(line, 'quantity_milli'));
                    await db.writePosLines7({ fact0: id(), target: target, newFid: newFid, quantityMilli: q.quantityMilli, discount: discount, billDiscount: billDiscount, amount: amount, tax: tax, id: line.id });
                    await db.writePosLines8({ quantityMilli: q.quantityMilli, amount: amount, tax: tax, discount: discount, billDiscount: billDiscount, id: line.id });
                }
                await db.writePosSales17({ issuer_json: sale.issuer_json, user: user, target: target });
                await refreshSale(db, target);
                await refreshSale(db, str(sale, 'id'));
                return { id: target };
            }
            else if (op === 'sale.cancel') {
                for (const f of await db.readPosFulfillments4({ id: sale.id })) {
                    if (!['draft', 'queued', 'cancelled'].includes(str(f, 'state')))
                        fail('POS_KITCHEN_ACTIVE', 'Cancel the kitchen task and record waste before cancelling the bill');
                    if (await db.readPosLines10({ id: f.id, id2: sale.id }))
                        fail('POS_SHARED_KITCHEN', 'This kitchen item is on a separate bill. Handle the item first');
                    await db.writePosReservations4({ id: f.id });
                    await db.writePosFulfillments9({ id: f.id });
                }
                await db.writePosSales18({ id: sale.id });
            }
            else
                fail('POS_ACTION_UNKNOWN', 'Command not found', 404);
            await refreshSale(db, str(sale, 'id'));
            return { id: sale.id, version: (await db.readPosSales4({ id: sale.id }))!.version };
        }
        if (op.startsWith('kitchen.')) {
            const p = base.extend({ reason: z.string().min(3).max(500).optional() }).parse(input), f = scopeRow(actor, await db.readPosFulfillments5({ id: p.id }));
            version(f, p);
            await db.assertFulfillmentHotelDay(company, f.id, str(f, 'business_date'));
            await db.assertPosDayWritable(company, str(f, 'branch_id'), str(f, 'business_date'));
            if (f.kind !== 'recipe')
                fail('POS_KITCHEN_ITEM', 'This item is not a prepared food item', 422);
            let state = '';
            const a = json<PosAccounts>(f.accounts_json);
            if (op === 'kitchen.send' && f.state === 'draft') {
                await reserve(db, f);
                state = 'queued';
            }
            else if (op === 'kitchen.start' && f.state === 'queued') {
                await consume(db, f, user, true);
                state = 'preparing';
            }
            else if (op === 'kitchen.ready' && f.state === 'preparing')
                state = 'ready';
            else if (op === 'kitchen.deliver' && f.state === 'ready') {
                await posting(db, company, str(f, 'branch_id'), a, businessDate(), str(f, 'currency'), `deliver:${f.id}`, str(f, 'id'), user, pair(a.cogs, a.wip, num(f, 'cost_minor')));
                state = 'delivered';
            }
            else if (op === 'kitchen.cancel' && ['draft', 'queued', 'preparing', 'ready', 'delivered'].includes(str(f, 'state'))) {
                ports.require(actor, 'pos.approve');
                if (!p.reason)
                    fail('POS_REASON_REQUIRED', 'Enter the cancellation reason', 422);
                if (['preparing', 'ready', 'delivered'].includes(str(f, 'state'))) {
                    const creator = await db.readPosLines11({ id: f.id, user: user });
                    const starter = await db.readPosAudit2({ id: f.id });
                    if (creator || starter && samePerson(actor, starter.actor_id))
                        fail('POS_INDEPENDENT_REVIEW', 'ผู้อนุมัติอีกคนต้องยกเลิกรายการที่เริ่มทำแล้ว');
                }
                if (await db.readPosLines12({ id: f.id }))
                    fail('POS_REFUND_REQUIRED', 'The bill is paid. Refund it through a refund request first');
                await posting(db, company, str(f, 'branch_id'), a, businessDate(), str(f, 'currency'), `waste:${f.id}`, str(f, 'id'), user, pair(a.waste, f.state === 'delivered' ? a.cogs : a.wip, num(f, 'cost_minor')));
                await db.writePosReservations4({ id: f.id });
                state = 'cancelled';
            }
            else
                fail('POS_KITCHEN_TRANSITION', 'Kitchen status changed');
            await db.writePosFulfillments10({ state: state, id: f.id });
            if (state === 'queued')
                await db.writePosFulfillments1({ id: f.id });
            return { id: f.id, state };
        }
        if (op === 'payment.create') {
            const p = base.extend({ method: z.enum(['cash', 'transfer', 'edc', 'stripe_card', 'stripe_promptpay']), amountMinor: money.refine(n => n > 0), tenderedMinor: money, reference: z.string().max(150).default(''), payerEmail: z.union([z.email(), z.literal('')]).default('') }).parse(input), sale = await saleRow(db, actor, p.id);
            version(sale, p);
            await db.assertHotelDay(company, sale.register_id, str(sale, 'business_date'));
            await db.assertPosDayWritable(company, str(sale, 'branch_id'), str(sale, 'business_date'));
            if (ports.environment("NODE_ENV") === 'production' && !['cash', 'transfer'].includes(p.method) && ports.environment("POS_STRIPE_LIVE_ENABLED") !== 'true')
                fail('POS_TENDER_COMING_SOON', 'The pilot accepts cash and verified bank transfers only', 403);
            if (!['open', 'held', 'payment_pending'].includes(str(sale, 'state')))
                fail('POS_SALE_LOCKED', 'Bill is not ready for payment');
            const settings = await config(db, company);
            if (!settings.accounting_approved_by) fail('POS_ACCOUNTING_REVIEW_REQUIRED', 'Finance must approve the posting accounts before selling.');
            if (!settings.enabled)
                fail('POS_DISABLED', 'New transactions are temporarily paused');
            await openPeriod(db, company, str(sale, 'business_date'));
            if (!await db.readPosShifts4({ shift_id: sale.shift_id }))
                fail('POS_SHIFT_CLOSED', 'Cash session is closed');
            const gross = await db.readPosLines5({ id: sale.id });
            if (num(sale, 'discount_minor') > scaledAmount(num(gross!, 'n'), num(settings, 'cashier_discount_bps'), 10000) && !sale.discount_approved_by)
                fail('POS_DISCOUNT_APPROVAL', 'This discount must be approved before payment');
            const reserved = await db.readPosPayments8({ id: sale.id });
            if (p.amountMinor > num(sale, 'total_minor') - num(reserved!, 'n') || p.tenderedMinor < p.amountMinor || p.method !== 'cash' && p.tenderedMinor !== p.amountMinor)
                fail('POS_PAYMENT_AMOUNT', 'Invalid payment or tendered amount', 422);
            if (['transfer', 'edc'].includes(p.method) && p.reference.trim().length < 2)
                fail('POS_REFERENCE_REQUIRED', 'Enter the payment reference', 422);
            if (p.method === 'stripe_promptpay' && !p.payerEmail)
                fail('POS_PROMPTPAY_EMAIL_REQUIRED', 'กรอกอีเมลผู้จ่ายสำหรับ PromptPay และการคืนเงินก่อนสร้าง QR', 422);
            for (const f of await db.readPosFulfillments4({ id: sale.id })) {
                if (f.state === 'cancelled')
                    fail('POS_ITEM_CANCELLED', 'Remove cancelled items before taking payment');
                await reserve(db, f);
            }
            let connection: PosRow | null = null;
            if (p.method.startsWith('stripe_')) {
                if (sale.currency !== 'THB')
                    fail('POS_STRIPE_CURRENCY', 'This Stripe POS version accepts THB only');
                connection = await db.readPosConnections1({ company: company });
                if (connection?.livemode && ports.environment("POS_STRIPE_LIVE_ENABLED") !== 'true')
                    fail('POS_LIVE_NOT_ENABLED', 'Complete sandbox testing before accepting live payments');
                if (!connection?.charges_enabled || !connection.payouts_enabled || !(p.method === 'stripe_card' ? connection.card_enabled : connection.promptpay_enabled))
                    fail('POS_STRIPE_NOT_READY', 'The Stripe account is not ready for this payment method');
            }
            const pid = id();
            await db.writePosPayments2({ pid: pid, id: sale.id, company: company, method: p.method, amountMinor: p.amountMinor, tenderedMinor: p.tenderedMinor, reference: p.reference, fact7: connection?.account_id ?? null, fact8: connection?.livemode ?? false, user: user, fact10: ports.clock.now(), fact11: ports.clock.now(), fact12: p.payerEmail || null });
            await capturePosMoney(db, sale, 'pos_payment', pid, 'requested', str(sale, 'business_date'), String(p.amountMinor));
            if (connection)
                await enqueue(db, company, 'checkout', pid);
            else if (p.method === 'transfer')
                await db.writePosPayments3({ fact0: ports.clock.now(), pid: pid });
            else
                await receivePayment(db, (await db.readPosPayments9({ pid: pid }))!, user);
            await db.writePosSales2({ fact0: ports.clock.now(), id: sale.id });
            return { id: pid, saleId: sale.id };
        }
        if (op === 'payment.verify_transfer') {
            const p = base.extend({ bankTransactionId: z.string().min(1), bankVersion: z.number().int().positive() }).parse(input);
            const payment = await db.readPosPayments10({ id: p.id, company: company });
            if (!payment)
                fail('POS_NOT_FOUND', 'Payment not found', 404);
            version(payment, p);
            if (payment.method !== 'transfer' || payment.state !== 'pending')
                fail('POS_TRANSFER_STATE', 'Only a pending bank transfer can be verified');
            if (samePerson(actor, str(payment, 'created_by')))
                fail('POS_INDEPENDENT_VERIFICATION', 'A different Finance user must verify the transfer', 403);
            const sale = await saleRow(db, actor, str(payment, 'sale_id'));
            const accounts = json<PosAccounts>(sale.accounts_json);
            const bank = await db.readFinanceBankTransactions1({ bankTransactionId: p.bankTransactionId, company: company });
            if (!bank || num(bank, 'version') !== p.bankVersion || !['unmatched', 'proposed'].includes(str(bank, 'match_status')) || bank.bank_account !== accounts.bank || bank.currency !== sale.currency || num(bank, 'amount_minor') !== num(payment, 'amount_minor') || str(bank, 'external_id').trim() !== str(payment, 'reference').trim() || str(bank, 'transaction_date') < str(sale, 'business_date'))
                fail('POS_BANK_EVIDENCE_MISMATCH', 'Match the exact imported bank account, reference, date, currency, and amount');
            await receivePayment(db, payment, user, str(bank, 'transaction_date'));
            const settled = await db.readPosPayments11({ id: payment.id });
            await db.writeFinanceBankTransactions1({ fact0: settled!.finance_payment_id, id: bank.id });
            return { id: payment.id, saleId: sale.id, state: 'succeeded', bankTransactionId: bank.id };
        }
        if (op === 'payment.cancel_transfer') {
            const p = base.parse(input), payment = await db.readPosPayments10({ id: p.id, company: company });
            if (!payment)
                fail('POS_NOT_FOUND', 'Payment not found', 404);
            version(payment, p);
            await saleRow(db, actor, str(payment, 'sale_id'));
            if (payment.method !== 'transfer' || payment.state !== 'pending')
                fail('POS_TRANSFER_STATE', 'Only a pending bank transfer can be cancelled');
            await db.writePosPayments4({ fact0: ports.clock.now(), id: payment.id });
            await db.writePosSales20({ fact0: ports.clock.now(), sale_id: payment.sale_id, sale_id2: payment.sale_id });
            return { id: payment.id, saleId: payment.sale_id, state: 'cancelled' };
        }
        if (op === 'payment.retry' || op === 'payment.cancel') {
            const p = base.extend({ payerEmail: z.email().optional() }).parse(input), payment = await db.readPosPayments13({ id: p.id });
            if (!payment)
                fail('POS_NOT_FOUND', 'Payment not found', 404);
            await saleRow(db, actor, str(payment, 'sale_id'));
            version(payment, p);
            if (!str(payment, 'method').startsWith('stripe_') || ['succeeded', 'cancelled'].includes(str(payment, 'state')))
                fail('POS_PAYMENT_LOCKED', 'This record cannot change state');
            if (p.payerEmail) {
                const checkout = await db.readPosJobs1({ id: payment.id });
                if (op !== 'payment.retry' || payment.method !== 'stripe_promptpay' || payment.provider_id || payment.payer_email || checkout?.state !== 'exception')
                    fail('POS_PAYMENT_LOCKED', 'ตรวจผลรายการเดิมก่อนแก้ข้อมูลผู้จ่าย');
                await db.writePosPayments5({ payerEmail: p.payerEmail, fact1: ports.clock.now(), id: payment.id });
            }
            if (op === 'payment.cancel')
                await enqueue(db, company, 'cancel_checkout', str(payment, 'id'));
            else
                await db.writePosJobs2({ id: payment.id });
            return { id: payment.id };
        }
        if (op.startsWith('refund.'))
            return refundCommand(db, actor, op, input);
        if (op === 'settlement.refresh') {
            if (actor.branchId)
                fail('POS_COMPANY_SETTINGS', 'Reconcile from the company area', 403);
            const p = z.object({ expectedVersion: z.number().int().positive(), accountingDate: z.iso.date(), reason: z.string().trim().min(3).max(1000) }).parse(input);
            const connection = await db.readPosConnections2({ company: company });
            if (!connection)
                fail('POS_CONNECTION_MISSING', 'Connect the company Stripe account first', 422);
            version(connection, p);
            await openPeriod(db, company, p.accountingDate);
            const syncId = id();
            await enqueue(db, company, 'balance_sync', syncId, { accountId: connection.account_id, livemode: connection.livemode, accountingDate: p.accountingDate, reason: p.reason, userId: user, assignmentId: actor.assignmentId });
            return { id: syncId };
        }
        if (op === 'settlement.map_external') {
            ports.require(actor, 'finance.approve');
            if (actor.branchId)
                fail('POS_COMPANY_SETTINGS', 'Reconcile from the company area', 403);
            const p = base.extend({ journalId: z.string(), reason: z.string().trim().min(3).max(1000) }).parse(input);
            const item = await db.readPosSettlements1({ id: p.id, company: company });
            if (!item)
                fail('POS_NOT_FOUND', 'Unmatched amount not found', 404);
            version(item, p);
            const settings = await config(db, company), a = json<PosAccounts>(settings.accounts_json), details = json<{
                net: number;
                manualPayout?: boolean;
                status?: string;
            }>(item.details_json), net = details.net;
            const journal = await db.readFinanceJournals1({ journalId: p.journalId, company: company, currency: item.currency, stripe: a.stripe });
            if (!journal || num(journal, 'net') !== net || await db.readPosSettlements2({ journalId: p.journalId }))
                fail('POS_EXTERNAL_JOURNAL_MISMATCH', 'Select an approved journal with the matching net amount in the Stripe receivables account, and one that has not already been matched');
            const manualPayout = item.kind === 'payout' && details.manualPayout === true;
            const bankNet = manualPayout ? await db.readFinanceJournalLines1({ journalId: p.journalId, bank: a.bank }) : null;
            if (manualPayout && (details.status !== 'paid' || net !== -num(item, 'amount_minor') || num(bankNet!, 'net') !== num(item, 'amount_minor')))
                fail('POS_EXTERNAL_JOURNAL_MISMATCH', 'The approved manual payout journal must debit the configured bank account for the verified net payout amount');
            await db.writePosSettlements1({ fact0: manualPayout ? 'paid' : 'mapped', journalId: p.journalId, id: item.id });
            return { id: item.id };
        }
        if (op === 'settlement.match') {
            if (actor.branchId)
                fail('POS_COMPANY_SETTINGS', 'Reconcile from the company area', 403);
            ports.require(actor, 'finance.approve');
            const p = base.extend({ bankTransactionId: z.string(), bankVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(1000) }).parse(input);
            const settlement = await db.readPosSettlements3({ id: p.id, company: company });
            if (!settlement || settlement.bank_transaction_id)
                fail('POS_PAYOUT_MATCHED', 'Unreconciled payout not found');
            version(settlement, p);
            if (await db.readFinanceBankMatchTargets1({ journal_id: settlement.journal_id }))
                fail('POS_CASH_JOURNAL_ALREADY_MATCHED', 'Reverse the confirmed Finance bank match before confirming this payout bank evidence');
            const bank = await db.readFinanceBankTransactions1({ bankTransactionId: p.bankTransactionId, company: company });
            if (!bank || bank.version !== p.bankVersion || !['unmatched', 'proposed'].includes(str(bank, 'match_status')) || bank.currency !== settlement.currency || bank.amount_minor !== settlement.amount_minor)
                fail('POS_BANK_MISMATCH', 'Bank amount, currency, or status does not match the payout');
            const bankNet = await db.readFinanceJournalLines2({ journal_id: settlement.journal_id, bank_account: bank.bank_account });
            if (num(bankNet!, 'net') !== num(settlement, 'amount_minor'))
                fail('POS_BANK_ACCOUNT', 'Bank account net amount must match the account receiving the payout');
            await db.writeFinanceBankTransactions2({ id: settlement.id, id2: bank.id });
            await db.writePosSettlements2({ id: bank.id, id2: settlement.id });
            return { id: settlement.id };
        }
        if (op === 'exception.attach_provider') {
            if (actor.branchId)
                fail('POS_COMPANY_SETTINGS', 'Review records from the company area', 403);
            const p = z.object({ id: z.string(), providerId: z.string().regex(/^(re|pyr|cs)_[a-zA-Z0-9_]+$/), accountingDate: z.iso.date(), reason: z.string().trim().min(3).max(1000) }).parse(input);
            const job = await db.readPosJobs2({ id: p.id, company: company });
            if (!job)
                fail('POS_NOT_FOUND', 'Exception record not found', 404);
            ports.require(actor,'finance.approve');
            await openPeriod(db, company, p.accountingDate);
            if (job.kind === 'refund' && (p.providerId.startsWith('re_') || p.providerId.startsWith('pyr_'))) {
                const changed = await db.readPosRefundPayments2({ providerId: p.providerId, record_id: job.record_id });
                if (!changed)
                    fail('POS_PROVIDER_ALREADY_BOUND', 'Record is already linked to Stripe');
            }
            else if (job.kind === 'checkout' && p.providerId.startsWith('cs_')) {
                const changed = await db.readPosPayments14({ providerId: p.providerId, record_id: job.record_id });
                if (!changed)
                    fail('POS_PROVIDER_ALREADY_BOUND', 'Record is already linked to Stripe');
                await enqueue(db, company, 'check_payment', str(job, 'record_id'), { accountingDate: p.accountingDate });
            }
            else
                fail('POS_PROVIDER_TYPE', 'Reference does not match the record type', 422);
            await db.writePosJobs3({ fact0: JSON.stringify({ ...json<Record<string, unknown>>(job.payload_json), accountingDate: p.accountingDate, resolutionReason: p.reason, resolvedBy: user }), id: job.id });
            return { id: job.id };
        }
        if (op === 'exception.retry') {
            if (actor.branchId)
                fail('POS_COMPANY_SETTINGS', 'Edit Stripe records from the company area', 403);
            const p = z.object({ id: z.string(), accountingDate: z.iso.date(), reason: z.string().trim().min(3).max(1000), refundInstructionsEmail: z.email().max(254).optional(), expectedRefundVersion: z.number().int().positive().optional() }).parse(input), job = await db.readPosJobs2({ id: p.id, company: company });
            if (!job)
                fail('POS_NOT_FOUND', 'Exception record not found', 404);
            ports.require(actor,job.kind === 'refund' ? 'pos.approve' : 'finance.approve');
            await openPeriod(db, company, p.accountingDate);
            const payload = json<Record<string, unknown>>(job.payload_json);
            if (p.refundInstructionsEmail) {
                ports.require(actor, 'finance.approve');
                if (actor.departmentId)
                    fail('POS_COMPANY_SETTINGS', 'Correct refund instructions from the company area', 403);
                const refund = await db.readPosRefundPayments3({ record_id: job.record_id, company: company, company2: company });
                if (job.kind !== 'refund' || job.last_error !== 'POS_PROVIDER_REFUND_EMAIL_RESTRICTED' || !refund || refund.method !== 'stripe_promptpay' || refund.provider_id || refund.payout_state !== 'requested' || refund.refund_state !== 'processing')
                    fail('POS_REFUND_EMAIL_LOCKED', 'Only an approved PromptPay refund with a rejected instruction email and no provider refund can be corrected');
                if (p.expectedRefundVersion !== num(refund, 'version'))
                    fail('POS_VERSION_CONFLICT', 'Reload the refund before correcting its instruction email', 409);
                const attempts = z.array(z.object({ email: z.email().max(254) })).max(10).parse(payload.refundInstructionsAttempts ?? []);
                if (attempts.length >= 10)
                    fail('POS_PROVIDER_LOOKUP_REQUIRED', 'Review the refund with Stripe before another email correction');
                if (p.refundInstructionsEmail === (attempts.at(-1)?.email ?? refund.payer_email))
                    fail('POS_REFUND_EMAIL_UNCHANGED', 'The instruction email is unchanged. Retry without a replacement after Stripe permits that recipient');
                payload.refundInstructionsAttempts = [...attempts, { email: p.refundInstructionsEmail }];
                await db.writePosRefunds1({ id: refund.id });
            }
            let providerAttemptStartedAt = typeof payload.providerAttemptStartedAt === 'number' ? payload.providerAttemptStartedAt : undefined;
            if (job.kind === 'refund') {
                const failed = await db.readPosRefundPayments4({ record_id: job.record_id });
                if (failed)
                    providerAttemptStartedAt = ports.clock.now();
            }
            await db.writePosJobs4({ fact0: JSON.stringify({ ...payload, accountingDate: p.accountingDate, providerAttemptStartedAt, resolutionReason: p.reason, resolvedBy: user }), id: job.id });
            return { id: job.id };
        }
        fail('POS_ACTION_UNKNOWN', 'POS command not found', 404);
    }
    async function refundCommand(db: PosUnit<Effect>, actor: WorkContext, op: string, input: unknown): Promise<Record<string, unknown>> {
        const user = actor.userId;
        if (op === 'refund.request') {
            const p = base.extend({ reason: z.string().trim().min(3).max(1000), lines: z.array(z.object({ lineId: z.string(), quantityMilli: qty, restock: z.boolean() })).max(100).default([]), amountMinor: money.optional(), payoutShiftId: z.string().optional() }).parse(input), sale = await saleRow(db, actor, p.id);
            version(sale, p);
            await db.assertPosDayWritable(str(sale, 'company_id'), str(sale, 'branch_id'), businessDate());
            if (await db.readPosDayCloses1({ company_id: sale.company_id, branch_id: sale.branch_id, business_date: sale.business_date }))
                fail('POS_DAY_REVIEW_PENDING', 'Finance must confirm the original day close before a refund can change its source records');
            if (await db.readPmsSources2({ fact0: `pos:${sale.id}:${sale.id}` }))
                fail('POS_ROOM_CHARGE_REVERSAL', 'Reverse this charge on the hotel guest account; it has no POS payment to refund');
            if (!['completed', 'payment_pending'].includes(str(sale, 'state')))
                fail('POS_REFUND_SALE', 'Bill has no payment');
            if (new Set(p.lines.map(l => l.lineId)).size !== p.lines.length)
                fail('POS_DUPLICATE_LINE', 'Duplicate refund item', 422);
            const advance = sale.state !== 'completed', snapshots: Array<Record<string, unknown>> = [];
            let amount = 0, tax = 0;
            if (advance) {
                if (!p.amountMinor || p.lines.length)
                    fail('POS_ADVANCE_REFUND', 'Enter an advance payment refund amount without selecting items', 422);
                amount = p.amountMinor;
            }
            else
                for (const part of p.lines) {
                    const l = await db.readPosLines14({ lineId: part.lineId, id: sale.id });
                    if (!l || num(l, 'returned_milli') + part.quantityMilli > num(l, 'quantity_milli'))
                        fail('POS_REFUND_QUANTITY', 'Refund quantity exceeds the original item', 422);
                    const subtotal = portion(num(l, 'subtotal_minor'), num(l, 'returned_milli'), part.quantityMilli, num(l, 'quantity_milli')), vat = portion(num(l, 'tax_minor'), num(l, 'returned_milli'), part.quantityMilli, num(l, 'quantity_milli'));
                    amount += subtotal + vat;
                    tax += vat;
                    snapshots.push({ ...part, subtotalMinor: subtotal, taxMinor: vat, previousReturned: num(l, 'returned_milli') });
                }
            if (amount <= 0)
                fail('POS_REFUND_EMPTY', 'Select an item with a refund amount', 422);
            const rid = id();
            await db.writePosRefunds2({ rid: rid, id: sale.id, company_id: sale.company_id, branch_id: sale.branch_id, fact4: p.payoutShiftId ?? null, fact5: advance ? 'advance' : 'sale', reason: p.reason, fact7: JSON.stringify(snapshots), amount: amount, user: user, fact10: ports.clock.now(), fact11: businessDate(), tax: tax });
            return { id: rid };
        }
        const p = base.extend({ reason: z.string().trim().min(3).max(1000).optional() }).parse(input), refund = scopeRow(actor, await db.readPosRefunds1({ id: p.id }));
        version(refund, p);
        if (refund.state !== 'requested' || samePerson(actor, refund.requested_by))
            fail('POS_INDEPENDENT_REVIEW', 'A different manager must approve the pending request');
        if (op === 'refund.reject') {
            if (!p.reason)
                fail('POS_REASON_REQUIRED', 'Enter a reason', 422);
            await db.writePosRefunds3({ user: user, id: refund.id });
            return { id: refund.id };
        }
        if (op !== 'refund.approve')
            fail('POS_ACTION_UNKNOWN', 'Refund command not found', 404);
        const sale = await saleRow(db, actor, str(refund, 'sale_id'));
        await db.assertHotelDay(actor.companyId,sale.register_id,str(refund,'accounting_date'));
        const a = json<PosAccounts>(sale.accounts_json), date = str(refund, 'accounting_date');
        const payments = await db.readPosPayments15({ id: sale.id });
        if (payments.reduce((n, r) => n + num(r, 'amount_minor') - num(r, 'refunded_minor'), 0) < num(refund, 'amount_minor'))
            fail('POS_REFUND_EXCEEDED', 'Refund exceeds the remaining payment amount');
        if (refund.kind === 'advance' && sale.state === 'completed')
            fail('POS_REFUND_CHANGED', 'Bill is closed. Request a refund from the item');
        if (refund.kind === 'sale' && sale.state !== 'completed')
            fail('POS_REFUND_CHANGED', 'Bill status changed');
        for (const part of json<Array<{
            lineId: string;
            quantityMilli: number;
            restock: boolean;
            subtotalMinor: number;
            taxMinor: number;
            previousReturned: number;
        }>>(refund.lines_json)) {
            const line = (await db.readPosLines15({ lineId: part.lineId, id: sale.id }))!;
            if (!line || num(line, 'returned_milli') !== part.previousReturned || part.quantityMilli + num(line, 'returned_milli') > num(line, 'quantity_milli'))
                fail('POS_REFUND_CHANGED', 'This item has already been refunded. Create a new request');
            const f = (await db.readPosFulfillments7({ fulfillment_id: line.fulfillment_id }))!;
            let restoredCost = 0;
            if (part.restock) {
                if (f.kind !== 'stock' || f.state !== 'delivered')
                    fail('POS_RETURN_NOT_STOCK', 'Only delivered items can be returned to inventory', 422);
                const returned = await db.readPosStockMoves1({ id: f.id });
                restoredCost = portion(num(f, 'cost_minor'), num(returned!, 'qty'), part.quantityMilli, num(f, 'quantity_milli'));
                await db.writeCommerceStock3({ warehouse_id: f.warehouse_id, product_id: f.product_id });
                await db.writeCommerceStock4({ quantityMilli: part.quantityMilli, restoredCost: restoredCost, warehouse_id: f.warehouse_id, product_id: f.product_id });
                await db.writePosStockMoves3({ fact0: id(), company_id: sale.company_id, warehouse_id: f.warehouse_id, product_id: f.product_id, id: f.id, id2: refund.id, number: sale.number, fact7: 'return', quantityMilli: part.quantityMilli, restoredCost: restoredCost, date: date, fact11: ports.clock.now() });
                await posting(db, str(sale, 'company_id'), str(sale, 'branch_id'), a, date, str(sale, 'currency'), `return:${refund.id}:${line.id}`, str(refund, 'id'), user, pair(a.inventory, a.cogs, restoredCost));
            }
            await db.writePosLines9({ quantityMilli: part.quantityMilli, fact1: part.subtotalMinor + part.taxMinor, taxMinor: part.taxMinor, restoredCost: restoredCost, id: line.id });
        }
        await posting(db, str(sale, 'company_id'), str(sale, 'branch_id'), a, date, str(sale, 'currency'), `refund:${refund.id}`, str(refund, 'id'), user, [{ account: refund.kind === 'advance' ? a.advance : a.revenue, debit: num(refund, 'amount_minor') - num(refund, 'tax_minor'), credit: 0 }, { account: a.outputTax, debit: num(refund, 'tax_minor'), credit: 0 }, { account: a.refund, debit: 0, credit: num(refund, 'amount_minor') }]);
        if (refund.kind === 'sale')
            await capturePosReversal(db, sale, 'pos_refund', str(refund, 'id'), 'total', date, String(refund.amount_minor), { sourceType: 'pos_sale', sourceId: str(sale, 'id'), amountKind: 'total' });
        let remaining = num(refund, 'amount_minor');
        for (const payment of payments) {
            const amount = Math.min(remaining, num(payment, 'amount_minor') - num(payment, 'refunded_minor'));
            if (!amount)
                continue;
            const manualPromptPay = payment.method === 'stripe_promptpay' && !payment.payer_email;
            const rpid = id();
            await db.writePosRefundPayments1({ rpid: rpid, id: refund.id, id2: payment.id, amount: amount, fact4: manualPromptPay ? (refund.payout_shift_id ? 'cash' : 'transfer') : null });
            await db.writePosPayments6({ amount: amount, id: payment.id });
            if (str(payment, 'method').startsWith('stripe_') && !manualPromptPay)
                await enqueue(db, str(sale, 'company_id'), 'refund', rpid);
            else {
                if (payment.method === 'cash' || (manualPromptPay && refund.payout_shift_id)) {
                    const shift = await db.readPosShifts6({ payout_shift_id: refund.payout_shift_id, companyId: actor.companyId, branch_id: sale.branch_id });
                    if (!shift)
                        fail('POS_REFUND_SHIFT_REQUIRED', 'Specify an open cash session for cash refund payout');
                }
                // Manual methods require the approver to confirm money was actually returned.
                if (!p.reason)
                    fail('POS_REFUND_PROOF_REQUIRED', 'Enter proof/reference for the cash, transfer, or EDC refund', 422);
                await completeRefundPayment(db, rpid, user);
            }
            remaining -= amount;
        }
        await db.writePosRefunds4({ user: user, id: refund.id, id2: refund.id });
        await releaseUnpaidReservations(db, str(sale, 'id'));
        return { id: refund.id };
    }
    async function completeRefundPayment(db: PosUnit<Effect>, refundPaymentId: string, actorId: string, date?: string) {
        const rp = await db.readPosRefundPayments5({ refundPaymentId: refundPaymentId });
        if (!rp || rp.state === 'succeeded')
            return;
        const refund = (await db.readPosRefunds2({ refund_id: rp.refund_id }))!, p = (await db.readPosPayments16({ payment_id: rp.payment_id }))!, sale = (await db.readPosSales5({ sale_id: refund.sale_id }))!, a = json<PosAccounts>(sale.accounts_json);
        const account = rp.manual_method === 'cash' ? a.cash : rp.manual_method === 'transfer' ? a.bank : p.method === 'cash' ? a.cash : p.method === 'transfer' ? a.bank : p.method === 'edc' ? a.edc : a.stripe;
        const journal = await posting(db, str(sale, 'company_id'), str(sale, 'branch_id'), a, date ?? str(refund, 'accounting_date'), str(sale, 'currency'), `refund-payment:${rp.id}`, str(refund, 'id'), actorId, pair(a.refund, account, num(rp, 'amount_minor')));
        await db.writeFinancePayments3({ fact0: id(), company_id: sale.company_id, party_id: sale.party_id, fact3: `POS-REF-${rp.id}`, fact4: date ?? refund.accounting_date, currency: sale.currency, amount_minor: rp.amount_minor, account: account, journal: journal, actorId: actorId, actorId2: actorId, fact11: ports.clock.now(), fact12: ports.clock.now() });
        await capturePosReversal(db, sale, 'pos_refund_payment', str(rp, 'id'), 'settled', date ?? str(refund, 'accounting_date'), String(rp.amount_minor), { sourceType: 'pos_payment', sourceId: str(p, 'id'), amountKind: 'received' });
        await db.writePosRefundPayments2({ id: rp.id });
        await db.writePosRefunds5({ id: refund.id, id2: refund.id });
    }
    async function releaseUnpaidReservations(db: PosUnit<Effect>, saleId: string) {
        if (await db.readPosPayments17({ saleId: saleId }))
            return;
        if (await db.readPosRefundPayments6({ saleId: saleId }))
            return;
        await db.writePosReservations6({ saleId: saleId });
        await db.writePosFulfillments12({ saleId: saleId });
        await db.writePosSales21({ saleId: saleId });
    }
    const posPermissions = (actor: WorkContext) => Object.fromEntries(['pos.read', 'pos.sell', 'pos.kitchen', 'pos.approve', 'pos.configure', 'pos.accounting.prepare', 'pos.accounting.approve', 'pos.payments.configure', 'pos.reports', 'finance.read', 'finance.write', 'finance.approve', 'pms.approve'].map(p => [p, hasPermission(actor, p)]));
    return { json, fail, scopeRow, saleRow, config, openPeriod, posting, audit, enqueue, reserve, receivePayment, completeSale, posCommand, completeRefundPayment, releaseUnpaidReservations, id, num, str, pair, posPermissions };
}
