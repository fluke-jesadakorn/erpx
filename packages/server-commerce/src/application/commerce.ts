export type CommerceReturnDecision = { id:string; orderId:string; status:string; version:number; stockJournalId?:string; creditJournalId?:string; goodsFxJournalId?:string };
export type CommerceReturnCompletion = { id:string; orderId:string; status:string; version:number; creditedMinor:number; refundMinor:number; refundId:string|null };
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { AttachmentManifestItem } from "@erpx/server-files/public/attachments/types";
import type { FunctionalFxEvidence } from "@erpx/server-finance/public/commerce-journal-contracts";
import type {EmployeeRequest} from '@erpx/server-records/public/employee-requests';
import type { SalesPricingRow } from "@erpx/server-sales/public/commerce-pricing-contracts";
import type { BillRow } from "@erpx/server-procurement/public/commerce-bill-contracts";
import type { ListPage } from "@erpx/server-work_projection/public/list-page";
import { minorToDecimal, decimalToMinor } from "@erpx/feature-finance/lib/finance-money";
import { quantityMilli, lineAmounts, scaledAmount } from "@erpx/feature-commerce/lib/commerce-math";
import { convertUnits } from "@erpx/feature-finance/lib/company-money";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import {orderSchema,transitionSchema,fulfillmentSchema,invoiceFromDeliverySchema,settlementSchema,returnRequestSchema,returnDecisionSchema,returnCompleteSchema,productSchema,partyMasterSchema,warehouseSchema,stockReservationSchema,roles,commercePermissions} from "@erpx/server-commerce/domain/commerce";
export type Row = Record<string, string | number | null>;
export type CommerceRows<Row> = {
    results: Row[];
};
export type CommerceAttachmentBinding<Effect> = {
    manifest: AttachmentManifestItem[];
    guards: Effect[];
    steps: Effect[];
};
export type CommerceJournal<Effect> = {
    steps: Effect[];
    guards: Effect[];
    preparedLines: Array<{
        debit: number;
        credit: number;
        baseDebit: number;
        baseCredit: number;
        accountId: string;
        description?: string;
        dimensions?: Record<string, string>;
    }>;
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
};
export type CommerceUnit<Effect> = {
    readCommerceSettings1(facts: {
        companyId: string;
    }): Promise<Row | null>;
    readFinancePeriods1(facts: {
        companyId: string;
        date: string;
        date2: string;
        fact3: string | null;
        fact4: string | null;
    }): Promise<CommerceRows<Row>>;
    readCommerceOrders1(facts: {
        id: string;
        fact1: Array<string | null>;
    }): Promise<Row | null>;
    readCommerceOrders2(facts: {
        fact0: Array<string | null>;
    }): Promise<CommerceRows<Row>>;
    readCommerceProducts1(facts: {
        companyId: string;
    }): Promise<CommerceRows<Row>>;
    readFinanceParties1(facts: {
        companyId: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceWarehouses1(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
    }): Promise<CommerceRows<Row>>;
    readInventoryActiveReservations1(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
    }): Promise<CommerceRows<Row>>;
    readInventoryLedger1(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
    }): Promise<CommerceRows<Row>>;
    readOrganizationNodes1(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
    }): Promise<CommerceRows<Row>>;
    readSalesPriceLists1(facts: {
        companyId: string;
    }): Promise<CommerceRows<Row>>;
    readSalesPriceListItems1(facts: {
        companyId: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceOrders3(facts: {
        fact0: Array<string | null>;
    }): Promise<CommerceRows<Row>>;
    readInventoryActiveReservations2(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceFulfillments1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceReturnLines1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceReturns1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readHrPeople1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceOrders4(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readInventoryOrderReservations1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readInventoryQualityHolds1(facts: {
        id: string;
        warehouse_id: string | number | null;
    }): Promise<CommerceRows<Row>>;
    readCommerceReturnLotAllocations1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceReturnLotAllocations2(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readInventoryQualityHolds2(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceFulfillmentEvidence1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readAttachments1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceCustomerNotifications1(facts: {
        id: string;
    }): Promise<CommerceRows<Row>>;
    readCommerceCustomerNotificationSettings1(facts: {
        companyId: string;
    }): Promise<Row | null>;
    readCommerceOrderLines1(facts: {
        lineId: string;
        id: string;
    }): Promise<Row | null>;
    prepareInventoryOrderReservations1(facts: {
        fact0: string;
        id: string | number | null;
        warehouse_id: string | number | null;
        product_id: string | number | null;
        qty: number;
        expiresAt: number;
        userId: string;
        reason: string;
        now: number;
    }): Effect;
    readInventoryOrderReservations2(facts: {
        id: string | number | null;
    }): Promise<CommerceRows<Row>>;
    prepareInventoryOrderReservations2(facts: {
        n: number;
        reason: string;
        id: string | number | null;
    }): Effect;
    prepareCommerceOrders1(facts: {
        now: number;
        id: string;
    }): Effect;
    readCommerceFulfillments2(facts: {
        fulfillmentId: string;
    }): Promise<Row | null>;
    readCommerceFulfillmentLines1(facts: {
        lineId: string;
        fulfillmentId: string;
    }): Promise<Row | null>;
    readProcurementBillMatchLines1(facts: {
        invoice_id: string | number | null;
        id: string | number | null;
    }): Promise<Row | null>;
    readCommerceReturnLines2(facts: {
        id: string | number | null;
    }): Promise<{
        quantity: number;
        value: number;
        subtotal: number;
        tax: number;
        expected_subtotal: number;
    } | null>;
    readInventoryFulfillmentLotAllocations1(facts: {
        id: string | number | null;
        lotId: string;
    }): Promise<Row | null>;
    readCommerceReturnLotAllocations3(facts: {
        id: string | number | null;
    }): Promise<{
        quantity: number;
        value: number;
    } | null>;
    readInventoryLotBalances1(facts: {
        lotId: string;
        warehouse_id: string | number | null;
    }): Promise<Row | null>;
    prepareCommerceReturns1(facts: {
        id: string;
        companyId: string;
        branch_id: string | number | null;
        order_id: string | number | null;
        fulfillmentId: string;
        invoice_id: string | number | null;
        kind: "supplier_return" | "customer_return";
        number: string;
        date: string;
        reason: string;
        subtotal: number;
        tax: number;
        total: number;
        userId: string;
        now: number;
        now2: number;
    }): Effect;
    prepareCommerceReturnLines1(facts: {
        returnLineId: string;
        id: string;
        id2: string | number | null;
        product_id: string | number | null;
        quantity: number;
        returnValue: number;
        returnSubtotal: number;
        returnTax: number;
        index: number;
        condition: string | number | null;
        fact10: string | number | null;
        fact11: string | number | null;
        expectedSubtotal: string | number | null;
    }): Effect;
    prepareCommerceReturnLotAllocations1(facts: {
        id: string;
        returnLineId: string;
        fulfillmentAllocationId: string;
        lotId: string;
        quantity: number;
        value: number;
        snapshotQuantity: number;
        snapshotValue: number;
        snapshotVersion: number;
    }): Effect;
    prepareCommerceFulfillments1(facts: {
        fulfillmentId: string;
        expectedVersion: number;
    }): Effect;
    readCommerceReturns2(facts: {
        returnId: string;
        companyId: string;
    }): Promise<Row | null>;
    prepareCommerceReturns2(facts: {
        userId: string;
        reason: string;
        fact2: number;
        returnId: string;
    }): Effect;
    readCommerceFulfillments3(facts: {
        fulfillment_id: string | number | null;
        companyId: string;
    }): Promise<Row | null>;
    readFinanceJournalLines1(facts: {
        id: string | number | null;
    }): Promise<CommerceRows<Row>>;
    readCommerceReturnLines3(facts: {
        returnId: string;
    }): Promise<CommerceRows<Row>>;
    readProcurementBillMatches1(facts: {
        invoice_id: string | number | null;
        companyId: string;
    }): Promise<Row | null>;
    prepareCommerceReturns3(facts: {
        returnId: string;
    }): Effect;
    prepareFinanceInvoices1(facts: {
        invoice_id: string | number | null;
    }): Effect;
    readCommerceStock1(facts: {
        warehouse_id: string | number | null;
        product_id: string | number | null;
    }): Promise<Row | null>;
    prepareCommerceStock1(facts: {
        fact0: string | number | null;
        fact1: string | number | null;
        warehouse_id: string | number | null;
        product_id: string | number | null;
    }): Effect;
    prepareCommerceReturnStockMoves1(facts: {
        fact0: string;
        returnId: string;
        warehouse_id: string | number | null;
        product_id: string | number | null;
        fact4: string | number | null;
        fact5: string | number | null;
        return_date: string | number | null;
        now: number;
    }): Effect;
    readCommerceReturnLotAllocations4(facts: {
        id: string | number | null;
    }): Promise<CommerceRows<Row>>;
    prepareInventoryLotBalances1(facts: {
        lot_id: string | number | null;
        warehouse_id: string | number | null;
        quantity_milli: string | number | null;
        value_minor: string | number | null;
    }): Effect;
    prepareInventoryLots1(facts: {
        lot_id: string | number | null;
    }): Effect;
    prepareInventoryLotBalances2(facts: {
        quantity_milli: string | number | null;
        value_minor: string | number | null;
        lot_id: string | number | null;
        warehouse_id: string | number | null;
    }): Effect;
    prepareInventoryLots2(facts: {
        lot_id: string | number | null;
        lot_id2: string | number | null;
    }): Effect;
    prepareInventoryLotMoves1(facts: {
        fact0: string;
        lot_id: string | number | null;
        warehouse_id: string | number | null;
        kind: string | number | null;
        fact4: string;
        fact5: string | number | null;
        fact6: string | number | null;
        return_date: string | number | null;
        userId: string;
        now: number;
    }): Effect;
    prepareCommerceReturns4(facts: {
        userId: string;
        stockJournal: string | null;
        creditJournal: string;
        baseCreditMinor: number;
        fact4: string | null;
        now: number;
        returnId: string;
    }): Effect;
    readCommerceReturns3(facts: {
        returnId: string;
        companyId: string;
    }): Promise<Row | null>;
    readFinanceJournalLines2(facts: {
        incoming: boolean;
        credit_journal_id: string | number | null;
        control: string;
    }): Promise<number | null>;
    readFinanceCharts1(facts: {
        chart_id: string | number | null;
        companyId: string;
    }): Promise<{
        base_currency: string;
    } | null>;
    readFinanceAccounts1(facts: {
        account: string;
        companyId: string;
        chart_id: string | number | null;
        base_currency: string;
        fact4: "other_income" | "other_expense";
    }): Promise<Record<string, unknown> | null>;
    prepareFinanceInvoices2(facts: {
        invoice_id: string | number | null;
    }): Effect;
    prepareFinanceInvoices3(facts: {
        credited: number;
        baseCreditedMinor: number;
        credited2: number;
        now: number;
        invoice_id: string | number | null;
    }): Effect;
    prepareCommerceReturns5(facts: {
        credited: number;
        refund: number;
        carried: number;
        baseCreditedMinor: number;
        baseRefundMinor: number;
        fact5: string | null;
        now: number;
        returnId: string;
    }): Effect;
    prepareFinanceRefundObligations1(facts: {
        refundId: string;
        companyId: string;
        branch_id: string | number | null;
        department_id: string | number | null;
        returnId: string;
        invoice_id: string | number | null;
        party_id: string | number | null;
        fact7: "outgoing" | "incoming";
        currency: string | number | null;
        chart_id: string | number | null;
        fact10: string | number | null;
        refund: number;
        refund2: number;
        baseRefundMinor: number;
        baseRefundMinor2: number;
        credit_journal_id: string | number | null;
        userId: string;
        now: number;
        now2: number;
    }): Effect;
    readEmployeeRequests1(facts: {
        companyId: string;
        id: string;
        id2: string;
    }): Promise<{
        id: string;
    } | null>;
    readCommerceWarehouses2(facts: {
        warehouseId: string;
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
    }): Promise<Row | null>;
    readCommerceProducts2(facts: {
        productId: string;
        companyId: string;
    }): Promise<Row | null>;
    prepareCommerceOrders2(facts: {
        partyId: string;
        warehouseId: string | null;
        branch_id: string | number | null;
        orderDate: string;
        dueDate: string;
        note: string;
        subtotal: number;
        tax: number;
        fact8: string | number | null;
        fact9: string | number | null;
        fact10: string | null;
        fact11: 0 | 1;
        now: number;
        record: string;
    }): Effect;
    prepareCommerceOrderLines1(facts: {
        record: string;
    }): Effect;
    prepareCommerceOrders3(facts: {
        record: string;
        companyId: string;
        branch_id: string | number | null;
        departmentId: string | null;
        kind: "sales" | "purchase" | "quote";
        fact5: string;
        partyId: string;
        warehouseId: string | null;
        currency: string | number | null;
        orderDate: string;
        dueDate: string;
        note: string;
        subtotal: number;
        tax: number;
        fact14: string | number | null;
        fact15: string | number | null;
        fact16: string | null;
        fact17: 0 | 1;
        userId: string;
        now: number;
        now2: number;
    }): Effect;
    prepareCommerceOrderLines2(facts: {
        id: string;
        record: string;
        productId: string;
        name: string | number | null;
        qty: number;
        price: number;
        taxBps: number;
        subtotal: number;
        tax: number;
        index: number;
        listPrice: number;
        discountBps: number;
        itemType: "stock" | "service";
    }): Effect;
    readCommerceFulfillments4(facts: {
        id: string;
    }): Promise<Record<string, unknown> | null>;
    prepareInventoryOrderReservations3(facts: {
        id: string;
    }): Effect;
    prepareCommerceOrders4(facts: {
        state: "draft" | "confirmed" | "cancelled" | "submitted" | "converted";
        fact1: string | number | null;
        fact2: boolean;
        userId: string;
        fact4: boolean;
        fact5: boolean;
        fact6: number;
        fact7: boolean;
        fact8: number;
        id: string;
    }): Effect;
    prepareFinanceParties1(facts: {
        party_id: string | number | null;
        companyId: string;
    }): Effect;
    prepareCommerceOrders5(facts: {
        salesId: string;
        fact1: string;
        userId: string;
        fact3: number;
        fact4: number;
        id: string;
    }): Effect;
    prepareCommerceOrderLines3(facts: {
        salesId: string;
        salesId2: string;
        id: string;
    }): Effect;
    prepareFinanceJournals1(facts: {
        journalId: string;
        companyId: string;
        chart_id: string | number | null;
        periodId: string;
        fact4: string;
        date: string;
        currency: string | number | null;
        source: string;
        sourceId: string;
        fact9: string;
        status: "draft" | "posted";
        userId: string;
        fact12: string | null;
        fact13: number | null;
        now: number;
        now2: number;
    }): Effect;
    prepareFinanceJournalLines1(facts: {
        fact0: string;
        journalId: string;
        account: string;
        debit: number;
        credit: number;
        debit2: number;
        credit2: number;
        i: number;
    }): Effect;
    readAttachments2(facts: {
        attachmentId: string;
        id: string;
    }): Promise<Record<string, unknown> | null>;
    prepareCommerceFulfillments2(facts: {
        fulfillment: string;
        id: string;
        fact2: string;
        date: string;
        userId: string;
        fact5: number;
        fact6: string | null;
        completionMode: "goods" | "service" | "mixed";
    }): Effect;
    readCommerceOrderLines2(facts: {
        lineId: string;
        id: string;
    }): Promise<Row | null>;
    readInventoryActiveReservations3(facts: {
        warehouse_id: string | number | null;
        product_id: string | number | null;
    }): Promise<Row | null>;
    readInventoryOrderReservations3(facts: {
        id: string | number | null;
    }): Promise<CommerceRows<Row>>;
    readCommerceFulfillmentLines2(facts: {
        id: string | number | null;
    }): Promise<{
        subtotal: number;
        tax: number;
    } | null>;
    readInventoryLots1(facts: {
        warehouse_id: string | number | null;
        companyId: string;
        product_id: string | number | null;
        identity: string;
        tracking: string;
    }): Promise<Row | null>;
    readInventoryQualityHolds3(facts: {
        fact0: string;
        companyId: string;
        product_id: string | number | null;
        warehouse_id: string | number | null;
    }): Promise<Row | null>;
    prepareCommerceStock2(facts: {
        warehouse_id: string | number | null;
        product_id: string | number | null;
    }): Effect;
    prepareInventoryOrderReservations4(facts: {
        n: number;
        id: string | number | null;
    }): Effect;
    prepareCommerceStock3(facts: {
        fact0: number;
        fact1: number;
        warehouse_id: string | number | null;
        product_id: string | number | null;
    }): Effect;
    prepareCommerceFulfillmentLines1(facts: {
        fact0: string;
        fulfillment: string;
        id: string | number | null;
        qty: number;
        cost: number;
        subtotal: number;
        tax: number;
        stockCurrency: string;
    }): Effect;
    prepareInventoryLots3(facts: {
        lotId: string;
        companyId: string;
        product_id: string | number | null;
        tracking: string;
        lotCode: string | null;
        serialNumber: string | null;
        manufacturedDate: string | null;
        expiryDate: string | null;
        userId: string;
        fact9: number;
    }): Effect;
    prepareInventoryLots4(facts: {
        lotId: string;
    }): Effect;
    prepareInventoryLotBalances3(facts: {
        lotId: string;
        warehouse_id: string | number | null;
        quantityMilli: number;
        valueMinor: number;
    }): Effect;
    prepareInventoryLotBalances4(facts: {
        quantityMilli: number;
        valueMinor: number;
        lotId: string;
        warehouse_id: string | number | null;
    }): Effect;
    prepareInventoryLots5(facts: {
        lotId: string;
        lotId2: string;
    }): Effect;
    prepareInventoryLotMoves2(facts: {
        fact0: string;
        lotId: string;
        warehouse_id: string | number | null;
        fact3: "goods_receipt" | "delivery";
        lineId: string;
        fact5: number;
        fact6: number;
        date: string;
        userId: string;
        fact9: number;
    }): Effect;
    prepareInventoryFulfillmentLotAllocations1(facts: {
        fact0: string;
        lineId: string;
        lotId: string;
        quantityMilli: number;
        valueMinor: number;
        disposition: "available" | "quarantine";
        qualityHoldId: string | null;
    }): Effect;
    prepareCommerceOrderLines4(facts: {
        qty: number;
        id: string | number | null;
    }): Effect;
    prepareCommerceStockMoves1(facts: {
        fact0: string;
        warehouse_id: string | number | null;
        product_id: string | number | null;
        fulfillment: string;
        fact4: number;
        fact5: number;
        fact6: number;
    }): Effect;
    prepareCommerceGoodsValuations1(facts: {
        fulfillment: string;
        companyId: string;
        chart_id: string | number | null;
        journal: string;
        fact4: string;
        fact5: string;
        stockCurrency: string;
        date: string;
        micros: number;
        sourceRateId: string | null;
        targetRateId: string | null;
        usedLatestFallback: boolean;
        documentSubtotal: number;
        value: number;
        userId: string;
        fact15: number;
    }): Effect;
    prepareCommerceFulfillmentEvidence1(facts: {
        fulfillment: string;
        attachmentId: string;
        userId: string;
        fact3: number;
    }): Effect;
    prepareCommerceFulfillments3(facts: {
        journal: string;
        fulfillment: string;
    }): Effect;
    prepareCommerceOrders6(facts: {
        id: string;
        fact1: number;
        id2: string;
    }): Effect;
    readCommerceFulfillments5(facts: {
        fulfillmentId: string;
    }): Promise<Row | null>;
    readFinanceJournals1(facts: {
        journal_id: string | number | null;
        companyId: string;
    }): Promise<Record<string, unknown> | null>;
    readCommerceFulfillmentLines3(facts: {
        fulfillmentId: string;
    }): Promise<{
        subtotal: number;
        tax: number;
    } | null>;
    prepareFinanceInvoices4(facts: {
        invoice: string;
        companyId: string;
        party_id: string | number | null;
        fact3: "receivable" | "payable";
        fact4: string;
        date: string;
        dueDate: string;
        invoiceCurrency: string;
        subtotal: number;
        tax: number;
        total: number;
        total2: number;
        fact12: string | number | null;
        fact13: string | number | null;
        fact14: string | number | null;
        fulfillmentId: string;
        userId: string;
        fact17: number;
        fact18: number;
    }): Effect;
    prepareCommerceFulfillments4(facts: {
        invoice: string;
        fulfillmentId: string;
    }): Effect;
    readFinanceInvoices1(facts: {
        invoiceId: string;
        companyId: string;
    }): Promise<Row | null>;
    readCommerceFulfillments6(facts: {
        source_id: string | number | null;
    }): Promise<{
        order_id: string;
    } | null>;
    readFinanceExpenses1(facts: {
        source_id: string | number | null;
        invoiceId: string;
        companyId: string;
    }): Promise<Row | null>;
    readFinanceSupplierInstructions1(facts: {
        invoiceId: string;
    }): Promise<Record<string, unknown> | null>;
    readAttachmentSets1(facts: {
        companyId: string;
        invoiceId: string;
        expectedVersion: number;
        paymentSlipId: string;
    }): Promise<{
        id: string;
    } | null>;
    readFinanceAccounts2(facts: {
        bankAccountId: string;
        companyId: string;
        chart_id: string | number | null;
        currency: string | number | null;
    }): Promise<Record<string, unknown> | null>;
    prepareFinancePayments1(facts: {
        payment: string;
        companyId: string;
        party_id: string | number | null;
        fact3: "receipt" | "payment";
        reference: string;
        date: string;
        currency: string | number | null;
        amountMinor: number;
        amountMinor2: number;
        bankAccountId: string;
        journal: string;
        userId: string;
        fact12: string | null;
        fact13: number;
        fact14: number;
        fact15: string | null;
    }): Effect;
    prepareFinanceAllocations1(facts: {
        fact0: string;
        companyId: string;
        payment: string;
        invoiceId: string;
        amountMinor: number;
        amountMinor2: number;
        amountMinor3: number;
        userId: string;
        fact8: number;
    }): Effect;
    prepareFinanceInvoices5(facts: {
        amountMinor: number;
        amountMinor2: number;
        amountMinor3: number;
        fact3: number;
        invoiceId: string;
    }): Effect;
    prepareFinanceExpenses1(facts: {
        fact0: number;
        fact1: number;
        fact2: number;
        fact3: number;
        fact4: number;
        id: string | number | null;
    }): Effect;
    prepareCommerceProducts1(facts: {
        sku: string;
        name: string;
        unit: string;
        trackingMode: "none" | "serial" | "lot";
        itemType: "stock" | "service";
        fact4: number;
        taxBps: number;
        status: "active" | "archived";
        id: string;
    }): Effect;
    prepareCommerceProducts2(facts: {
        record: string;
        companyId: string;
        sku: string;
        name: string;
        unit: string;
        trackingMode: "none" | "serial" | "lot";
        itemType: "stock" | "service";
        fact6: number;
        taxBps: number;
        status: "active" | "archived";
    }): Effect;
    prepareFinanceParties2(facts: {
        code: string;
        name: string;
        paymentTermsDays: number;
        fact3: number;
        status: "active" | "blocked";
        fact5: string | null;
        fact6: "unknown" | "granted" | "revoked";
        fact7: number;
        id: string;
    }): Effect;
    prepareFinanceParties3(facts: {
        record: string;
        companyId: string;
        kind: "supplier" | "customer";
        code: string;
        name: string;
        currency: string | number | null;
        paymentTermsDays: number;
        fact7: number;
        status: "active" | "blocked";
        fact9: string | null;
        fact10: "unknown" | "granted" | "revoked";
        fact11: number;
        fact12: number;
    }): Effect;
    prepareCommerceWarehouses1(facts: {
        code: string;
        name: string;
        status: "active" | "archived";
        id: string;
    }): Effect;
    prepareCommerceWarehouses2(facts: {
        record: string;
        companyId: string;
        branchId: string;
        code: string;
        name: string;
        status: "active" | "archived";
    }): Effect;
    readInventoryInTransit1(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        branchId3: string | null;
    }): Promise<CommerceRows<Row>>;
    guardRule1(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
        table: string;
        states: string[];
    }): Effect;
    guardRule2(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule3(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule4(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule5(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule6(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule7(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule8(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule9(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule10(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule11(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule12(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule13(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule14(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule15(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule16(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    guardRule17(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule18(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule19(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule20(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule21(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    guardRule22(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule23(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    guardRule24(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule25(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule26(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule27(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule28(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule29(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule30(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule31(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    guardRule32(facts: {
        key: string;
        guardId: string;
        values: (string | null)[];
    }): Effect;
    guardRule33(facts: {
        key: string;
        guardId: string;
        values: (string | null)[];
    }): Effect;
    guardRule34(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule35(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
    guardRule36(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    guardRule37(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule38(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    guardRule39(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    guardRule40(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule41(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule42(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    guardRule43(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule44(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    snapshotMoney(arg0: WorkContext, arg1: {
        amountUnits: string;
        currencyScale: number;
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
        currency: string;
    }): Promise<{
        amountUnits: string;
        usdUnits: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        statement: Effect;
    }>;
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
    }): Promise<CommerceAttachmentBinding<Effect>>;
    prepareBusinessEvidence(arg0: WorkContext, arg1: {
        attachmentSetIds: string[];
        targetTypes: string[];
        targetId: string;
        expectedTargetVersion: number;
        commandId: string;
        bindingType: string;
        bindingId: string;
        contexts: {
            purposeKey: string;
            componentKind: "action";
            componentRef: string;
        }[];
    } | {
        attachmentSetIds: string[];
        targetTypes: string[];
        bindingType: string;
        contexts: {
            purposeKey: string;
            componentKind: "action";
            componentRef: string;
        }[];
        targetId: string;
        expectedTargetVersion: number;
        commandId: string;
        bindingId: string;
    }): Promise<{
        manifest: {
            linkId: string;
            sourceId: string;
            purposeKey: string;
            checksumSha256: string;
            mediaType: string;
            bytes: number;
        }[];
        guards: Effect[];
        steps: Effect[];
    } | null>;
    prepareReturnCredit(arg0: WorkContext, arg1: {
        key: string;
        invoiceId: string;
        returnId: string;
        periodId: string;
        entryDate: string;
        postings: Posting[];
    }): Promise<{
        steps: Effect[];
        journalId: string;
        baseCreditMinor: number;
        controlAccountId: string;
        baseCurrency: string;
        basePostings: {
            baseDebit: number;
            baseCredit: number;
            debit: number;
            credit: number;
            accountId: string;
            description?: string | undefined;
            dimensions?: Record<string, string> | undefined;
        }[];
    }>;
    prepareReturnGoodsFx(arg0: WorkContext, arg1: {
        invoiceId: string;
        returnId: string;
        key: string;
        periodId: string;
        entryDate: string;
        stockBase: number;
        clearingAccountId: string;
        creditPostings: {
            baseDebit: number;
            baseCredit: number;
            debit: number;
            credit: number;
            accountId: string;
            description?: string | undefined;
            dimensions?: Record<string, string> | undefined;
        }[];
    }): Promise<{
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
    } | null>;
    snapshotMoneyReversal(arg0: WorkContext, arg1: {
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
    receiptQualityHoldSteps(arg0: WorkContext, arg1: {
        id: string;
        key: string;
        branchId: string;
        warehouseId: string;
        productId: string;
        lotId: string;
        quantityMilli: number;
        reason: string;
        dueDate: string;
        sourceType: "customer_return";
        sourceId: string;
    } | {
        id: string;
        key: string;
        branchId: string;
        warehouseId: string;
        productId: string;
        quantityMilli: number;
        reason: string;
        dueDate: string;
        sourceType: "customer_return";
        sourceId: string;
    } | {
        id: string;
        key: string;
        branchId: string;
        warehouseId: string;
        productId: string;
        lotId: string;
        quantityMilli: number;
        reason: string;
        dueDate: string;
        sourceId: string;
    }): Promise<Effect[]>;
    prepareJournal(arg0: WorkContext, arg1: {
        chartId: string;
        periodId: string;
        entryDate: string;
        currency: string;
        memo: string;
        sourceType: string;
        sourceId: string;
        isOpening: false;
        lines: {
            accountId: string;
            debit: string;
            credit: string;
        }[];
    } | {
        chartId: string;
        periodId: string;
        entryDate: string;
        currency: string;
        fxRate: string;
        sourceType: string;
        sourceId: string;
        memo: string;
        isOpening: false;
        lines: {
            accountId: string;
            debit: string;
            credit: string;
        }[];
    }, arg2: string, arg3: "posted" | "draft"): Promise<{
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
    refundNotifications(arg0: string, arg1: string, arg2: "approval", arg3: "Posted return requires independent refund approval", arg4: string): Effect;
    getEmployeeRequest(arg0: WorkContext, arg1: string): Promise<EmployeeRequest>;
    prepareRequestOrderHandoff(arg0: WorkContext, arg1: string, arg2: string, arg3: string, arg4: number | undefined, arg5: boolean, arg6: string | undefined): Promise<{
        row: EmployeeRequest;
        actor: WorkContext;
        steps: Effect[];
    }>;
    resolveSalesPricing(arg0: WorkContext, arg1: {
        kind: "sales" | "purchase" | "quote";
        partyId: string;
        warehouseId: string;
        orderDate: string;
        dueDate: string;
        note: string;
        lines: {
            productId: string;
            quantity: string;
            price: string;
            taxBps: number;
        }[];
        requestId?: string | undefined;
        requestVersion?: number | undefined;
        rfqId?: string | undefined;
        rfqVersion?: number | undefined;
        priceListId?: string | undefined;
        discountReason?: string | undefined;
        expectedVersion?: number | undefined;
    }): Promise<{
        list: SalesPricingRow | null;
        items: Map<string, SalesPricingRow>;
        requiresApproval: boolean;
    }>;
    prepareRfqOrderHandoff(arg0: WorkContext, arg1: {
        kind: "sales" | "purchase" | "quote";
        partyId: string;
        warehouseId: string;
        orderDate: string;
        dueDate: string;
        note: string;
        lines: {
            productId: string;
            quantity: string;
            price: string;
            taxBps: number;
        }[];
        requestId?: string | undefined;
        requestVersion?: number | undefined;
        rfqId?: string | undefined;
        rfqVersion?: number | undefined;
        priceListId?: string | undefined;
        discountReason?: string | undefined;
        expectedVersion?: number | undefined;
    }, arg2: string, arg3: string, arg4: boolean): Promise<Effect[]>;
    confirmedFunctionalFx(arg0: WorkContext, arg1: string, arg2: string, arg3: string): Promise<{
        micros: number;
        sourceRateId: string | null;
        targetRateId: string | null;
        usedLatestFallback: boolean;
    }>;
    supplierBillInvoiceHandoff(arg0: WorkContext, arg1: Row, arg2: {
        expectedVersion: number;
        date: string;
        dueDate: string;
        matchId?: string | undefined;
        matchVersion?: number | undefined;
    }, arg3: string, arg4: string): Promise<{
        bill: BillRow;
        steps: Effect[];
    } | null>;
    assertControlledThaiTaxPaymentOwner(arg0: WorkContext, arg1: string): Promise<void>;
    replayCommand<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    executeCommand<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, "companyId" | "branchId" | "departmentId">): Promise<Result>;
    pages(actor: WorkContext, query: {
        view: string;
        kind?: string;
        q?: string;
        cursor?: string;
        movementCursor?: string;
        new?: string;
        edit?: string;
        request?: string;
        status?: string;
        startDate?: string;
        endDate?: string;
    }): Promise<{
        page: ListPage<Row & {
            id: string;
            page_key: string;
        }>;
        movementPage: ListPage<Row & {
            id: string;
            page_key: string;
        }> | null;
    }>;
};
export type Posting = {
    account: string;
    debit: number;
    credit: number;
};
export function createCommerceApplication<Effect, Context = unknown>(ports: {
    clock: Clock;
    require(actor: WorkContext, permission: string): WorkContext;
    error(status: number, code: string, message: string): Error;
    hash(value: string): Promise<string>;
    forActor(actor: WorkContext): Promise<CommerceUnit<Effect>>;
    forContext(context: Context): Promise<CommerceUnit<Effect>>;
    attachmentReadable(): boolean;
    returnCompletionValues(input: {
        totalMinor: number;
        baseCreditMinor: number;
        outstandingMinor: number;
        baseOutstandingMinor: number;
    }): {
        creditedMinor: number;
        refundMinor: number;
        baseCreditedMinor: number;
        baseRefundMinor: number;
        realizedFxMinor: number;
    };
}) {
    async function commerceMoney(write: CommerceUnit<Effect>, actor: WorkContext, input: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
        currency: string;
        amountUnits: number;
    }) {
        if (!Number.isSafeInteger(input.amountUnits))
            throw ports.error(422, "MONEY_OVERFLOW", "The commerce amount exceeds supported ledger precision.");
        return (await write.snapshotMoney(actor, { ...input, amountUnits: String(input.amountUnits), currencyScale: 2 })).statement;
    }
    function allowed(actor: WorkContext, kind: keyof typeof roles) {
        return ports.require(actor, roles[kind]);
    }
    function scope(actor: WorkContext) { return { args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] }; }
    async function settings(db: CommerceUnit<Effect>, actor: WorkContext) {
        const row = await db.readCommerceSettings1({ companyId: actor.companyId });
        if (!row)
            throw ports.error(409, "COMMERCE_SETUP_REQUIRED", "Configure the company posting accounts before processing orders.");
        return row;
    }
    async function period(db: CommerceUnit<Effect>, actor: WorkContext, date: string, selectedId?: string) {
        const rows = await db.readFinancePeriods1({ companyId: actor.companyId, date: date, date2: date, fact3: selectedId ?? null, fact4: selectedId ?? null });
        if (rows.results.length !== 1)
            throw ports.error(409, "PERIOD_UNAVAILABLE", selectedId ? "Choose an open Company accounting period containing the payment date." : "Choose the accounting period when several open periods contain this date.");
        return String(rows.results[0].id);
    }
    function versionGuard(db: CommerceUnit<Effect>, key: string, table: string, id: string, version: number, states: string[]) {
        return db.guardRule1({ key: key, guardId: `version-${table}`, values: [id, version, ...states], table, states });
    }
    async function orderRow(db: CommerceUnit<Effect>, actor: WorkContext, id: string) {
        const s = scope(actor);
        const row = await db.readCommerceOrders1({ id: id, fact1: s.args });
        if (!row)
            throw ports.error(404, "ORDER_NOT_FOUND", "Order not found in this workspace.");
        return row;
    }
    async function commerceOverview(actor: WorkContext) {
        actor = allowed(actor, "read");
        const write = await ports.forActor(actor);
        const s = scope(actor);
        const [orders, products, parties, warehouses, stock, moves, nodes, config, priceLists, priceListItems] = await Promise.all([
            write.readCommerceOrders2({ fact0: s.args }),
            write.readCommerceProducts1({ companyId: actor.companyId }),
            write.readFinanceParties1({ companyId: actor.companyId }),
            write.readCommerceWarehouses1({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId }),
            write.readInventoryActiveReservations1({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId }),
            write.readInventoryLedger1({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId }),
            write.readOrganizationNodes1({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId }),
            settings(write, actor),
            write.readSalesPriceLists1({ companyId: actor.companyId }),
            write.readSalesPriceListItems1({ companyId: actor.companyId }),
        ]);
        return {
            orders: orders.results,
            products: products.results,
            parties: parties.results,
            warehouses: warehouses.results,
            stock: stock.results,
            moves: moves.results,
            nodes: nodes.results,
            currency: String(config.currency),
            priceLists: priceLists.results,
            priceListItems: priceListItems.results,
        };
    }
    async function workOrderRows(actor: WorkContext, ownerWrite?: CommerceUnit<Effect>) {
        actor = allowed(actor, "read");
        const write = ownerWrite ?? await ports.forActor(actor), s = scope(actor);
        return (await write.readCommerceOrders3({ fact0: s.args })).results;
    }
    async function getOrder(actor: WorkContext, id: string, ownerWrite?: CommerceUnit<Effect>) {
        actor = allowed(actor, "read");
        const write = ownerWrite ?? await ports.forActor(actor);
        const order = await orderRow(write, actor, id);
        // A composed owner command shares one PostgreSQL client. Defer each read
        // until the prior one completes while normal pooled page reads stay parallel.
        let readQueue: Promise<unknown> = Promise.resolve();
        const read = <T>(query: () => Promise<T>): Promise<T> => {
            if (!ownerWrite)
                return query();
            const result = readQueue.then(query);
            readQueue = result;
            return result;
        };
        const [lines, fulfillments, fulfillmentLines, returns, history, children, reservations, lots, lotAllocations, returnLotAllocations, returnQualityHolds, fulfillmentEvidence, pendingDeliveryEvidence, customerNotifications, customerNotificationSettings] = await Promise.all([
            read(() => write.readInventoryActiveReservations2({ id: id })),
            read(() => write.readCommerceFulfillments1({ id: id })),
            read(() => write.readCommerceReturnLines1({ id: id })),
            read(() => write.readCommerceReturns1({ id: id })),
            read(() => write.readHrPeople1({ id: id })),
            read(() => write.readCommerceOrders4({ id: id })),
            read(() => write.readInventoryOrderReservations1({ id: id })),
            read(() => write.readInventoryQualityHolds1({ id: id, warehouse_id: order.warehouse_id })),
            read(() => write.readCommerceReturnLotAllocations1({ id: id })),
            read(() => write.readCommerceReturnLotAllocations2({ id: id })),
            read(() => write.readInventoryQualityHolds2({ id: id })),
            read(() => write.readCommerceFulfillmentEvidence1({ id: id })),
            read(() => write.readAttachments1({ id: id })),
            read(() => write.readCommerceCustomerNotifications1({ id: id })),
            read(() => write.readCommerceCustomerNotificationSettings1({ companyId: actor.companyId })),
        ]);
        return {
            order,
            lines: lines.results,
            fulfillments: fulfillments.results,
            fulfillmentLines: fulfillmentLines.results,
            returns: returns.results,
            history: history.results,
            children: children.results,
            reservations: reservations.results,
            lots: lots.results,
            lotAllocations: lotAllocations.results,
            returnLotAllocations: returnLotAllocations.results,
            returnQualityHolds: returnQualityHolds.results,
            fulfillmentEvidence: fulfillmentEvidence.results,
            pendingDeliveryEvidence: pendingDeliveryEvidence.results,
            customerNotifications: customerNotifications.results,
            customerNotificationSettings: customerNotificationSettings ?? { company_id: actor.companyId, adapter: 'disabled', version: 0 },
        };
    }
    async function changeOrderReservation(actor: WorkContext, id: string, raw: unknown, key: string, ownerWrite?: CommerceUnit<Effect>) {
        actor = allowed(actor, 'stock');
        const input = stockReservationSchema.parse(raw);
        const write = ownerWrite ?? await ports.forActor(actor);
        const prior = await write.replayCommand<{ id:string; version:number }>(actor, key, 'commerce.stock.reserve', id, input);
        if (prior)
            return prior;
        const order = await orderRow(write, actor, id);
        if (order.kind !== 'sales')
            throw ports.error(422, 'SALES_ORDER_REQUIRED', 'Reserve stock against an approved sales order.');
        const line = await write.readCommerceOrderLines1({ lineId: input.lineId, id: id });
        if (line?.item_type === "service") throw ports.error(422, "SERVICE_RESERVATION_FORBIDDEN", "Services do not reserve inventory.");
        if (!line)
            throw ports.error(404, 'LINE_NOT_FOUND', 'Order line not found.');
        const qty = quantityMilli(input.quantity), now = ports.clock.now();
        const steps: Effect[] = [versionGuard(write, key, 'commerce_orders', id, input.expectedVersion, ['confirmed', 'partially_fulfilled'])];
        if (input.action === 'reserve') {
            if (!input.expiresAt || input.expiresAt <= now || input.expiresAt > now + 30 * 86400000)
                throw ports.error(422, 'RESERVATION_EXPIRY_REQUIRED', 'Choose an expiry within the next 30 days.');
            steps.push(write.guardRule2({ key: key, guardId: 'remaining', values: [line.id, qty] }), write.guardRule3({ key: key, guardId: 'active-stock', values: [order.warehouse_id, line.product_id, actor.companyId] }), write.prepareInventoryOrderReservations1({ fact0: `reservation-${key}`, id: line.id, warehouse_id: order.warehouse_id, product_id: line.product_id, qty: qty, expiresAt: input.expiresAt, userId: actor.userId, reason: input.reason, now: now }));
        }
        else {
            const held = (await write.readInventoryOrderReservations2({ id: line.id })).results;
            let left = qty;
            for (const row of held) {
                const n = Math.min(left, Number(row.quantity_milli) - Number(row.consumed_milli) - Number(row.released_milli));
                if (!n)
                    break;
                steps.push(write.prepareInventoryOrderReservations2({ n: n, reason: input.reason, id: row.id }));
                left -= n;
            }
            if (left)
                throw ports.error(422, 'OVER_RELEASE', 'Release quantity exceeds the remaining reservation.');
        }
        steps.push(write.prepareCommerceOrders1({ now: now, id: id }));
        return write.executeCommand(actor, key, 'commerce.stock.reserve', input, id, input.expectedVersion + 1, { id, version: input.expectedVersion + 1 }, steps, { companyId: actor.companyId, branchId: String(order.branch_id), departmentId: order.department_id ? String(order.department_id) : null });
    }
    async function requestCommerceReturn(actor: WorkContext, fulfillmentId: string, raw: z.input<typeof returnRequestSchema>, key: string, ownerWrite?: CommerceUnit<Effect>) {
        const input = returnRequestSchema.parse(raw);
        const commandInput = { fulfillmentId, ...input };
        const write = ownerWrite ?? await ports.forActor(actor), previous = await write.replayCommand<{ id:string; orderId:string; version:number; status:string }>(actor, key, "commerce.return.request", `return-${key}`, commandInput);
        if (previous)
            return previous;
        const fulfillment = await write.readCommerceFulfillments2({ fulfillmentId: fulfillmentId });
        if (!fulfillment)
            throw ports.error(404, "INVOICED_FULFILLMENT_NOT_FOUND", "An invoiced receipt or delivery was not found.");
        await orderRow(write, actor, String(fulfillment.order_id));
        actor = allowed(actor, fulfillment.kind === "purchase" ? "purchase" : "sales");
        if (!["posted", "partially_paid", "paid"].includes(String(fulfillment.invoice_status)))
            throw ports.error(409, "POSTED_INVOICE_REQUIRED", "Post the related invoice before requesting a return.");
        if (input.date < String(fulfillment.fulfillment_date))
            throw ports.error(422, "INVALID_RETURN_DATE", "Return date cannot precede the original stock movement.");
        if (new Set(input.lines.map(line => line.lineId)).size !== input.lines.length)
            throw ports.error(422, "DUPLICATE_LINE", "Each fulfillment line may appear once.");
        const id = `return-${key}`, lines: Array<Row & {
            quantity: number;
            returnValue: number;
            returnSubtotal: number;
            returnTax: number;
            index: number;
        }> = [], returnLots = new Map<string, Array<{
            id: string;
            fulfillmentAllocationId: string;
            lotId: string;
            quantity: number;
            value: number;
            snapshotQuantity: number;
            snapshotValue: number;
            snapshotVersion: number;
        }>>();
        let subtotal = 0, tax = 0;
        for (const [index, entry] of input.lines.entries()) {
            const line = await write.readCommerceFulfillmentLines1({ lineId: entry.lineId, fulfillmentId: fulfillmentId });
            if (!line)
                throw ports.error(422, "RETURN_LINE_INVALID", "Fulfillment line not found.");
            if (fulfillment.kind === 'sales' && line.item_type !== 'service') {
                if (!entry.qualityDueDate || entry.qualityDueDate < input.date)
                    throw ports.error(422, 'RETURN_QUALITY_DUE_DATE_REQUIRED', 'Choose a Quality review date on or after the customer return date.');
                if (entry.conditionNote.length < 3)
                    throw ports.error(422, 'RETURN_CONDITION_EVIDENCE_REQUIRED', 'Describe the returned item condition for Quality review.');
            }
            // Financial credit follows the reviewed supplier bill; stock retains receipt cost.
            const matchedLine = fulfillment.kind === 'purchase' ? await write.readProcurementBillMatchLines1({ invoice_id: fulfillment.invoice_id, id: line.id }) : null;
            const financialSubtotal = Number(matchedLine?.subtotal_minor ?? line.subtotal_minor), financialTax = Number(matchedLine?.tax_minor ?? line.tax_minor);
            const prior = await write.readCommerceReturnLines2({ id: line.id });
            const qty = quantityMilli(entry.quantity), remaining = Number(line.quantity_milli) - Number(prior?.quantity ?? 0);
            if (qty > remaining)
                throw ports.error(422, "OVER_RETURN", "Return quantity exceeds the unreturned fulfillment quantity.");
            const prepared: Array<{
                id: string;
                fulfillmentAllocationId: string;
                lotId: string;
                quantity: number;
                value: number;
                snapshotQuantity: number;
                snapshotValue: number;
                snapshotVersion: number;
            }> = [];
            let returnValue: number;
            if (String(line.tracking_mode) === 'none') {
                if (entry.allocations.length)
                    throw ports.error(422, "RETURN_LOT_NOT_ALLOWED", "Lot allocations are only valid for tracked products.");
                returnValue = qty === remaining ? Number(line.value_minor) - Number(prior?.value ?? 0) : scaledAmount(Number(line.value_minor), qty, Number(line.quantity_milli));
            }
            else {
                if (!entry.allocations.length)
                    throw ports.error(422, "RETURN_LOT_REQUIRED", "Select the fulfilled lot or serial being returned.");
                if (new Set(entry.allocations.map(a => a.lotId)).size !== entry.allocations.length)
                    throw ports.error(422, "DUPLICATE_RETURN_LOT", "Select each returned lot or serial once.");
                let selectedQty = 0;
                for (const [lotIndex, selection] of entry.allocations.entries()) {
                    const allocation = await write.readInventoryFulfillmentLotAllocations1({ id: line.id, lotId: selection.lotId });
                    if (!allocation)
                        throw ports.error(422, "RETURN_LOT_INVALID", "Choose a lot or serial from the original fulfillment.");
                    const previous = await write.readCommerceReturnLotAllocations3({ id: allocation.id }), lotQty = quantityMilli(selection.quantity), lotRemaining = Number(allocation.quantity_milli) - Number(previous?.quantity ?? 0);
                    if (lotQty > lotRemaining)
                        throw ports.error(422, "OVER_RETURN_LOT", "Returned lot or serial quantity exceeds the unreturned fulfillment allocation.");
                    if (String(allocation.tracking_mode) === 'serial' && lotQty !== 1000)
                        throw ports.error(422, "SERIAL_QUANTITY_INVALID", "Return a serial number as one whole unit.");
                    const lotValue = lotQty === lotRemaining ? Number(allocation.value_minor) - Number(previous?.value ?? 0) : scaledAmount(Number(allocation.value_minor), lotQty, Number(allocation.quantity_milli)), balance = await write.readInventoryLotBalances1({ lotId: selection.lotId, warehouse_id: fulfillment.warehouse_id });
                    if (fulfillment.kind === 'purchase' && (!balance || Number(balance.quantity_milli) < lotQty))
                        throw ports.error(409, "RETURN_LOT_STOCK_UNAVAILABLE", "The selected supplier-return lot is no longer available in this warehouse.");
                    prepared.push({ id: `${id}-${index}-lot-${lotIndex}`, fulfillmentAllocationId: String(allocation.id), lotId: selection.lotId, quantity: lotQty, value: lotValue, snapshotQuantity: Number(balance?.quantity_milli ?? 0), snapshotValue: Number(balance?.value_minor ?? 0), snapshotVersion: Number(balance?.version ?? 0) });
                    selectedQty += lotQty;
                }
                if (selectedQty !== qty)
                    throw ports.error(422, "RETURN_LOT_TOTAL_MISMATCH", "Returned lot or serial quantities must equal the fulfillment line return quantity.");
                returnValue = prepared.reduce((sum, lot) => sum + lot.value, 0);
            }
            const exact = qty === remaining, returnSubtotal = exact ? financialSubtotal - Number(prior?.subtotal ?? 0) : scaledAmount(financialSubtotal, qty, Number(line.quantity_milli)), returnTax = exact ? financialTax - Number(prior?.tax ?? 0) : scaledAmount(financialTax, qty, Number(line.quantity_milli));
            const expectedSubtotal = exact ? Number(line.subtotal_minor) - Number(prior?.expected_subtotal ?? 0) : scaledAmount(Number(line.subtotal_minor), qty, Number(line.quantity_milli));
            subtotal += returnSubtotal;
            tax += returnTax;
            lines.push({ ...line, quantity: qty, returnValue, returnSubtotal, returnTax, expectedSubtotal, index, condition: entry.condition, conditionNote: entry.conditionNote, qualityDueDate: entry.qualityDueDate });
            returnLots.set(`${id}-${index}`, prepared);
        }
        const total = subtotal + tax;
        const now = ports.clock.now(), kind = fulfillment.kind === "purchase" ? "supplier_return" : "customer_return", number = `${kind === "supplier_return" ? "SR" : "CR"}-${(await ports.hash(key)).slice(0, 24).toUpperCase()}`;
        const attachmentBinding = input.attachmentSetIds.length ? await write.prepareAttachmentBinding(actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: "commerce_fulfillment", bindingType: "commerce_return", contexts: [{ purposeKey: "return_condition", componentKind: "action", componentRef: "return-request" }] }, targetId: fulfillmentId, expectedTargetVersion: input.expectedVersion, commandId: key, bindingId: id }) : null;
        const steps: Effect[] = [
            ...(attachmentBinding?.guards ?? []),
            write.guardRule4({ key: key, guardId: "fulfillment", values: [fulfillmentId, input.expectedVersion] }),
            write.prepareCommerceReturns1({ id: id, companyId: actor.companyId, branch_id: fulfillment.branch_id, order_id: fulfillment.order_id, fulfillmentId: fulfillmentId, invoice_id: fulfillment.invoice_id, kind: kind, number: number, date: input.date, reason: input.reason, subtotal: subtotal, tax: tax, total: total, userId: actor.userId, now: now, now2: now }),
        ];
        for (const line of lines) {
            const returnLineId = `${id}-${line.index}`;
            steps.push(write.guardRule5({ key: key, guardId: `return-line-${line.index}`, values: [line.id, line.quantity, line.id, fulfillmentId] }), write.prepareCommerceReturnLines1({ returnLineId: returnLineId, id: id, id2: line.id, product_id: line.product_id, quantity: line.quantity, returnValue: line.returnValue, returnSubtotal: line.returnSubtotal, returnTax: line.returnTax, index: line.index, condition: line.condition, fact10: line.conditionNote || null, fact11: line.qualityDueDate || null, expectedSubtotal: line.expectedSubtotal }));
            for (const lot of returnLots.get(returnLineId) ?? [])
                steps.push(write.prepareCommerceReturnLotAllocations1({ id: lot.id, returnLineId: returnLineId, fulfillmentAllocationId: lot.fulfillmentAllocationId, lotId: lot.lotId, quantity: lot.quantity, value: lot.value, snapshotQuantity: lot.snapshotQuantity, snapshotValue: lot.snapshotValue, snapshotVersion: lot.snapshotVersion }));
        }
        if (attachmentBinding)
            steps.push(...attachmentBinding.steps);
        steps.push(write.prepareCommerceFulfillments1({ fulfillmentId: fulfillmentId, expectedVersion: input.expectedVersion }));
        return write.executeCommand(actor, key, "commerce.return.request", commandInput, id, 1, { id, orderId: String(fulfillment.order_id), status: "requested", version: 1 }, steps);
    }
    async function decideCommerceReturn(actor: WorkContext, returnId: string, action: "approve" | "reject", input: z.infer<typeof returnDecisionSchema>, key: string): Promise<CommerceReturnDecision> {
        const write = await ports.forActor(actor), previous = await write.replayCommand<CommerceReturnDecision>(actor, key, `commerce.return.${action}`, returnId, input);
        if (previous)
            return previous;
        const row = await write.readCommerceReturns2({ returnId: returnId, companyId: actor.companyId });
        if (!row)
            throw ports.error(404, "RETURN_NOT_FOUND", "Return request not found.");
        await orderRow(write, actor, String(row.order_id));
        if (!commercePermissions(actor).approve || samePerson(actor, row.requested_by))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different commerce manager must review the return.");
        const reviewEvidence = await write.prepareBusinessEvidence(actor, { attachmentSetIds: input.attachmentSetIds ?? [], targetTypes: ["commerce_return"], targetId: returnId, expectedTargetVersion: input.expectedVersion, commandId: key, bindingType: "commerce_return_decision", bindingId: `${returnId}:${input.expectedVersion + 1}`, contexts: ["condition_photo", "return_document"].map(purposeKey => ({ purposeKey, componentKind: "action" as const, componentRef: "return-review" })) });
        if (action === "reject")
            return write.executeCommand(actor, key, "commerce.return.reject", input, returnId, input.expectedVersion + 1, { id: returnId, orderId: String(row.order_id), status: "rejected", version: input.expectedVersion + 1 }, [
                ...(reviewEvidence?.guards ?? []), ...(reviewEvidence?.steps ?? []),
                write.guardRule6({ key: key, guardId: "return", values: [returnId, actor.companyId, input.expectedVersion] }),
                write.prepareCommerceReturns2({ userId: actor.userId, reason: input.reason, fact2: ports.clock.now(), returnId: returnId }),
            ]);
        const lines = (await write.readCommerceReturnLines3({ returnId })).results;
        const hasStock = lines.some(line => line.item_type !== "service");
        const original = hasStock ? await write.readCommerceFulfillments3({ fulfillment_id: row.fulfillment_id, companyId: actor.companyId }) : null;
        const originalLines = original ? (await write.readFinanceJournalLines1({ id: original.id })).results : [];
        const debitAccount = originalLines.find(line => Number(line.debit_minor) > 0)?.account_id, creditAccount = originalLines.find(line => Number(line.credit_minor) > 0)?.account_id;
        if (hasStock && (!original || original.status !== "posted" || originalLines.length !== 2 || !debitAccount || !creditAccount))
            throw ports.error(409, "ORIGINAL_POSTING_REVIEW_REQUIRED", "Review the original posted goods journal before reversing its stock value.");
        const chart = await write.readFinanceCharts1({ chart_id: row.chart_id, companyId: actor.companyId });
        const config: Row = { chart_id: original?.chart_id ?? row.chart_id, currency: original?.base_currency ?? chart?.base_currency ?? row.currency, inventory_account_id: row.kind === "customer_return" ? creditAccount ?? null : debitAccount ?? null, cogs_account_id: debitAccount ?? null, clearing_account_id: creditAccount ?? null };
        const periodId = await period(write, actor, String(row.return_date)), incoming = row.kind === "customer_return", stockJournal = hasStock ? `return-${key}-stock` : null, now = ports.clock.now(), value = lines.reduce((sum, line) => sum + Number(line.value_minor), 0);
        const stockPostings: Posting[] = incoming ? [{ account: String(config.inventory_account_id), debit: value, credit: 0 }, { account: String(config.cogs_account_id), debit: 0, credit: value }] : [{ account: String(config.clearing_account_id), debit: value, credit: 0 }, { account: String(config.inventory_account_id), debit: 0, credit: value }];
        let creditPostings: Posting[] = incoming ? [{ account: String(row.credit_account_id), debit: Number(row.subtotal_minor), credit: 0 }, ...(Number(row.tax_minor) ? [{ account: String(row.tax_account_id), debit: Number(row.tax_minor), credit: 0 }] : []), { account: String(row.debit_account_id), debit: 0, credit: Number(row.total_minor) }] : [{ account: String(row.credit_account_id), debit: Number(row.total_minor), credit: 0 }, { account: String(row.debit_account_id), debit: 0, credit: Number(row.subtotal_minor) }, ...(Number(row.tax_minor) ? [{ account: String(row.tax_account_id), debit: 0, credit: Number(row.tax_minor) }] : [])];
        if (!incoming) {
            const matched = await write.readProcurementBillMatches1({ invoice_id: row.invoice_id, companyId: actor.companyId });
            if (matched) {
                if (lines.some(line => line.expected_subtotal_minor === null))
                    throw ports.error(409, 'RETURN_GOODS_VALUATION_REVIEW_REQUIRED', 'Reconcile the original document-price basis before returning historical foreign goods.');
                const expected = lines.reduce((sum, line) => sum + Number(line.expected_subtotal_minor), 0), variance = Number(row.subtotal_minor) - expected;
                if (variance && !matched.variance_account_id)
                    throw ports.error(409, 'RETURN_VARIANCE_REVIEW_REQUIRED', 'Review the original supplier price variance before returning goods.');
                creditPostings = [{ account: String(matched.payable_account_id), debit: Number(row.total_minor), credit: 0 }, ...(expected ? [{ account: String(matched.clearing_account_id), debit: 0, credit: expected }] : []), ...(variance ? [{ account: String(matched.variance_account_id), debit: variance < 0 ? -variance : 0, credit: variance > 0 ? variance : 0 }] : []), ...(Number(row.tax_minor) ? [{ account: String(matched.tax_account_id), debit: 0, credit: Number(row.tax_minor) }] : [])];
            }
        }
        const credit = await write.prepareReturnCredit(actor, { key, invoiceId: String(row.invoice_id), returnId, periodId, entryDate: String(row.return_date), postings: creditPostings }), creditJournal = credit.journalId;
        const goodsFx = incoming ? null : await write.prepareReturnGoodsFx(actor, { invoiceId: String(row.invoice_id), returnId, key, periodId, entryDate: String(row.return_date), stockBase: value, clearingAccountId: String(config.clearing_account_id), creditPostings: credit.basePostings });
        const creditMoney = await write.snapshotMoneyReversal(actor, { sourceType: 'commerce_return', sourceId: returnId, amountKind: 'credit_total', transactionDate: String(row.return_date), amountUnits: String(row.total_minor), original: { sourceType: 'finance_invoice', sourceId: String(row.invoice_id), amountKind: 'total' } });
        const steps: Effect[] = [...(reviewEvidence?.guards ?? []), ...(reviewEvidence?.steps ?? []), write.prepareCommerceReturns3({ returnId: returnId }), write.prepareFinanceInvoices1({ invoice_id: row.invoice_id }), creditMoney.statement, write.guardRule7({ key: key, guardId: "supplier-reservation", values: [row.invoice_id] }), write.guardRule6({ key: key, guardId: "return", values: [returnId, actor.companyId, input.expectedVersion] }), ...(stockJournal ? journalSteps(write, actor, key, stockJournal, config, periodId, String(row.return_date), String(row.kind) + "_stock", returnId, stockPostings, "draft") : []), ...credit.steps];
        for (const [index, line] of lines.entries()) {
            if (line.item_type === "service") continue;
            if (Number(line.value_minor) > 0)
                steps.push((await write.snapshotMoneyReversal(actor, { sourceType: 'commerce_return_line', sourceId: String(line.id), amountKind: 'stock_value', transactionDate: String(row.return_date), amountUnits: String(line.value_minor), original: { sourceType: 'commerce_fulfillment_line', sourceId: String(line.fulfillment_line_id), amountKind: 'stock_value' } })).statement);
            const stock = await write.readCommerceStock1({ warehouse_id: row.warehouse_id, product_id: line.product_id });
            if (!stock)
                throw ports.error(409, "STOCK_NOT_FOUND", "Stock record is unavailable for this return.");
            if (!incoming && (Number(stock.quantity_milli) < Number(line.quantity_milli) || Number(stock.value_minor) < Number(line.value_minor)))
                throw ports.error(409, "INSUFFICIENT_RETURN_STOCK", "Supplier return quantity or value exceeds current stock.");
            steps.push(write.guardRule9({ key: key, guardId: `return-date-${index}`, values: [row.warehouse_id, line.product_id, row.return_date] }), write.guardRule10({ key: key, guardId: `return-stock-${index}`, values: [row.warehouse_id, line.product_id, stock.version] }), write.prepareCommerceStock1({ fact0: incoming ? line.quantity_milli : -Number(line.quantity_milli), fact1: incoming ? line.value_minor : -Number(line.value_minor), warehouse_id: row.warehouse_id, product_id: line.product_id }), write.prepareCommerceReturnStockMoves1({ fact0: `${returnId}-${index}`, returnId: returnId, warehouse_id: row.warehouse_id, product_id: line.product_id, fact4: incoming ? line.quantity_milli : -Number(line.quantity_milli), fact5: incoming ? line.value_minor : -Number(line.value_minor), return_date: row.return_date, now: now }));
            const lotAllocations = (await write.readCommerceReturnLotAllocations4({ id: line.id })).results;
            for (const [lotIndex, lot] of lotAllocations.entries()) {
                if (Number(lot.snapshot_version) === 0)
                    steps.push(write.guardRule11({ key: key, guardId: `return-lot-${index}-${lotIndex}`, values: [lot.lot_id, row.warehouse_id] }));
                else
                    steps.push(write.guardRule12({ key: key, guardId: `return-lot-${index}-${lotIndex}`, values: [lot.lot_id, row.warehouse_id, lot.snapshot_version, lot.snapshot_quantity_milli, lot.snapshot_value_minor] }));
                if (!incoming && (Number(lot.snapshot_quantity_milli) < Number(lot.quantity_milli) || Number(lot.snapshot_value_minor) < Number(lot.value_minor)))
                    throw ports.error(409, "RETURN_LOT_STOCK_UNAVAILABLE", "The selected supplier-return lot quantity or value is no longer available.");
                if (incoming)
                    steps.push(write.prepareInventoryLotBalances1({ lot_id: lot.lot_id, warehouse_id: row.warehouse_id, quantity_milli: lot.quantity_milli, value_minor: lot.value_minor }), write.prepareInventoryLots1({ lot_id: lot.lot_id }));
                else
                    steps.push(write.prepareInventoryLotBalances2({ quantity_milli: lot.quantity_milli, value_minor: lot.value_minor, lot_id: lot.lot_id, warehouse_id: row.warehouse_id }), write.prepareInventoryLots2({ lot_id: lot.lot_id, lot_id2: lot.lot_id }));
                steps.push(write.prepareInventoryLotMoves1({ fact0: `lot-return-${returnId}-${index}-${lotIndex}`, lot_id: lot.lot_id, warehouse_id: row.warehouse_id, kind: row.kind, fact4: String(lot.id), fact5: incoming ? lot.quantity_milli : -Number(lot.quantity_milli), fact6: incoming ? lot.value_minor : -Number(lot.value_minor), return_date: row.return_date, userId: actor.userId, now: now }));
                if (incoming)
                    steps.push(...await write.receiptQualityHoldSteps(actor, { id: `quality-return-${key}-${index}-${lotIndex}`, key: `${key}-quality-${index}-${lotIndex}`, branchId: String(row.branch_id), warehouseId: String(row.warehouse_id), productId: String(line.product_id), lotId: String(lot.lot_id), quantityMilli: Number(lot.quantity_milli), reason: `Customer return · ${String(line.condition_code)} · ${String(line.condition_note)}`, dueDate: String(line.quality_due_date), sourceType: 'customer_return', sourceId: String(lot.id) }));
            }
            if (incoming && !lotAllocations.length)
                steps.push(...await write.receiptQualityHoldSteps(actor, { id: `quality-return-${key}-${index}`, key: `${key}-quality-${index}`, branchId: String(row.branch_id), warehouseId: String(row.warehouse_id), productId: String(line.product_id), quantityMilli: Number(line.quantity_milli), reason: `Customer return · ${String(line.condition_code)} · ${String(line.condition_note)}`, dueDate: String(line.quality_due_date), sourceType: 'customer_return', sourceId: String(line.id) }));
        }
        if (goodsFx)
            steps.push(...goodsFx.steps);
        steps.push(write.prepareCommerceReturns4({ userId: actor.userId, stockJournal: stockJournal, creditJournal: creditJournal, baseCreditMinor: credit.baseCreditMinor, fact4: goodsFx?.result.id ?? null, now: now, returnId: returnId }));
        return write.executeCommand(actor, key, "commerce.return.approve", input, returnId, input.expectedVersion + 1, { id: returnId, orderId: String(row.order_id), status: "approved", version: input.expectedVersion + 1, ...(stockJournal ? { stockJournalId: stockJournal } : {}), creditJournalId: creditJournal, ...(goodsFx ? { goodsFxJournalId: goodsFx.result.id } : {}) }, steps);
    }
    async function completeCommerceReturn(actor: WorkContext, returnId: string, input: z.infer<typeof returnCompleteSchema>, key: string): Promise<CommerceReturnCompletion> {
        actor = allowed(actor, "finance");
        const write = await ports.forActor(actor), previous = await write.replayCommand<CommerceReturnCompletion>(actor, key, "commerce.return.complete", returnId, input);
        if (previous)
            return previous;
        const row = await write.readCommerceReturns3({ returnId: returnId, companyId: actor.companyId });
        if (!row)
            throw ports.error(404, "RETURN_NOT_FOUND", "Return request not found.");
        await orderRow(write, actor, String(row.order_id));
        if (!hasPermission(actor, "finance.approve") || samePerson(actor, row.requested_by))
            throw ports.error(403, "INDEPENDENT_FINANCE_REVIEW_REQUIRED", "A different finance approver must complete the credit or debit note.");
        const incoming = row.kind === 'customer_return', control = String(incoming ? row.debit_account_id : row.credit_account_id), carried = await write.readFinanceJournalLines2({ incoming: incoming, credit_journal_id: row.credit_journal_id, control: control });
        if (carried === null || carried < 0 || row.base_credit_minor !== null && Number(row.base_credit_minor) !== carried)
            throw ports.error(409, 'RETURN_CARRYING_RECONCILIATION_REQUIRED', 'Reconcile the posted original credit-note control value before completion.');
        const values = ports.returnCompletionValues({ totalMinor: Number(row.total_minor), baseCreditMinor: carried, outstandingMinor: Number(row.outstanding_minor), baseOutstandingMinor: Number(row.base_outstanding_minor) }), credited = values.creditedMinor, refund = values.refundMinor, now = ports.clock.now(), refundId = refund ? `refund-${returnId}` : null;
        let fx: CommerceJournal<Effect> | null = null;
        if (values.realizedFxMinor) {
            const chart = await write.readFinanceCharts1({ chart_id: row.chart_id, companyId: actor.companyId });
            if (!chart)
                throw ports.error(422, 'ACCOUNT_INVALID', 'The original invoice chart is unavailable.');
            const gain = (incoming ? -values.realizedFxMinor : values.realizedFxMinor) > 0, account = gain ? input.fxGainAccountId : input.fxLossAccountId;
            if (!account)
                throw ports.error(422, gain ? 'FX_GAIN_ACCOUNT_REQUIRED' : 'FX_LOSS_ACCOUNT_REQUIRED', 'Select the approved realized FX account to reconcile the current invoice carrying value.');
            if (!await write.readFinanceAccounts1({ account: account, companyId: actor.companyId, chart_id: row.chart_id, base_currency: chart.base_currency, fact4: gain ? 'other_income' : 'other_expense' }))
                throw ports.error(422, 'FX_ACCOUNT_INVALID', 'Use a same-chart functional-currency realized gain or loss account.');
            const amount = minorToDecimal(Math.abs(values.realizedFxMinor)), controlDebit = incoming ? values.realizedFxMinor < 0 : values.realizedFxMinor > 0;
            fx = await write.prepareJournal(actor, { chartId: String(row.chart_id), periodId: await period(write, actor, String(row.return_date)), entryDate: String(row.return_date), currency: chart.base_currency, memo: `Realized carrying adjustment ${returnId}`, sourceType: 'commerce_return_fx', sourceId: returnId, isOpening: false, lines: [{ accountId: control, debit: controlDebit ? amount : '0', credit: controlDebit ? '0' : amount }, { accountId: account, debit: controlDebit ? '0' : amount, credit: controlDebit ? amount : '0' }] }, `${key}:fx`, 'posted');
        }
        const steps: Effect[] = [
            write.prepareFinanceInvoices1({ invoice_id: row.invoice_id }),
            ...(fx?.steps ?? []),
            write.guardRule7({ key: key, guardId: "supplier-reservation", values: [row.invoice_id] }),
            write.guardRule14({ key: key, guardId: "return", values: [returnId, actor.companyId, input.expectedVersion, row.invoice_version] }),
            write.guardRule15({ key: key, guardId: "return-periods", values: [row.stock_journal_id, row.credit_journal_id, actor.companyId] }),
            write.guardRule16({ key: key, guardId: "goods-fx-posted", values: [returnId] }),
            write.prepareFinanceInvoices3({ credited: credited, baseCreditedMinor: values.baseCreditedMinor, credited2: credited, now: now, invoice_id: row.invoice_id }),
            write.prepareCommerceReturns5({ credited: credited, refund: refund, carried: carried, baseCreditedMinor: values.baseCreditedMinor, baseRefundMinor: values.baseRefundMinor, fact5: fx?.result.id ?? null, now: now, returnId: returnId }),
        ];
        if (refundId)
            steps.push(write.prepareFinanceRefundObligations1({ refundId: refundId, companyId: actor.companyId, branch_id: row.branch_id, department_id: row.department_id, returnId: returnId, invoice_id: row.invoice_id, party_id: row.party_id, fact7: row.kind === "customer_return" ? "outgoing" : "incoming", currency: row.currency, chart_id: row.chart_id, fact10: row.kind === "customer_return" ? row.debit_account_id : row.credit_account_id, refund: refund, refund2: refund, baseRefundMinor: values.baseRefundMinor, baseRefundMinor2: values.baseRefundMinor, credit_journal_id: row.credit_journal_id, userId: actor.userId, now: now, now2: now }));
        if (refundId)
            steps.push(write.refundNotifications(key, refundId, 'approval', 'Posted return requires independent refund approval', actor.userId));
        return write.executeCommand(actor, key, "commerce.return.complete", input, returnId, input.expectedVersion + 1, { id: returnId, orderId: String(row.order_id), status: "posted", version: input.expectedVersion + 1, creditedMinor: credited, refundMinor: refund, refundId }, steps);
    }
    async function saveOrder(actor: WorkContext, id: string | null, input: z.infer<typeof orderSchema>, key: string, ownerWrite?: CommerceUnit<Effect>) {
        actor = allowed(actor, input.kind === "purchase" ? "purchase" : "sales");
        const write = ownerWrite ?? await ports.forActor(actor);
        const orderId = id ?? `order-${key}`;
        const linked = id ? await write.readEmployeeRequests1({ companyId: actor.companyId, id: id, id2: id }) : null;
        const requestId = linked?.id ?? input.requestId;
        if (requestId) {
            const request = await write.getEmployeeRequest(actor, requestId);
            if (input.kind !== "purchase")
                throw ports.error(422, "PURCHASE_REQUIRED", "Resource requests hand off to purchase orders.");
            actor = { ...actor, branchId: request.branch_id, departmentId: request.department_id };
        }
        const config = await settings(write, actor);
        const previous = await write.replayCommand<{ id:string; version:number }>(actor, key, id ? "commerce.order.update" : "commerce.order.create", id ?? `order-${key}`, input);
        if (previous)
            return previous;
        const handoff = requestId ? await write.prepareRequestOrderHandoff(actor, requestId, orderId, key, input.requestVersion, !!id, input.rfqId) : null;
        const existing = id ? await orderRow(write, actor, id) : null;
        if (existing && existing.kind !== input.kind)
            throw ports.error(422, "ORDER_KIND_LOCKED", "The document type cannot change.");
        if (input.dueDate < input.orderDate)
            throw ports.error(422, "INVALID_DATES", "Due date must be on or after order date.");
        if (new Set(input.lines.map((l) => l.productId)).size !== input.lines.length)
            throw ports.error(422, "DUPLICATE_PRODUCT", "Use one line per product.");
        const warehouse = input.warehouseId ? await write.readCommerceWarehouses2({ warehouseId: input.warehouseId, companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId }) : null;
        if (input.warehouseId && !warehouse)
            throw ports.error(422, "WAREHOUSE_INVALID", "Choose an active warehouse in this workspace.");
        const record = id ?? `order-${key}`;
        const version = existing ? (input.expectedVersion ?? 0) + 1 : 1;
        const now = ports.clock.now();
        const pricing = await write.resolveSalesPricing(actor, input);
        let subtotal = 0, tax = 0;
        const lineRows = [];
        for (const [index, line] of input.lines.entries()) {
            const product = await write.readCommerceProducts2({ productId: line.productId, companyId: actor.companyId });
            if (!product)
                throw ports.error(422, "PRODUCT_INVALID", "Choose an active product.");
            if (product.item_type !== "service" && !warehouse)
                throw ports.error(422, "WAREHOUSE_REQUIRED", "Choose a warehouse for stock items.");
            if (product.item_type === "service" && input.kind === "purchase")
                throw ports.error(422, "SERVICE_SUPPLIER_BILL_REQUIRED", "Record purchased services through supplier bills.");
            const qty = quantityMilli(line.quantity);
            const price = decimalToMinor(line.price);
            const priceItem = pricing.items.get(line.productId), listPrice = Number(priceItem?.price_minor ?? price), discountBps = listPrice > 0 && price < listPrice ? Math.floor((listPrice - price) * 10000 / listPrice) : 0;
            const values = lineAmounts(qty, price, line.taxBps);
            subtotal += values.subtotal;
            tax += values.tax;
            lineRows.push({
                id: `${record}-line-${index}`,
                product,
                line,
                qty,
                price,
                listPrice,
                discountBps,
                ...values,
                index,
            });
        }
        if (!Number.isSafeInteger(subtotal + tax) || subtotal <= 0)
            throw ports.error(422, "ORDER_AMOUNT_INVALID", "Order total must be positive and within supported limits.");
        const money = [await commerceMoney(write, actor, { sourceType: 'commerce_order_revision', sourceId: `${record}:${version}`, amountKind: 'total', transactionDate: input.orderDate, currency: String(config.currency), amountUnits: subtotal + tax })];
        for (const line of lineRows)
            for (const [amountKind, amountUnits] of [['unit_price', line.price], ['subtotal', line.subtotal], ['tax', line.tax]] as const)
                money.push(await commerceMoney(write, actor, { sourceType: 'commerce_order_line_revision', sourceId: `${line.id}:${version}`, amountKind, transactionDate: input.orderDate, currency: String(config.currency), amountUnits }));
        const steps = [
            ...money,
            write.guardRule17({ key: key, guardId: "party", values: [
                    input.partyId,
                    actor.companyId,
                    input.kind === "purchase" ? "supplier" : "customer",
                    config.currency,
                ] }),
        ];
        if (pricing.list)
            steps.push(write.guardRule18({ key: key, guardId: 'price-list', values: [pricing.list.id, actor.companyId, pricing.list.version, input.orderDate, input.orderDate] }));
        if (existing) {
            steps.push(versionGuard(write, key, "commerce_orders", record, input.expectedVersion ?? 0, ["draft"]));
            steps.push(write.prepareCommerceOrders2({ partyId: input.partyId, warehouseId: input.warehouseId || null, branch_id: warehouse?.branch_id ?? actor.branchId, orderDate: input.orderDate, dueDate: input.dueDate, note: input.note, subtotal: subtotal, tax: tax, fact8: pricing.list?.id ?? null, fact9: pricing.list?.version ?? null, fact10: input.discountReason?.trim() || null, fact11: pricing.requiresApproval ? 1 : 0, now: now, record: record }));
            steps.push(write.prepareCommerceOrderLines1({ record: record }));
        }
        else
            steps.push(write.prepareCommerceOrders3({ record: record, companyId: actor.companyId, branch_id: warehouse?.branch_id ?? actor.branchId, departmentId: actor.departmentId, kind: input.kind, fact5: `${input.kind === "quote" ? "QT" : input.kind === "sales" ? "SO" : "PO"}-${(await ports.hash(key)).slice(0, 24).toUpperCase()}`, partyId: input.partyId, warehouseId: input.warehouseId || null, currency: config.currency, orderDate: input.orderDate, dueDate: input.dueDate, note: input.note, subtotal: subtotal, tax: tax, fact14: pricing.list?.id ?? null, fact15: pricing.list?.version ?? null, fact16: input.discountReason?.trim() || null, fact17: pricing.requiresApproval ? 1 : 0, userId: actor.userId, now: now, now2: now }));
        for (const l of lineRows) {
            steps.push(write.guardRule19({ key: key, guardId: `product-${l.index}`, values: [l.product.id, l.product.version] }));
            steps.push(write.prepareCommerceOrderLines2({ id: l.id, record: record, productId: l.line.productId, name: l.product.name, qty: l.qty, price: l.price, taxBps: l.line.taxBps, subtotal: l.subtotal, tax: l.tax, index: l.index, listPrice: l.listPrice, discountBps: l.discountBps, itemType: l.product.item_type === "service" ? "service" : "stock" }));
        }
        if (handoff)
            steps.push(...handoff.steps);
        // The RFQ award and draft PO commit together; ordinary PO approval remains unchanged.
        steps.push(...await write.prepareRfqOrderHandoff(actor, input, record, key, !!existing));
        return write.executeCommand(actor, key, existing ? "commerce.order.update" : "commerce.order.create", input, record, version, { id: record, version }, steps);
    }
    async function transitionOrder(actor: WorkContext, id: string, input: z.infer<typeof transitionSchema>, key: string, ownerWrite?: CommerceUnit<Effect>) {
        const write = ownerWrite ?? await ports.forActor(actor);
        const previous = await write.replayCommand<{ id:string; version:number; state:string; salesId:string|null }>(actor, key, `commerce.order.${input.action}`, id, input);
        if (previous)
            return previous;
        const row = await orderRow(write, actor, id);
        actor = allowed(actor, row.kind === "purchase" ? "purchase" : "sales");
        if (input.action === "approve" &&
            (!commercePermissions(actor).approve || samePerson(actor, row.created_by)))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different manager must approve this document.");
        if (input.action === 'approve' && Number(row.discount_approval_required) && (!input.reason || input.reason.trim().length < 5))
            throw ports.error(422, 'DISCOUNT_APPROVAL_REASON_REQUIRED', 'Record the independent discount approval reason.');
        if (input.action === "cancel" &&
            ["confirmed", "partially_fulfilled", "fulfilled"].includes(String(row.state)) &&
            (!commercePermissions(actor).approve || samePerson(actor, row.created_by)))
            throw ports.error(403, "INDEPENDENT_CANCELLER_REQUIRED", "A different manager must cancel an approved document.");
        if (input.action === "cancel" &&
            ["confirmed", "partially_fulfilled", "fulfilled"].includes(String(row.state))) {
            const fulfillment = await write.readCommerceFulfillments4({ id: id });
            if (fulfillment)
                throw ports.error(409, "ORDER_REVERSAL_REQUIRED", "This order has stock movements. Use the return workflow instead of cancellation.");
        }
        if (input.action === "convert" && row.kind !== "quote")
            throw ports.error(422, "QUOTE_REQUIRED", "Only a quotation can be converted.");
        const from = input.action === "submit"
            ? ["draft"]
            : input.action === "cancel"
                ? row.state === "confirmed"
                    ? ["confirmed"]
                    : ["draft"]
                : input.action === "convert"
                    ? ["confirmed"]
                    : ["submitted"];
        const state = input.action === "submit"
            ? "submitted"
            : input.action === "approve"
                ? "confirmed"
                : input.action === "return"
                    ? "draft"
                    : input.action === "convert"
                        ? "converted"
                        : "cancelled";
        const steps = [
            versionGuard(write, key, "commerce_orders", id, input.expectedVersion, from),
            ...(input.action === 'cancel' ? [write.prepareInventoryOrderReservations3({ id: id })] : []),
            write.prepareCommerceOrders4({ state: state, fact1: input.action === "approve" ? actor.userId : row.approved_by, fact2: input.action === 'approve', userId: actor.userId, fact4: input.action === 'return', fact5: input.action === 'approve', fact6: ports.clock.now(), fact7: input.action === 'return', fact8: ports.clock.now(), id: id }),
        ];
        if (input.action === "approve" && row.kind === "sales") {
            steps.unshift(write.prepareFinanceParties1({ party_id: row.party_id, companyId: actor.companyId }), write.guardRule20({ key: key, guardId: "customer-credit-limit", values: [row.party_id, actor.companyId, Number(row.subtotal_minor) + Number(row.tax_minor)] }));
        }
        let salesId: string | null = null;
        if (input.action === "convert") {
            salesId = `order-${key}`;
            steps.push(await commerceMoney(write, actor, { sourceType: 'commerce_order_revision', sourceId: `${salesId}:1`, amountKind: 'total', transactionDate: String(row.order_date), currency: String(row.currency), amountUnits: Number(row.subtotal_minor) + Number(row.tax_minor) }));
            steps.push(write.prepareCommerceOrders5({ salesId: salesId, fact1: `SO-${(await ports.hash(key)).slice(0, 24).toUpperCase()}`, userId: actor.userId, fact3: ports.clock.now(), fact4: ports.clock.now(), id: id }));
            steps.push(write.prepareCommerceOrderLines3({ salesId: salesId, salesId2: salesId, id: id }));
        }
        return write.executeCommand(actor, key, `commerce.order.${input.action}`, input, id, input.expectedVersion + 1, { id, state, version: input.expectedVersion + 1, salesId }, steps);
    }
    function journalSteps(db: CommerceUnit<Effect>, actor: WorkContext, key: string, journalId: string, config: Row, periodId: string, date: string, source: string, sourceId: string, lines: Posting[], status: "draft" | "posted" = "posted") {
        const debit = lines.reduce((s, l) => s + l.debit, 0), credit = lines.reduce((s, l) => s + l.credit, 0);
        if (debit !== credit || !Number.isSafeInteger(debit))
            throw ports.error(422, "UNBALANCED_JOURNAL", "The journal is not balanced.");
        const now = ports.clock.now();
        const steps = [
            db.guardRule21({ key: key, guardId: `period-${journalId}`, values: [periodId, actor.companyId, date, date] }),
            db.prepareFinanceJournals1({ journalId: journalId, companyId: actor.companyId, chart_id: config.chart_id, periodId: periodId, fact4: `GL-${journalId.slice(-36)}`, date: date, currency: config.currency, source: source, sourceId: sourceId, fact9: `${source} ${sourceId}`, status: status, userId: actor.userId, fact12: status === "posted" ? actor.userId : null, fact13: status === "posted" ? now : null, now: now, now2: now }),
        ];
        for (const [i, line] of lines.filter((l) => l.debit || l.credit).entries()) {
            steps.push(db.guardRule22({ key: key, guardId: `account-${journalId}-${i}`, values: [line.account, actor.companyId, config.chart_id] }));
            steps.push(db.prepareFinanceJournalLines1({ fact0: `${journalId}-${i}`, journalId: journalId, account: line.account, debit: line.debit, credit: line.credit, debit2: line.debit, credit2: line.credit, i: i }));
        }
        return steps;
    }
    async function fulfillOrder(actor: WorkContext, id: string, raw: z.input<typeof fulfillmentSchema>, key: string) {
        const input = fulfillmentSchema.parse(raw);
        actor = allowed(actor, input.completionMode === "service" ? "sales" : "stock");
        if (input.completionMode === "mixed") actor = allowed(actor, "sales");
        const write = await ports.forActor(actor);
        const previous = await write.replayCommand<{ id:string; version:number; fulfillmentId:string }>(actor, key, "commerce.order.fulfill", id, input);
        if (previous)
            return previous;
        const order = await orderRow(write, actor, id);
        if (order.kind === "quote")
            throw ports.error(422, "ORDER_REQUIRED", "Convert the quotation to a sales order first.");
        const config = await settings(write, actor);
        const periodId = await period(write, actor, input.date);
        if (input.completionMode !== "service" && (!config.inventory_account_id || !config.cogs_account_id || !config.clearing_account_id))
            throw ports.error(409, "INVENTORY_POSTING_SETUP_REQUIRED", "Ask Finance to configure inventory, cost of sales and clearing accounts before delivering goods.");
        const fulfillment = `delivery-${key}`;
        const journal = `journal-${key}`;
        const incoming = order.kind === "purchase";
        if (incoming && input.completionMode !== "goods")
            throw ports.error(422, "SERVICE_SUPPLIER_BILL_REQUIRED", "Purchased services use supplier bills.");
        if (input.date < String(order.order_date))
            throw ports.error(422, "INVALID_COMPLETION_DATE", "Completion cannot precede the order date.");
        const stockCurrency = String(config.base_currency), valuationFx = await write.confirmedFunctionalFx(actor, incoming ? String(order.currency) : stockCurrency, stockCurrency, input.date);
        const convertedStock = (units: number) => {
            const value = convertUnits(String(units), 2, 2, '1', minorToDecimal(valuationFx.micros, 6));
            if (value > BigInt(Number.MAX_SAFE_INTEGER))
                throw ports.error(422, 'MONEY_OVERFLOW', 'The functional inventory amount exceeds supported ledger precision.');
            return Number(value);
        };
        if (!incoming && input.completionMode === "service" && input.evidenceNote.length < 3)
            throw ports.error(422, "SERVICE_COMPLETION_NOTE_REQUIRED", "Describe the service completed and who confirmed it.");
        if (!incoming && input.completionMode !== "service" && ((!input.evidenceIds.length && !input.attachmentSetIds.length) || input.evidenceNote.length < 3))
            throw ports.error(422, 'DELIVERY_EVIDENCE_REQUIRED', 'Attach proof of delivery and record who or what confirmed receipt.');
        if (new Set(input.evidenceIds).size !== input.evidenceIds.length)
            throw ports.error(422, 'DELIVERY_EVIDENCE_DUPLICATE', 'Use each delivery evidence file once.');
        if (!incoming)
            for (const attachmentId of input.evidenceIds) {
                const valid = await write.readAttachments2({ attachmentId: attachmentId, id: id });
                if (!valid)
                    throw ports.error(422, 'DELIVERY_EVIDENCE_INVALID', 'The selected proof is missing, revoked, belongs to another order, or is already linked to a delivery.');
            }
        if (new Set(input.lines.map((l) => l.lineId)).size !== input.lines.length)
            throw ports.error(422, "DUPLICATE_LINE", "Each order line may appear once.");
        const attachmentBinding = await write.prepareBusinessEvidence(actor, { attachmentSetIds: input.attachmentSetIds, targetTypes: incoming ? ["purchase_order", "purchase_receipt", "commerce_order"] : ["commerce_order_delivery"], bindingType: incoming ? "purchase_receipt" : "commerce_fulfillment", contexts: incoming ? [...(["delivery_note", "packing_list", "damage_photo"].map(purposeKey => ({ purposeKey, componentKind: "action" as const, componentRef: "goods-receipt" }))), { purposeKey: "purchase_receipt", componentKind: "action", componentRef: "goods-receipt" }] : [{ purposeKey: "proof_of_delivery", componentKind: "action", componentRef: "delivery" }], targetId: id, expectedTargetVersion: input.expectedVersion, commandId: key, bindingId: fulfillment });
        const steps: Effect[] = [
            ...(attachmentBinding?.guards ?? []),
            versionGuard(write, key, "commerce_orders", id, input.expectedVersion, [
                "confirmed",
                "partially_fulfilled",
            ]),
            write.prepareCommerceFulfillments2({ fulfillment: fulfillment, id: id, fact2: `${incoming ? "GR" : "DN"}-${(await ports.hash(key)).slice(0, 24).toUpperCase()}`, date: input.date, userId: actor.userId, fact5: ports.clock.now(), fact6: incoming ? null : input.evidenceNote, completionMode: input.completionMode }),
        ];
        if (!incoming)
            for (const [index, attachmentId] of input.evidenceIds.entries())
                steps.push(write.guardRule23({ key: key, guardId: `delivery-evidence-${index}`, values: [attachmentId, id] }));
        let value = 0, documentSubtotal = 0, goodsCount = 0;
        for (const [i, entry] of input.lines.entries()) {
            const line = await write.readCommerceOrderLines2({ lineId: entry.lineId, id: id });
            if (!line)
                throw ports.error(422, "LINE_INVALID", "Order line not found.");
            const qty = quantityMilli(entry.quantity);
            const remaining = Number(line.quantity_milli) - Number(line.fulfilled_milli);
            if (qty > remaining)
                throw ports.error(422, "OVER_FULFILLMENT", "Quantity exceeds the remaining order quantity.");
            if (line.item_type === "service") {
                if (incoming || input.completionMode === "goods")
                    throw ports.error(422, "SERVICE_COMPLETION_REQUIRED", "Complete service lines through the service action.");
                if (entry.allocations?.length)
                    throw ports.error(422, "SERVICE_STOCK_ALLOCATION_FORBIDDEN", "Services have no lot or serial allocations.");
                const prior = await write.readCommerceFulfillmentLines2({ id: line.id });
                const amounts = qty === remaining ? { subtotal: Number(line.subtotal_minor) - Number(prior?.subtotal ?? 0), tax: Number(line.tax_minor) - Number(prior?.tax ?? 0) } : lineAmounts(qty, Number(line.price_minor), Number(line.tax_bps));
                steps.push(write.guardRule25({ key, guardId: `service-line-${i}`, values: [line.id, line.fulfilled_milli, qty] }));
                for (const [amountKind, amountUnits] of [["subtotal", amounts.subtotal], ["tax", amounts.tax], ["total", amounts.subtotal + amounts.tax]] as const)
                    steps.push(await commerceMoney(write, actor, { sourceType: "commerce_fulfillment_line", sourceId: `${fulfillment}-${i}`, amountKind, transactionDate: input.date, currency: String(order.currency), amountUnits }));
                steps.push(write.prepareCommerceFulfillmentLines1({ fact0: `${fulfillment}-${i}`, fulfillment, id: line.id, qty, cost: 0, subtotal: amounts.subtotal, tax: amounts.tax, stockCurrency }));
                steps.push(write.prepareCommerceOrderLines4({ qty, id: line.id }));
                continue;
            }
            if (input.completionMode === "service" || !order.warehouse_id)
                throw ports.error(422, "STOCK_DELIVERY_REQUIRED", "Stock items require warehouse delivery authority.");
            goodsCount += 1;
            const stock = await write.readInventoryActiveReservations3({ warehouse_id: order.warehouse_id, product_id: line.product_id });
            const ownReservations = incoming ? [] : (await write.readInventoryOrderReservations3({ id: line.id })).results;
            const ownHeld = ownReservations.reduce((sum, r) => sum + Number(r.quantity_milli) - Number(r.consumed_milli) - Number(r.released_milli), 0);
            if (!incoming && Number(stock?.quantity_milli ?? 0) - Number(stock?.reserved_milli ?? 0) + ownHeld < qty)
                throw ports.error(409, "INSUFFICIENT_STOCK", "There is not enough stock in this warehouse. Receive the purchase first.");
            const previous = await write.readCommerceFulfillmentLines2({ id: line.id });
            const amounts = qty === remaining
                ? {
                    subtotal: Number(line.subtotal_minor) - Number(previous?.subtotal ?? 0),
                    tax: Number(line.tax_minor) - Number(previous?.tax ?? 0),
                }
                : lineAmounts(qty, Number(line.price_minor), Number(line.tax_bps));
            const stockSubtotal = incoming ? convertedStock(documentSubtotal + amounts.subtotal) - convertedStock(documentSubtotal) : 0;
            documentSubtotal += amounts.subtotal;
            const tracking = String(line.tracking_mode ?? 'none'), allocations = entry.allocations ?? [];
            if (tracking === 'none' && allocations.length)
                throw ports.error(422, 'LOT_ALLOCATION_UNEXPECTED', 'This product does not use lot or serial tracking.');
            if (tracking !== 'none' && allocations.reduce((sum, a) => sum + quantityMilli(a.quantity), 0) !== qty)
                throw ports.error(422, 'LOT_ALLOCATION_REQUIRED', 'Allocate the full movement quantity to lots or serial numbers.');
            if (tracking === 'serial' && allocations.some(a => quantityMilli(a.quantity) !== 1000))
                throw ports.error(422, 'SERIAL_QUANTITY_INVALID', 'Each serial allocation must be exactly one unit.');
            const lotPrepared: Array<{
                lotId: string;
                quantityMilli: number;
                valueMinor: number;
                disposition: 'available' | 'quarantine';
                reason: string;
                dueDate: string;
                existing: Row | null;
                lotCode: string | null;
                serialNumber: string | null;
                manufacturedDate: string | null;
                expiryDate: string | null;
            }> = [];
            let cost = tracking === 'none' ? (incoming ? stockSubtotal : qty === Number(stock?.quantity_milli) ? Number(stock?.value_minor) : scaledAmount(Number(stock?.value_minor ?? 0), qty, Number(stock?.quantity_milli ?? 1))) : 0;
            if (tracking !== 'none') {
                const identities = new Set<string>();
                let allocatedValue = 0;
                for (const [allocationIndex, allocation] of allocations.entries()) {
                    const allocationQty = quantityMilli(allocation.quantity), identity = incoming ? (tracking === 'serial' ? allocation.serialNumber : allocation.lotCode) : allocation.lotId;
                    if (!identity || identities.has(identity))
                        throw ports.error(422, 'LOT_ALLOCATION_DUPLICATE', 'Use each lot or serial once per movement line.');
                    identities.add(identity);
                    let existing: Row | null;
                    if (incoming)
                        existing = await write.readInventoryLots1({ warehouse_id: order.warehouse_id, companyId: actor.companyId, product_id: line.product_id, identity: identity, tracking: tracking }) ?? null;
                    else
                        existing = await write.readInventoryQualityHolds3({ fact0: allocation.lotId ?? '', companyId: actor.companyId, product_id: line.product_id, warehouse_id: order.warehouse_id }) ?? null;
                    if (!incoming && (!existing || Number(existing.quantity_milli) < allocationQty || Number(existing.quality_held)))
                        throw ports.error(409, 'LOT_UNAVAILABLE', 'The selected lot or serial is unavailable, quarantined, or has insufficient quantity.');
                    if (incoming && tracking === 'serial' && existing)
                        throw ports.error(409, 'SERIAL_ALREADY_RECEIVED', 'This serial number already exists for the product.');
                    if (incoming && allocation.disposition === 'quarantine' && (!allocation.quarantineReason || !allocation.quarantineDueDate))
                        throw ports.error(422, 'QUARANTINE_EVIDENCE_REQUIRED', 'Quarantined receipt allocations require a reason and review due date.');
                    const lotId = String(existing?.id ?? `lot-${(await ports.hash(`${actor.companyId}:${line.product_id}:${tracking}:${identity}`)).slice(0, 48)}`), allocationValue = incoming ? (allocationIndex === allocations.length - 1 ? stockSubtotal - allocatedValue : scaledAmount(stockSubtotal, allocationQty, qty)) : (allocationQty === Number(existing!.quantity_milli) ? Number(existing!.value_minor) : scaledAmount(Number(existing!.value_minor), allocationQty, Number(existing!.quantity_milli)));
                    allocatedValue += allocationValue;
                    cost += incoming ? 0 : allocationValue;
                    lotPrepared.push({ lotId, quantityMilli: allocationQty, valueMinor: allocationValue, disposition: allocation.disposition, reason: allocation.quarantineReason, dueDate: allocation.quarantineDueDate, existing, lotCode: tracking === 'lot' ? String(allocation.lotCode) : null, serialNumber: tracking === 'serial' ? String(allocation.serialNumber) : null, manufacturedDate: allocation.manufacturedDate || String(existing?.manufactured_date ?? '') || null, expiryDate: allocation.expiryDate || String(existing?.expiry_date ?? '') || null });
                }
                if (incoming)
                    cost = allocatedValue;
            }
            value += cost;
            steps.push(write.guardRule24({ key: key, guardId: `movement-date-${i}`, values: [order.warehouse_id, line.product_id, input.date] }));
            steps.push(write.guardRule25({ key: key, guardId: `line-${i}`, values: [line.id, line.fulfilled_milli, qty] }));
            if (stock)
                steps.push(write.guardRule26({ key: key, guardId: `stock-${i}`, values: [order.warehouse_id, line.product_id, stock.version] }));
            else
                steps.push(write.prepareCommerceStock2({ warehouse_id: order.warehouse_id, product_id: line.product_id }));
            let consumeRemaining = qty;
            for (const r of ownReservations) {
                const n = Math.min(consumeRemaining, Number(r.quantity_milli) - Number(r.consumed_milli) - Number(r.released_milli));
                if (!n)
                    break;
                steps.push(write.prepareInventoryOrderReservations4({ n: n, id: r.id }));
                consumeRemaining -= n;
            }
            steps.push(write.prepareCommerceStock3({ fact0: incoming ? qty : -qty, fact1: incoming ? cost : -cost, warehouse_id: order.warehouse_id, product_id: line.product_id }));
            for (const [amountKind, amountUnits] of [['stock_value', cost], ['subtotal', amounts.subtotal], ['tax', amounts.tax], ['total', amounts.subtotal + amounts.tax]] as const)
                steps.push(await commerceMoney(write, actor, { sourceType: 'commerce_fulfillment_line', sourceId: `${fulfillment}-${i}`, amountKind, transactionDate: input.date, currency: amountKind === 'stock_value' ? stockCurrency : String(order.currency), amountUnits }));
            steps.push(write.prepareCommerceFulfillmentLines1({ fact0: `${fulfillment}-${i}`, fulfillment: fulfillment, id: line.id, qty: qty, cost: cost, subtotal: amounts.subtotal, tax: amounts.tax, stockCurrency: stockCurrency }));
            for (const [allocationIndex, allocation] of lotPrepared.entries()) {
                const allocationId = `${fulfillment}-${i}-${allocationIndex}`, lineId = `${fulfillment}-${i}`;
                if (incoming) {
                    if (!allocation.existing)
                        steps.push(write.prepareInventoryLots3({ lotId: allocation.lotId, companyId: actor.companyId, product_id: line.product_id, tracking: tracking, lotCode: allocation.lotCode, serialNumber: allocation.serialNumber, manufacturedDate: allocation.manufacturedDate, expiryDate: allocation.expiryDate, userId: actor.userId, fact9: ports.clock.now() }));
                    else
                        steps.push(write.guardRule27({ key: key, guardId: `lot-${i}-${allocationIndex}`, values: [allocation.lotId, actor.companyId, line.product_id, tracking, allocation.lotCode, allocation.serialNumber, allocation.manufacturedDate, allocation.expiryDate] }), write.prepareInventoryLots4({ lotId: allocation.lotId }));
                    if (allocation.existing?.balance_version)
                        steps.push(write.guardRule28({ key: key, guardId: `lot-balance-${i}-${allocationIndex}`, values: [allocation.lotId, order.warehouse_id, allocation.existing.balance_version] }));
                    steps.push(write.prepareInventoryLotBalances3({ lotId: allocation.lotId, warehouse_id: order.warehouse_id, quantityMilli: allocation.quantityMilli, valueMinor: allocation.valueMinor }));
                }
                else {
                    steps.push(write.guardRule29({ key: key, guardId: `lot-balance-${i}-${allocationIndex}`, values: [allocation.lotId, order.warehouse_id, allocation.existing!.balance_version, allocation.quantityMilli] }), write.prepareInventoryLotBalances4({ quantityMilli: allocation.quantityMilli, valueMinor: allocation.valueMinor, lotId: allocation.lotId, warehouse_id: order.warehouse_id }), write.prepareInventoryLots5({ lotId: allocation.lotId, lotId2: allocation.lotId }));
                }
                const qualityHoldId = incoming && allocation.disposition === 'quarantine' ? `quality-${key}-${i}-${allocationIndex}` : null;
                if (qualityHoldId)
                    steps.push(...await write.receiptQualityHoldSteps(actor, { id: qualityHoldId, key: `${key}-${i}-${allocationIndex}`, branchId: String(order.branch_id), warehouseId: String(order.warehouse_id), productId: String(line.product_id), lotId: allocation.lotId, quantityMilli: allocation.quantityMilli, reason: allocation.reason, dueDate: allocation.dueDate, sourceId: allocationId }));
                steps.push(write.prepareInventoryLotMoves2({ fact0: `lot-move-${allocationId}`, lotId: allocation.lotId, warehouse_id: order.warehouse_id, fact3: incoming ? 'goods_receipt' : 'delivery', lineId: lineId, fact5: incoming ? allocation.quantityMilli : -allocation.quantityMilli, fact6: incoming ? allocation.valueMinor : -allocation.valueMinor, date: input.date, userId: actor.userId, fact9: ports.clock.now() }), write.prepareInventoryFulfillmentLotAllocations1({ fact0: `lot-allocation-${allocationId}`, lineId: lineId, lotId: allocation.lotId, quantityMilli: allocation.quantityMilli, valueMinor: allocation.valueMinor, disposition: allocation.disposition, qualityHoldId: qualityHoldId }));
            }
            steps.push(write.prepareCommerceOrderLines4({ qty: qty, id: line.id }));
            steps.push(write.prepareCommerceStockMoves1({ fact0: `${fulfillment}-${i}`, warehouse_id: order.warehouse_id, product_id: line.product_id, fulfillment: fulfillment, fact4: incoming ? qty : -qty, fact5: incoming ? cost : -cost, fact6: ports.clock.now() }));
        }
        if (goodsCount) {
        const goods = await write.prepareJournal(actor, { chartId: String(config.chart_id), periodId, entryDate: input.date,
            currency: incoming ? String(order.currency) : stockCurrency, fxRate: minorToDecimal(valuationFx.micros, 6),
            sourceType: incoming ? 'goods_receipt' : 'delivery', sourceId: fulfillment, memo: `${incoming ? 'Goods receipt' : 'Delivery'} ${fulfillment}`, isOpening: false,
            lines: [{ accountId: String(incoming ? config.inventory_account_id : config.cogs_account_id), debit: minorToDecimal(incoming ? documentSubtotal : value), credit: '0' },
                { accountId: String(incoming ? config.clearing_account_id : config.inventory_account_id), debit: '0', credit: minorToDecimal(incoming ? documentSubtotal : value) }] }, key, 'draft');
        if (goods.preparedLines.reduce((sum, line) => sum + line.baseDebit, 0) !== value)
            throw ports.error(409, 'GOODS_VALUATION_RECONCILIATION_REQUIRED', 'The frozen functional stock value must match the exact goods journal.');
        steps.push(...goods.steps, write.prepareCommerceGoodsValuations1({ fulfillment: fulfillment, companyId: actor.companyId, chart_id: config.chart_id, journal: journal, fact4: String(order.currency), fact5: incoming ? String(order.currency) : stockCurrency, stockCurrency: stockCurrency, date: input.date, micros: valuationFx.micros, sourceRateId: valuationFx.sourceRateId, targetRateId: valuationFx.targetRateId, usedLatestFallback: valuationFx.usedLatestFallback, documentSubtotal: documentSubtotal, value: value, userId: actor.userId, fact15: ports.clock.now() }));
        steps.push(write.prepareCommerceFulfillments3({ journal: journal, fulfillment: fulfillment }));
        }
        if (!incoming)
            for (const attachmentId of input.evidenceIds)
                steps.push(write.prepareCommerceFulfillmentEvidence1({ fulfillment: fulfillment, attachmentId: attachmentId, userId: actor.userId, fact3: ports.clock.now() }));
        if (attachmentBinding)
            steps.push(...attachmentBinding.steps);
        steps.push(write.prepareCommerceOrders6({ id: id, fact1: ports.clock.now(), id2: id }));
        return write.executeCommand(actor, key, "commerce.order.fulfill", input, id, input.expectedVersion + 1, { id, fulfillmentId: fulfillment, version: input.expectedVersion + 1 }, steps);
    }
    async function invoiceFulfillment(actor: WorkContext, fulfillmentId: string, input: z.infer<typeof invoiceFromDeliverySchema>, key: string) {
        actor = allowed(actor, "finance");
        const write = await ports.forActor(actor);
        const previous = await write.replayCommand<{ id:string; invoiceId:string }>(actor, key, "commerce.invoice.create", fulfillmentId, input);
        if (previous) return previous;
        const f = await write.readCommerceFulfillments5({ fulfillmentId });
        if (!f) throw ports.error(404, "DELIVERY_NOT_FOUND", "Delivery or service completion not found.");
        const order = await orderRow(write, actor, String(f.order_id));
        const payable = order.kind === "purchase";
        const sources = [{ row: f, version: input.expectedVersion }];
        const ids = new Set([fulfillmentId]);
        for (const additional of input.additionalFulfillments ?? []) {
            if (payable || ids.has(additional.id)) throw ports.error(422, "INVOICE_SOURCES_INVALID", "Combine distinct sales completions from one order.");
            ids.add(additional.id);
            const row = await write.readCommerceFulfillments5({ fulfillmentId: additional.id });
            if (!row || row.order_id !== order.id) throw ports.error(422, "INVOICE_SOURCES_INVALID", "All completions must belong to this sales order.");
            sources.push({ row, version: additional.expectedVersion });
        }
        let totals = { subtotal: 0, tax: 0 };
        for (const source of sources) {
            if (Number(source.row.version) !== source.version || source.row.invoice_id)
                throw ports.error(409, "VERSION_CONFLICT", "A completion changed or is already invoiced. Refresh the order.");
            if (source.row.completion_kind !== "service" && !await write.readFinanceJournals1({ journal_id: source.row.journal_id, companyId: actor.companyId }))
                throw ports.error(409, "STOCK_GL_REQUIRED", "An accountant must review and post the goods movement GL before invoicing.");
            if (input.dueDate < input.date || input.date < String(source.row.fulfillment_date))
                throw ports.error(422, "INVALID_DATES", "Invoice and due dates must follow every completion date.");
            const amounts = await write.readCommerceFulfillmentLines3({ fulfillmentId: String(source.row.id) });
            if (!amounts || !amounts.subtotal) throw ports.error(422, "EMPTY_DELIVERY", "There are no billable lines.");
            totals.subtotal += Number(amounts.subtotal); totals.tax += Number(amounts.tax);
        }
        const config = await settings(write, actor), invoice = `invoice-${key}`;
        if (input.useSalesDefaults && !config.approved_sales_defaults_id) throw ports.error(409, "SALES_DEFAULTS_REQUIRED", "Approve valid Company sales defaults before preparing this invoice.");
        const matching = payable ? await write.supplierBillInvoiceHandoff(actor, f, input, invoice, key) : null;
        if (matching) totals = { subtotal: Number(matching.bill.subtotal_minor), tax: Number(matching.bill.tax_minor) };
        const total = totals.subtotal + totals.tax, invoiceCurrency = String(matching?.bill.currency ?? order.currency);
        if (!Number.isSafeInteger(total)) throw ports.error(422, "MONEY_OVERFLOW", "Invoice exceeds supported precision.");
        const steps = [
            await commerceMoney(write, actor, { sourceType: "commerce_invoice_source", sourceId: invoice, amountKind: "total", transactionDate: input.date, currency: invoiceCurrency, amountUnits: total }),
            ...sources.map(source => write.guardRule30({ key, guardId: `completion-${source.row.id}`, values: [String(source.row.id), source.version] })),
            write.prepareFinanceInvoices4({ invoice, companyId: actor.companyId, party_id: order.party_id, fact3: payable ? "payable" : "receivable", fact4: `${payable ? "BILL" : "INV"}-${(await ports.hash(key)).slice(0, 24).toUpperCase()}`, date: input.date, dueDate: input.dueDate, invoiceCurrency, subtotal: totals.subtotal, tax: totals.tax, total, total2: total, fact12: payable ? matching?.bill.clearing_account_id ?? config.clearing_account_id : config.receivable_account_id, fact13: payable ? matching?.bill.payable_account_id ?? config.payable_account_id : config.revenue_account_id, fact14: payable ? matching?.bill.tax_account_id ?? config.input_tax_account_id : config.output_tax_account_id, fulfillmentId, userId: actor.userId, fact17: ports.clock.now(), fact18: ports.clock.now() }),
            ...(matching?.steps ?? sources.map(source => write.prepareCommerceFulfillments4({ invoice, fulfillmentId: String(source.row.id) }))),
        ];
        return write.executeCommand(actor, key, "commerce.invoice.create", input, fulfillmentId, input.expectedVersion + 1, { id: String(order.id), invoiceId: invoice }, steps);
    }
    async function settleInvoice(actor: WorkContext, invoiceId: string, raw: z.input<typeof settlementSchema>, key: string) {
        const input = { ...raw, attachmentSetIds: raw.attachmentSetIds ?? [] };
        actor = allowed(actor, "finance");
        const write = await ports.forActor(actor);
        const previous = await write.replayCommand<{ id:string; paymentId:string; outstandingMinor:number }>(actor, key, "commerce.invoice.settle", invoiceId, input);
        if (previous)
            return previous;
        const invoice = await write.readFinanceInvoices1({ invoiceId: invoiceId, companyId: actor.companyId });
        if (!invoice)
            throw ports.error(404, "INVOICE_NOT_FOUND", "Connected workflow invoice not found.");
        const fulfillment = invoice.source_type === "commerce_fulfillment"
            ? await write.readCommerceFulfillments6({ source_id: invoice.source_id })
            : null;
        if (fulfillment)
            await orderRow(write, actor, fulfillment.order_id);
        else if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_SCOPE_REQUIRED", "Standalone invoices require a holding-level finance assignment.");
        if (invoice.kind === "payable" &&
            (!hasPermission(actor, "finance.pay") ||
                samePerson(actor, invoice.created_by)))
            throw ports.error(403, "PAYMENT_APPROVER_REQUIRED", "A different finance approver or treasury operator must record the supplier payment.");
        const expense = invoice.source_type === "expense_claim"
            ? await write.readFinanceExpenses1({ source_id: invoice.source_id, invoiceId: invoiceId, companyId: actor.companyId })
            : null;
        if (invoice.source_type === "expense_claim" &&
            (!expense ||
                [
                    expense.created_by,
                    expense.prepared_by,
                    expense.claimant_user_id,
                ].includes(actor.userId)))
            throw ports.error(403, "SELF_REIMBURSEMENT_FORBIDDEN", "A different finance approver or treasury operator must record reimbursement.");
        if (expense?.workflow_version === 2)
            throw ports.error(409, "EXPENSE_PAYMENT_REQUIRED", "Record payment from the expense claim and attach its bank slip.");
        if (invoice.kind === "payable" && invoice.source_type !== "expense_claim")
            await write.assertControlledThaiTaxPaymentOwner(actor, input.date);
        const supplierReservation = await write.readFinanceSupplierInstructions1({ invoiceId: invoiceId });
        if (supplierReservation)
            throw ports.error(409, "CONFLICT", "The invoice is already reserved by a supplier payment workflow. Refresh and review it again.");
        let paymentSetIds = [...input.attachmentSetIds];
        if (invoice.kind === "payable" && !paymentSetIds.length && input.paymentSlipId) {
            const compatibilitySet = await write.readAttachmentSets1({ companyId: actor.companyId, invoiceId: invoiceId, expectedVersion: input.expectedVersion, paymentSlipId: input.paymentSlipId });
            if (compatibilitySet)
                paymentSetIds = [compatibilitySet.id];
        }
        const paymentBinding = invoice.kind === "payable" && paymentSetIds.length
            ? await write.prepareAttachmentBinding(actor, {
                attachmentSetIds: paymentSetIds,
                bindingContract: { targetType: "invoice_payment", bindingType: "invoice_payment_settlement", contexts: [{ purposeKey: "payment_slip", componentKind: "action", componentRef: "legacy-upload-adapter" }] },
                targetId: invoiceId,
                expectedTargetVersion: input.expectedVersion,
                commandId: key,
                bindingId: `invoice-payment-${key}`,
            })
            : null;
        if (invoice.kind === "payable" && !paymentBinding && !input.paymentSlipId)
            throw ports.error(422, "PAYMENT_SLIP_REQUIRED", "Upload the bank payment slip before recording payment.");
        if (invoice.kind === "payable" && !paymentBinding && ports.attachmentReadable())
            throw ports.error(422, "PAYMENT_SLIP_REQUIRED", "Choose a clean payment slip prepared for this exact invoice version.");
        const boundPaymentSlipId = paymentBinding?.manifest[0]?.sourceId ?? input.paymentSlipId;
        const prepared = await prepareInvoiceSettlement(write, actor, invoice, expense, { ...input, paymentSlipId: boundPaymentSlipId }, key);
        if (invoice.kind === "payable") {
            prepared.steps.unshift(...(paymentBinding?.guards ?? []), ...((paymentBinding?.manifest ?? []).map((file, index) => write.guardRule31({ key: key, guardId: `unique-payment-slip-${index}`, values: [file.sourceId, actor.companyId] }))), ...(!paymentBinding ? [write.guardRule32({ key: key, guardId: "payment-slip", values: [boundPaymentSlipId ?? null, invoiceId] }), write.guardRule33({ key: key, guardId: "unique-payment-slip-legacy", values: [boundPaymentSlipId ?? null, actor.companyId] })] : []));
            prepared.steps.push(...(paymentBinding?.steps ?? []));
        }
        return write.executeCommand(actor, key, "commerce.invoice.settle", input, invoiceId, input.expectedVersion + 1, { ...prepared.result, id: fulfillment?.order_id ?? invoiceId }, prepared.steps);
    }
    async function prepareInvoiceSettlement(write: CommerceUnit<Effect>, actor: WorkContext, invoice: Row, expense: Row | null, input: z.infer<typeof settlementSchema>, key: string) {
        const invoiceId = String(invoice.id);
        if (input.date < String(invoice.invoice_date))
            throw ports.error(422, "INVALID_PAYMENT_DATE", "Payment date cannot precede the invoice.");
        const amountMinor = decimalToMinor(input.amount);
        if (amountMinor <= 0 || amountMinor > Number(invoice.outstanding_minor))
            throw ports.error(422, "PAYMENT_EXCEEDS_BALANCE", "Amount must be positive and no greater than the outstanding balance.");
        const config = await settings(write, actor), periodId = await period(write, actor, input.date, input.periodId);
        if (input.bankAccountId) {
            const bank = await write.readFinanceAccounts2({ bankAccountId: input.bankAccountId, companyId: actor.companyId, chart_id: config.chart_id, currency: invoice.currency });
            if (!bank)
                throw ports.error(422, "BANK_ACCOUNT_INVALID", "Choose an active bank account from the invoice's primary book and currency.");
        }
        const bankAccountId = input.bankAccountId ?? String(config.cash_account_id);
        const payment = `payment-${key}`, journal = `journal-${key}`;
        const payable = invoice.kind === "payable";
        if (invoice.currency !== config.currency)
            throw ports.error(409, "CURRENCY_MAPPING_REQUIRED", "Configure settlement accounts for this invoice currency.");
        const lines = [
            {
                account: String(payable ? invoice.credit_account_id : bankAccountId),
                debit: amountMinor,
                credit: 0,
            },
            {
                account: String(payable ? bankAccountId : invoice.debit_account_id),
                debit: 0,
                credit: amountMinor,
            },
        ];
        const steps = [
            ...(input.bankAccountId ? [write.guardRule34({ key: key, guardId: "settlement-bank", values: [bankAccountId, actor.companyId, config.chart_id, invoice.currency] })] : []),
            await commerceMoney(write, actor, { sourceType: 'finance_payment', sourceId: payment, amountKind: 'settled', transactionDate: input.date, currency: String(invoice.currency), amountUnits: amountMinor }),
            await commerceMoney(write, actor, { sourceType: 'finance_journal', sourceId: journal, amountKind: 'debit_total', transactionDate: input.date, currency: String(invoice.currency), amountUnits: amountMinor }),
            write.guardRule35({ key: key, guardId: "settlement-functional-currency", values: [config.chart_id, actor.companyId, invoice.currency, invoiceId] }),
            write.guardRule36({ key: key, guardId: "supplier-reservation", values: [invoiceId] }),
            versionGuard(write, key, "finance_invoices", invoiceId, input.expectedVersion, ["posted", "partially_paid"]),
            write.guardRule37({ key: key, guardId: "balance", values: [invoiceId, amountMinor] }),
            ...journalSteps(write, actor, key, journal, config, periodId, input.date, "commerce_payment", payment, lines),
            write.prepareFinancePayments1({ payment: payment, companyId: actor.companyId, party_id: invoice.party_id, fact3: payable ? "payment" : "receipt", reference: input.reference, date: input.date, currency: config.currency, amountMinor: amountMinor, amountMinor2: amountMinor, bankAccountId: bankAccountId, journal: journal, userId: actor.userId, fact12: payable ? actor.userId : null, fact13: ports.clock.now(), fact14: ports.clock.now(), fact15: input.paymentSlipId ?? null }),
            write.prepareFinanceAllocations1({ fact0: `allocation-${key}`, companyId: actor.companyId, payment: payment, invoiceId: invoiceId, amountMinor: amountMinor, amountMinor2: amountMinor, amountMinor3: amountMinor, userId: actor.userId, fact8: ports.clock.now() }),
            write.prepareFinanceInvoices5({ amountMinor: amountMinor, amountMinor2: amountMinor, amountMinor3: amountMinor, fact3: ports.clock.now(), invoiceId: invoiceId }),
        ];
        if (expense)
            steps.push(write.guardRule38({ key: key, guardId: "expense-claimant", values: [String(expense.employee_id), actor.userId] }), versionGuard(write, key, "finance_expenses", String(expense.id), Number(expense.version), ["posted"]), write.guardRule39({ key: key, guardId: "no-export", values: [String(expense.id)] }), write.prepareFinanceExpenses1({ fact0: Number(invoice.outstanding_minor) - amountMinor, fact1: Number(invoice.outstanding_minor) - amountMinor, fact2: Number(invoice.outstanding_minor) - amountMinor, fact3: ports.clock.now(), fact4: ports.clock.now(), id: expense.id }));
        return {
            result: {
                id: invoiceId,
                paymentId: payment,
                outstandingMinor: Number(invoice.outstanding_minor) - amountMinor,
            },
            steps,
        };
    }
    async function saveMaster(actor: WorkContext, kind: "products" | "parties" | "warehouses", id: string | null, input: unknown, key: string) {
        const write = await ports.forActor(actor);
        const config = await settings(write, actor);
        const record = id ?? `${kind}-${key}`;
        const previous = await write.replayCommand<{ id:string; version:number }>(actor, key, `commerce.${kind}.${id ? "update" : "create"}`, record, input);
        if (previous)
            return previous;
        const steps: Effect[] = [];
        let version = 1;
        if (kind === "products") {
            actor = allowed(actor, "stock");
            const v = productSchema.parse(input);
            version = id ? (v.expectedVersion ?? 0) + 1 : 1;
            steps.push(await commerceMoney(write, actor, { sourceType: 'commerce_product_revision', sourceId: `${record}:${version}`, amountKind: 'unit_price', transactionDate: new Date(ports.clock.now()).toISOString().slice(0, 10), currency: String(config.currency), amountUnits: decimalToMinor(v.price) }));
            if (id)
                steps.push(write.guardRule40({ key: key, guardId: "master", values: [id, actor.companyId, v.expectedVersion ?? 0] }));
            steps.push(id
                ? write.prepareCommerceProducts1({ sku: v.sku, name: v.name, unit: v.unit, trackingMode: v.trackingMode, itemType: v.itemType, fact4: decimalToMinor(v.price), taxBps: v.taxBps, status: v.status, id: id })
                : write.prepareCommerceProducts2({ record: record, companyId: actor.companyId, sku: v.sku, name: v.name, unit: v.unit, trackingMode: v.trackingMode, itemType: v.itemType, fact6: decimalToMinor(v.price), taxBps: v.taxBps, status: v.status }));
        }
        else if (kind === "parties") {
            const v = partyMasterSchema.parse(input);
            actor = allowed(actor, v.kind === "supplier" ? "purchase" : "sales");
            version = id ? (v.expectedVersion ?? 0) + 1 : 1;
            steps.push(await commerceMoney(write, actor, { sourceType: 'commerce_party_revision', sourceId: `${record}:${version}`, amountKind: 'credit_limit', transactionDate: new Date(ports.clock.now()).toISOString().slice(0, 10), currency: String(config.currency), amountUnits: decimalToMinor(v.creditLimit) }));
            if (id)
                steps.push(write.guardRule41({ key: key, guardId: "master", values: [id, actor.companyId, v.kind, v.expectedVersion ?? 0] }));
            steps.push(id
                ? write.prepareFinanceParties2({ code: v.code, name: v.name, paymentTermsDays: v.paymentTermsDays, fact3: decimalToMinor(v.creditLimit), status: v.status, fact5: v.kind === 'customer' ? (v.notificationEmail || null) : null, fact6: v.kind === 'customer' ? v.notificationConsent : 'unknown', fact7: ports.clock.now(), id: id })
                : write.prepareFinanceParties3({ record: record, companyId: actor.companyId, kind: v.kind, code: v.code, name: v.name, currency: config.currency, paymentTermsDays: v.paymentTermsDays, fact7: decimalToMinor(v.creditLimit), status: v.status, fact9: v.kind === 'customer' ? (v.notificationEmail || null) : null, fact10: v.kind === 'customer' ? v.notificationConsent : 'unknown', fact11: ports.clock.now(), fact12: ports.clock.now() }));
        }
        else {
            actor = allowed(actor, "stock");
            const v = warehouseSchema.parse(input);
            version = id ? (v.expectedVersion ?? 0) + 1 : 1;
            if (actor.branchId && v.branchId !== actor.branchId)
                throw ports.error(403, "SCOPE_FORBIDDEN", "Choose your assigned branch.");
            steps.push(write.guardRule42({ key: key, guardId: "branch", values: [v.branchId, actor.companyId] }));
            if (id) {
                steps.push(write.guardRule43({ key: key, guardId: "master", values: [id, actor.companyId, v.branchId, v.expectedVersion ?? 0] }));
                if (v.status === "archived")
                    steps.push(write.guardRule44({ key: key, guardId: "stock-empty", values: [id] }));
            }
            steps.push(id
                ? write.prepareCommerceWarehouses1({ code: v.code, name: v.name, status: v.status, id: id })
                : write.prepareCommerceWarehouses2({ record: record, companyId: actor.companyId, branchId: v.branchId, code: v.code, name: v.name, status: v.status }));
        }
        return write.executeCommand(actor, key, `commerce.${kind}.${id ? "update" : "create"}`, input, record, version, { id: record, version }, steps);
    }
    async function commercePage(actor: WorkContext, query: {
        view: string;
        kind?: string;
        q?: string;
        cursor?: string;
        movementCursor?: string;
        new?: string;
        edit?: string;
        request?: string;
        status?: string;
        startDate?: string;
        endDate?: string;
    }) {
        actor = allowed(actor, 'read');
        const write = await ports.forActor(actor);
        const list = ['sales', 'quote', 'purchase'].includes(query.view) ? 'orders' : query.view;
        const [{ page, movementPage }, config, options] = await Promise.all([write.pages(actor, query), settings(write, actor), query.new === '1' || query.edit || query.request ? commerceOverview(actor) : null]);
        const data: {
            orders: Row[];
            products: Row[];
            parties: Row[];
            warehouses: Row[];
            stock: Row[];
            moves: Row[];
            nodes: Row[];
            currency: string;
        } = { orders: [], products: [], parties: [], warehouses: [], stock: [], moves: movementPage?.items ?? [], nodes: [], currency: String(query.view === 'stock' ? config.base_currency : config.currency) };
        data[list as 'orders' | 'products' | 'parties' | 'warehouses' | 'stock'] = page.items;
        const inTransit = query.view === 'stock' ? (await write.readInventoryInTransit1({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, branchId3: actor.branchId })).results : [];
        return { ...data, page, movementPage, options, inTransit };
    }
    return { commerceOverview, workOrderRows, getOrder, changeOrderReservation, requestCommerceReturn, decideCommerceReturn, completeCommerceReturn, saveOrder, transitionOrder, fulfillOrder, invoiceFulfillment, settleInvoice, prepareInvoiceSettlement, saveMaster, commercePage };
}
