import{commercePages,commerceSalesDocuments}from '@erpx/server-commerce/adapters/commerce-pages';
export{commercePages,commerceSalesDocuments};

import{getOrder,nativeCommerceOrderReadRepository}from '@erpx/server-commerce/adapters/commerce-order-read';
export{getOrder};
import { commerceOrderReadKinds, requireCommercePurchasedRead, withCommercePurchasedCommand } from "@erpx/server-commerce/adapters/purchase-boundary";
import {ensureCustomerReceipt} from '@erpx/server-finance/public/customer-receipt-issue';

import { commerceFinalOwnerProjectionSources as procurementProjectionSources } from "@erpx/server-procurement/public/commerce-final-owner-projection-sources";
import { commerceFinalOwnerProjectionSources as recordsProjectionSources } from "@erpx/server-records/public/commerce-final-owner-projection-sources";
import {commerceCommerceStockEffects} from '@erpx/server-inventory/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";

import { commerceExpensePaymentEffects } from "@erpx/server-people_payroll/public/commerce-expense-payment-effects";
import { commerceFinancePageSources } from "@erpx/server-finance/public/commerce-page-sources";

import { commerceFinanceJoinedFacts } from "@erpx/server-finance/public/commerce-joined-facts";
import { commerceInventoryJoinedFacts } from "@erpx/server-inventory/public/commerce-joined-facts";
import { refundNotifications } from "@erpx/server-finance/public/refund-notifications";

import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { z } from "zod";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { attachmentsV2Readable } from "@erpx/server-files/public/attachments/feature";
import {snapshotMoney,snapshotMoneyReversal} from '@erpx/server-finance/public/company-money';
import { prepareBusinessEvidence } from "@erpx/server-records/public/business-evidence";
import { prepareReturnCredit, returnCompletionValues } from "@erpx/server-commerce/public/commerce-return-accounting";
import {confirmedFunctionalFx,prepareJournal} from '@erpx/server-finance/public/journal-planning';
import { prepareReturnGoodsFx } from "@erpx/server-commerce/public/commerce-goods-accounting";
import { assertControlledThaiTaxPaymentOwner } from "@erpx/server-finance/public/finance-withholding";
import { createCommerceApplication, type CommerceUnit, type Row } from "@erpx/server-commerce/application/commerce";
import { orderSchema, transitionSchema, fulfillmentSchema, invoiceFromDeliverySchema, settlementSchema, returnRequestSchema, returnDecisionSchema, returnCompleteSchema } from "@erpx/server-commerce/domain/commerce";
import type { Posting } from "@erpx/server-commerce/application/commerce";
export type { Row } from "@erpx/server-commerce/application/commerce";
export { orderSchema, transitionSchema, fulfillmentSchema, invoiceFromDeliverySchema, settlementSchema, returnRequestSchema, returnDecisionSchema, returnCompleteSchema, productSchema, partyMasterSchema, warehouseSchema, stockReservationSchema, commercePermissions } from "@erpx/server-commerce/domain/commerce";
import { receiptQualityHoldSteps } from "@erpx/server-quality/public/quality";
import {getEmployeeRequest} from '@erpx/server-records/public/employee-requests-read';
import {prepareRequestOrderHandoff} from '@erpx/server-records/public/employee-requests';
import { resolveSalesPricing } from "@erpx/server-sales/public/sales-pricing";
import { prepareRfqOrderHandoff } from "@erpx/server-procurement/public/procurement-rfqs";
import { supplierBillInvoiceHandoff } from "@erpx/server-procurement/public/supplier-bill-matching";
import { CommerceFinanceFacts } from "@erpx/server-finance/public/commerce-facts";
import { CommerceInventoryFacts } from "@erpx/server-inventory/public/commerce-facts";
import { CommerceOrganizationFacts } from "@erpx/server-organization/public/commerce-facts";
import { CommerceSalesFacts } from "@erpx/server-sales/public/commerce-facts";
import { CommerceFilesFacts } from "@erpx/server-files/public/commerce-facts";
import { CommerceProcurementFacts } from "@erpx/server-procurement/public/commerce-facts";

/** Sales lists do not require posting setup; preparing documents still does. */

function unit(db: SqlDatabase, actor?:WorkContext): CommerceUnit<SqlStatement> {
    return {
        readCommerceSettings1: facts => commerceFinanceJoinedFacts(db).readCommerceSettings1(facts),
        readFinancePeriods1: facts => CommerceFinanceFacts(db).readFinancePeriods1(facts),
        readCommerceOrders1(...args){return nativeCommerceOrderReadRepository(db).readCommerceOrders1(...args);},
        readCommerceOrders2: async facts => {
            const kinds=actor?await commerceOrderReadKinds(db,actor):null;
            if(kinds===null)return commerceFinanceJoinedFacts(db).readCommerceOrders2(facts);
            return db.prepare(`SELECT o.*,p.name party_name FROM commerce_orders o ${commerceFinancePageSources.orderPartyJoin} WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND ${kinds.length?`o.kind IN (${kinds.map(()=>"?").join(",")})`:"false"} ORDER BY o.created_at DESC LIMIT 250`).bind(...facts.fact0,...kinds).all<Row>();
        },
        readCommerceProducts1: (facts: {
            companyId: string;
        }) => db.prepare("SELECT * FROM commerce_products WHERE company_id=? ORDER BY sku").bind(facts.companyId).all<Row>(),
        readFinanceParties1: facts => CommerceFinanceFacts(db).readFinanceParties1(facts),
        readCommerceWarehouses1: (facts: {
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
        }) => db.prepare("SELECT * FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name").bind(facts.companyId, facts.branchId, facts.branchId2).all<Row>(),
        readInventoryActiveReservations1: facts => CommerceInventoryFacts(db).readInventoryActiveReservations1(facts),
        readInventoryLedger1: facts => CommerceInventoryFacts(db).readInventoryLedger1(facts),
        readOrganizationNodes1: facts => CommerceOrganizationFacts(db).readOrganizationNodes1(facts),
        readSalesPriceLists1: facts => CommerceSalesFacts(db).readSalesPriceLists1(facts),
        readSalesPriceListItems1: facts => CommerceSalesFacts(db).readSalesPriceListItems1(facts),
        readCommerceOrders3: async facts => {
            const rows=await commerceFinanceJoinedFacts(db).readCommerceOrders3(facts);
            if(!actor)return rows;
            const access=await (await import("@erpx/server-commerce/adapters/record-purchases")).commerceWorkAccess(db,actor,rows.results.map(row=>String(row.id)));
            return access===null?rows:{...rows,results:rows.results.filter(row=>access.get(String(row.id))?.history)};
        },
        readInventoryActiveReservations2(...args){return nativeCommerceOrderReadRepository(db).readInventoryActiveReservations2(...args);},
        readCommerceFulfillments1(...args){return nativeCommerceOrderReadRepository(db).readCommerceFulfillments1(...args);},
        readCommerceReturnLines1(...args){return nativeCommerceOrderReadRepository(db).readCommerceReturnLines1(...args);},
        readCommerceReturns1(...args){return nativeCommerceOrderReadRepository(db).readCommerceReturns1(...args);},
        readHrPeople1(...args){return nativeCommerceOrderReadRepository(db).readHrPeople1(...args);},
        readCommerceOrders4(...args){return nativeCommerceOrderReadRepository(db).readCommerceOrders4(...args);},
        readInventoryOrderReservations1(...args){return nativeCommerceOrderReadRepository(db).readInventoryOrderReservations1(...args);},
        readInventoryQualityHolds1(...args){return nativeCommerceOrderReadRepository(db).readInventoryQualityHolds1(...args);},
        readCommerceReturnLotAllocations1(...args){return nativeCommerceOrderReadRepository(db).readCommerceReturnLotAllocations1(...args);},
        readCommerceReturnLotAllocations2(...args){return nativeCommerceOrderReadRepository(db).readCommerceReturnLotAllocations2(...args);},
        readInventoryQualityHolds2(...args){return nativeCommerceOrderReadRepository(db).readInventoryQualityHolds2(...args);},
        readCommerceFulfillmentEvidence1(...args){return nativeCommerceOrderReadRepository(db).readCommerceFulfillmentEvidence1(...args);},
        readAttachments1(...args){return nativeCommerceOrderReadRepository(db).readAttachments1(...args);},
        readCommerceCustomerNotifications1(...args){return nativeCommerceOrderReadRepository(db).readCommerceCustomerNotifications1(...args);},
        readCommerceCustomerNotificationSettings1(...args){return nativeCommerceOrderReadRepository(db).readCommerceCustomerNotificationSettings1(...args);},
        readCommerceOrderLines1: (facts: {
            lineId: string;
            id: string;
        }) => db.prepare('SELECT * FROM commerce_order_lines WHERE id=? AND order_id=?').bind(facts.lineId, facts.id).first<Row>(),
        prepareInventoryOrderReservations1: facts => CommerceInventoryFacts(db).prepareInventoryOrderReservations1(facts),
        readInventoryOrderReservations2: facts => CommerceInventoryFacts(db).readInventoryOrderReservations2(facts),
        prepareInventoryOrderReservations2: facts => CommerceInventoryFacts(db).prepareInventoryOrderReservations2(facts),
        prepareCommerceOrders1: (facts: {
            now: number;
            id: string;
        }) => db.prepare('UPDATE commerce_orders SET version=version+1,updated_at=? WHERE id=?').bind(facts.now, facts.id),
        readCommerceFulfillments2: facts => commerceFinanceJoinedFacts(db).readCommerceFulfillments2(facts),
        readCommerceFulfillmentLines1: (facts: {
            lineId: string;
            fulfillmentId: string;
        }) => db.prepare("SELECT fl.*,ol.product_id,ol.item_type,p.tracking_mode FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.id=? AND fl.fulfillment_id=?").bind(facts.lineId, facts.fulfillmentId).first<Row>(),
        readProcurementBillMatchLines1: facts => CommerceProcurementFacts(db).readProcurementBillMatchLines1(facts),
        readCommerceReturnLines2: (facts: {
            id: string | number | null;
        }) => db.prepare("SELECT COALESCE(SUM(rl.quantity_milli),0) quantity,COALESCE(SUM(rl.value_minor),0) value,COALESCE(SUM(rl.subtotal_minor),0) subtotal,COALESCE(SUM(rl.tax_minor),0) tax,COALESCE(SUM(rl.expected_subtotal_minor),0) expected_subtotal FROM commerce_return_lines rl JOIN commerce_returns r ON r.id=rl.return_id WHERE rl.fulfillment_line_id=? AND r.status!='rejected'").bind(facts.id).first<{
            quantity: number;
            value: number;
            subtotal: number;
            tax: number;
            expected_subtotal: number;
        }>(),
        readInventoryFulfillmentLotAllocations1: facts => CommerceInventoryFacts(db).readInventoryFulfillmentLotAllocations1(facts),
        readCommerceReturnLotAllocations3: (facts: {
            id: string | number | null;
        }) => db.prepare("SELECT COALESCE(SUM(a.quantity_milli),0) quantity,COALESCE(SUM(a.value_minor),0) value FROM commerce_return_lot_allocations a JOIN commerce_return_lines rl ON rl.id=a.return_line_id JOIN commerce_returns r ON r.id=rl.return_id WHERE a.fulfillment_allocation_id=? AND r.status!='rejected'").bind(facts.id).first<{
            quantity: number;
            value: number;
        }>(),
        readInventoryLotBalances1: facts => CommerceInventoryFacts(db).readInventoryLotBalances1(facts),
        prepareCommerceReturns1: (facts: {
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
        }) => db.prepare("INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,subtotal_minor,tax_minor,total_minor,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(facts.id, facts.companyId, facts.branch_id, facts.order_id, facts.fulfillmentId, facts.invoice_id, facts.kind, facts.number, facts.date, facts.reason, facts.subtotal, facts.tax, facts.total, facts.userId, facts.now, facts.now2),
        prepareCommerceReturnLines1: (facts: {
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
        }) => db.prepare("INSERT INTO commerce_return_lines(id,return_id,fulfillment_line_id,product_id,quantity_milli,value_minor,subtotal_minor,tax_minor,ordinal,condition_code,condition_note,quality_due_date,expected_subtotal_minor) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(facts.returnLineId, facts.id, facts.id2, facts.product_id, facts.quantity, facts.returnValue, facts.returnSubtotal, facts.returnTax, facts.index, facts.condition, facts.fact10, facts.fact11, facts.expectedSubtotal),
        prepareCommerceReturnLotAllocations1: (facts: {
            id: string;
            returnLineId: string;
            fulfillmentAllocationId: string;
            lotId: string;
            quantity: number;
            value: number;
            snapshotQuantity: number;
            snapshotValue: number;
            snapshotVersion: number;
        }) => db.prepare("INSERT INTO commerce_return_lot_allocations(id,return_line_id,fulfillment_allocation_id,lot_id,quantity_milli,value_minor,snapshot_quantity_milli,snapshot_value_minor,snapshot_version) VALUES(?,?,?,?,?,?,?,?,?)").bind(facts.id, facts.returnLineId, facts.fulfillmentAllocationId, facts.lotId, facts.quantity, facts.value, facts.snapshotQuantity, facts.snapshotValue, facts.snapshotVersion),
        prepareCommerceFulfillments1: (facts: {
            fulfillmentId: string;
            expectedVersion: number;
        }) => db.prepare('UPDATE commerce_fulfillments SET version=version+1 WHERE id=? AND version=?').bind(facts.fulfillmentId, facts.expectedVersion),
        readCommerceReturns2: facts => commerceFinanceJoinedFacts(db).readCommerceReturns2(facts),
        prepareCommerceReturns2: (facts: {
            userId: string;
            reason: string;
            fact2: number;
            returnId: string;
        }) => db.prepare("UPDATE commerce_returns SET status='rejected',approved_by=?,reason=reason||E'\nRejected: '||?,version=version+1,updated_at=? WHERE id=?").bind(facts.userId, facts.reason, facts.fact2, facts.returnId),
        readCommerceFulfillments3: facts => commerceFinanceJoinedFacts(db).readCommerceFulfillments3(facts),
        readFinanceJournalLines1: facts => CommerceFinanceFacts(db).readFinanceJournalLines1(facts),
        readCommerceReturnLines3: (facts: {
            returnId: string;
        }) => db.prepare("SELECT rl.*,ol.item_type FROM commerce_return_lines rl JOIN commerce_fulfillment_lines fl ON fl.id=rl.fulfillment_line_id JOIN commerce_order_lines ol ON ol.id=fl.order_line_id WHERE rl.return_id=? ORDER BY rl.ordinal").bind(facts.returnId).all<Row>(),
        readProcurementBillMatches1: facts => CommerceProcurementFacts(db).readProcurementBillMatches1(facts),
        prepareCommerceReturns3: (facts: {
            returnId: string;
        }) => db.prepare('SELECT id FROM commerce_returns WHERE id=? FOR UPDATE').bind(facts.returnId),
        prepareFinanceInvoices1: facts => CommerceFinanceFacts(db).prepareFinanceInvoices1(facts),
        readCommerceStock1: (facts: {
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => db.prepare(`SELECT * FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?`).bind(facts.warehouse_id, facts.product_id).first<Row>(),
        prepareCommerceStock1: (facts: {
            fact0: string | number | null;
            fact1: string | number | null;
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => commerceCommerceStockEffects(db).prepareCommerceStock1(facts),
        prepareCommerceReturnStockMoves1: (facts: {
            fact0: string;
            returnId: string;
            warehouse_id: string | number | null;
            product_id: string | number | null;
            fact4: string | number | null;
            fact5: string | number | null;
            return_date: string | number | null;
            now: number;
        }) => db.prepare("INSERT INTO commerce_return_stock_moves(id,return_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(facts.fact0, facts.returnId, facts.warehouse_id, facts.product_id, facts.fact4, facts.fact5, facts.return_date, facts.now),
        readCommerceReturnLotAllocations4: facts => commerceInventoryJoinedFacts(db).readCommerceReturnLotAllocations4(facts),
        prepareInventoryLotBalances1: facts => CommerceInventoryFacts(db).prepareInventoryLotBalances1(facts),
        prepareInventoryLots1: facts => CommerceInventoryFacts(db).prepareInventoryLots1(facts),
        prepareInventoryLotBalances2: facts => CommerceInventoryFacts(db).prepareInventoryLotBalances2(facts),
        prepareInventoryLots2: facts => CommerceInventoryFacts(db).prepareInventoryLots2(facts),
        prepareInventoryLotMoves1: facts => CommerceInventoryFacts(db).prepareInventoryLotMoves1(facts),
        prepareCommerceReturns4: (facts: {
            userId: string;
            stockJournal: string | null;
            creditJournal: string;
            baseCreditMinor: number;
            fact4: string | null;
            now: number;
            returnId: string;
        }) => db.prepare("UPDATE commerce_returns SET status='approved',approved_by=?,stock_journal_id=?,credit_journal_id=?,base_credit_minor=?,goods_fx_journal_id=?,version=version+1,updated_at=? WHERE id=?").bind(facts.userId, facts.stockJournal, facts.creditJournal, facts.baseCreditMinor, facts.fact4, facts.now, facts.returnId),
        readCommerceReturns3: facts => commerceFinanceJoinedFacts(db).readCommerceReturns3(facts),
        readFinanceJournalLines2: facts => CommerceFinanceFacts(db).readFinanceJournalLines2(facts),
        readFinanceCharts1: facts => CommerceFinanceFacts(db).readFinanceCharts1(facts),
        readFinanceAccounts1: facts => CommerceFinanceFacts(db).readFinanceAccounts1(facts),
        prepareFinanceInvoices2: facts => CommerceFinanceFacts(db).prepareFinanceInvoices2(facts),
        prepareFinanceInvoices3: facts => CommerceFinanceFacts(db).prepareFinanceInvoices3(facts),
        prepareCommerceReturns5: (facts: {
            credited: number;
            refund: number;
            carried: number;
            baseCreditedMinor: number;
            baseRefundMinor: number;
            fact5: string | null;
            now: number;
            returnId: string;
        }) => db.prepare("UPDATE commerce_returns SET status='posted',credited_minor=?,refund_minor=?,base_credit_minor=?,base_credited_minor=?,base_refund_minor=?,fx_journal_id=?,version=version+1,updated_at=? WHERE id=?").bind(facts.credited, facts.refund, facts.carried, facts.baseCreditedMinor, facts.baseRefundMinor, facts.fact5, facts.now, facts.returnId),
        prepareFinanceRefundObligations1: facts => CommerceFinanceFacts(db).prepareFinanceRefundObligations1(facts),
        readEmployeeRequests1: (facts: {
            companyId: string;
            id: string;
            id2: string;
        }) => db.prepare(`SELECT r.id FROM (${recordsProjectionSources.employee_requests}) r WHERE r.company_id=? AND (r.purchase_order_id=? OR EXISTS(SELECT 1 FROM (${procurementProjectionSources.procurement_request_orders}) l WHERE l.request_id=r.id AND l.order_id=?))`).bind(facts.companyId, facts.id, facts.id2).first<{
            id: string;
        }>(),
        readCommerceWarehouses2: (facts: {
            warehouseId: string;
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
        }) => db.prepare("SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)").bind(facts.warehouseId, facts.companyId, facts.branchId, facts.branchId2).first<Row>(),
        readCommerceProducts2: (facts: {
            productId: string;
            companyId: string;
        }) => db.prepare("SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'").bind(facts.productId, facts.companyId).first<Row>(),
        prepareCommerceOrders2: (facts: {
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
        }) => db.prepare("UPDATE commerce_orders SET party_id=?,warehouse_id=?,branch_id=?,order_date=?,due_date=?,note=?,subtotal_minor=?,tax_minor=?,price_list_id=?,price_list_version=?,discount_reason=?,discount_approval_required=?,discount_approved_by=NULL,discount_approved_at=NULL,version=version+1,updated_at=? WHERE id=?").bind(facts.partyId, facts.warehouseId, facts.branch_id, facts.orderDate, facts.dueDate, facts.note, facts.subtotal, facts.tax, facts.fact8, facts.fact9, facts.fact10, facts.fact11, facts.now, facts.record),
        prepareCommerceOrderLines1: (facts: {
            record: string;
        }) => db.prepare("DELETE FROM commerce_order_lines WHERE order_id=?").bind(facts.record),
        prepareCommerceOrders3: (facts: {
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
        }) => db.prepare("INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,price_list_id,price_list_version,discount_reason,discount_approval_required,created_by,created_at,updated_at) VALUES(?,?,?,?,?,CASE WHEN ?='quote' THEN erpx_sales_document_number(?,'QT',?) ELSE ? END,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(facts.record, facts.companyId, facts.branch_id, facts.departmentId, facts.kind, facts.kind, facts.companyId, facts.orderDate, facts.fact5, facts.partyId, facts.warehouseId, facts.currency, facts.orderDate, facts.dueDate, facts.note, facts.subtotal, facts.tax, facts.fact14, facts.fact15, facts.fact16, facts.fact17, facts.userId, facts.now, facts.now2),
        prepareCommerceOrderLines2: (facts: {
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
        }) => db.prepare("INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(facts.id, facts.record, facts.productId, facts.name, facts.qty, facts.price, facts.taxBps, facts.subtotal, facts.tax, facts.index, facts.listPrice, facts.discountBps, facts.itemType),
        readCommerceFulfillments4: (facts: {
            id: string;
        }) => db.prepare("SELECT id FROM commerce_fulfillments WHERE order_id=? LIMIT 1").bind(facts.id).first(),
        prepareInventoryOrderReservations3: facts => CommerceInventoryFacts(db).prepareInventoryOrderReservations3(facts),
        prepareCommerceOrders4: (facts: {
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
        }) => db.prepare("UPDATE commerce_orders SET state=?,approved_by=?,discount_approved_by=CASE WHEN ? AND discount_approval_required=1 THEN ? WHEN ? THEN NULL ELSE discount_approved_by END,discount_approved_at=CASE WHEN ? AND discount_approval_required=1 THEN ? WHEN ? THEN NULL ELSE discount_approved_at END,version=version+1,updated_at=? WHERE id=?").bind(facts.state, facts.fact1, facts.fact2, facts.userId, facts.fact4, facts.fact5, facts.fact6, facts.fact7, facts.fact8, facts.id),
        prepareFinanceParties1: facts => CommerceFinanceFacts(db).prepareFinanceParties1(facts),
        prepareCommerceOrders5: (facts: {
            salesId: string;
            fact1: string;
            userId: string;
            fact3: number;
            fact4: number;
            id: string;
        }) => db.prepare("INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,source_id,price_list_id,price_list_version,discount_reason,discount_approval_required,discount_approved_by,discount_approved_at,created_by,created_at,updated_at) SELECT ?,company_id,branch_id,department_id,'sales',?,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,id,price_list_id,price_list_version,discount_reason,discount_approval_required,discount_approved_by,discount_approved_at,?,?,? FROM commerce_orders WHERE id=?").bind(facts.salesId, facts.fact1, facts.userId, facts.fact3, facts.fact4, facts.id),
        prepareCommerceOrderLines3: (facts: {
            salesId: string;
            salesId2: string;
            id: string;
        }) => db.prepare("INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type) SELECT ?||'-'||ordinal,?,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type FROM commerce_order_lines WHERE order_id=?").bind(facts.salesId, facts.salesId2, facts.id),
        prepareFinanceJournals1: facts => CommerceFinanceFacts(db).prepareFinanceJournals1(facts),
        prepareFinanceJournalLines1: facts => CommerceFinanceFacts(db).prepareFinanceJournalLines1(facts),
        readAttachments2: facts => CommerceFilesFacts(db).readAttachments2(facts),
        prepareCommerceFulfillments2: (facts: {
            fulfillment: string;
            id: string;
            fact2: string;
            date: string;
            userId: string;
            fact5: number;
            fact6: string | null;
            completionMode: "goods" | "service" | "mixed";
        }) => db.prepare("INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,evidence_note,completion_kind,completion_note) VALUES(?,?,?,?,?,?,?,?,?)").bind(facts.fulfillment, facts.id, facts.fact2, facts.date, facts.userId, facts.fact5, facts.fact6, facts.completionMode, facts.fact6 ?? ""),
        readCommerceOrderLines2: (facts: {
            lineId: string;
            id: string;
        }) => db.prepare("SELECT l.*,p.tracking_mode,p.unit FROM commerce_order_lines l JOIN commerce_products p ON p.id=l.product_id WHERE l.id=? AND l.order_id=?").bind(facts.lineId, facts.id).first<Row>(),
        readInventoryActiveReservations3: facts => CommerceInventoryFacts(db).readInventoryActiveReservations3(facts),
        readInventoryOrderReservations3: facts => CommerceInventoryFacts(db).readInventoryOrderReservations3(facts),
        readCommerceFulfillmentLines2: (facts: {
            id: string | number | null;
        }) => db.prepare("SELECT COALESCE(SUM(subtotal_minor),0) subtotal,COALESCE(SUM(tax_minor),0) tax FROM commerce_fulfillment_lines WHERE order_line_id=?").bind(facts.id).first<{
            subtotal: number;
            tax: number;
        }>(),
        readInventoryLots1: facts => CommerceInventoryFacts(db).readInventoryLots1(facts),
        readInventoryQualityHolds3: facts => CommerceInventoryFacts(db).readInventoryQualityHolds3(facts),
        prepareCommerceStock2: (facts: {
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => commerceCommerceStockEffects(db).prepareCommerceStock2(facts),
        prepareInventoryOrderReservations4: facts => CommerceInventoryFacts(db).prepareInventoryOrderReservations4(facts),
        prepareCommerceStock3: (facts: {
            fact0: number;
            fact1: number;
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => commerceCommerceStockEffects(db).prepareCommerceStock3(facts),
        prepareCommerceFulfillmentLines1: (facts: {
            fact0: string;
            fulfillment: string;
            id: string | number | null;
            qty: number;
            cost: number;
            subtotal: number;
            tax: number;
            stockCurrency: string;
        }) => db.prepare("INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor,stock_currency) VALUES(?,?,?,?,?,?,?,?)").bind(facts.fact0, facts.fulfillment, facts.id, facts.qty, facts.cost, facts.subtotal, facts.tax, facts.stockCurrency),
        prepareInventoryLots3: facts => CommerceInventoryFacts(db).prepareInventoryLots3(facts),
        prepareInventoryLots4: facts => CommerceInventoryFacts(db).prepareInventoryLots4(facts),
        prepareInventoryLotBalances3: facts => CommerceInventoryFacts(db).prepareInventoryLotBalances3(facts),
        prepareInventoryLotBalances4: facts => CommerceInventoryFacts(db).prepareInventoryLotBalances4(facts),
        prepareInventoryLots5: facts => CommerceInventoryFacts(db).prepareInventoryLots5(facts),
        prepareInventoryLotMoves2: facts => CommerceInventoryFacts(db).prepareInventoryLotMoves2(facts),
        prepareInventoryFulfillmentLotAllocations1: facts => CommerceInventoryFacts(db).prepareInventoryFulfillmentLotAllocations1(facts),
        prepareCommerceOrderLines4: (facts: {
            qty: number;
            id: string | number | null;
        }) => db.prepare("UPDATE commerce_order_lines SET fulfilled_milli=fulfilled_milli+? WHERE id=?").bind(facts.qty, facts.id),
        prepareCommerceStockMoves1: (facts: {
            fact0: string;
            warehouse_id: string | number | null;
            product_id: string | number | null;
            fulfillment: string;
            fact4: number;
            fact5: number;
            fact6: number;
        }) => commerceCommerceStockEffects(db).prepareCommerceStockMoves1(facts),
        prepareCommerceGoodsValuations1: (facts: {
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
        }) => db.prepare(`INSERT INTO commerce_goods_valuations(fulfillment_id,company_id,chart_id,journal_id,document_currency,journal_currency,stock_currency,
    valuation_date,fx_rate_micros,source_rate_id,target_rate_id,used_latest_fallback,document_subtotal_minor,stock_value_minor,captured_by,captured_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(facts.fulfillment, facts.companyId, facts.chart_id, facts.journal, facts.fact4, facts.fact5, facts.stockCurrency, facts.date, facts.micros, facts.sourceRateId, facts.targetRateId, facts.usedLatestFallback, facts.documentSubtotal, facts.value, facts.userId, facts.fact15),
        prepareCommerceFulfillmentEvidence1: (facts: {
            fulfillment: string;
            attachmentId: string;
            userId: string;
            fact3: number;
        }) => db.prepare('INSERT INTO commerce_fulfillment_evidence(fulfillment_id,attachment_id,linked_by,linked_at) VALUES(?,?,?,?)').bind(facts.fulfillment, facts.attachmentId, facts.userId, facts.fact3),
        prepareCommerceFulfillments3: (facts: {
            journal: string;
            fulfillment: string;
        }) => db.prepare("UPDATE commerce_fulfillments SET journal_id=? WHERE id=?").bind(facts.journal, facts.fulfillment),
        prepareCommerceOrders6: (facts: {
            id: string;
            fact1: number;
            id2: string;
        }) => db.prepare("UPDATE commerce_orders SET state=CASE WHEN EXISTS(SELECT 1 FROM commerce_order_lines WHERE order_id=? AND fulfilled_milli<quantity_milli) THEN 'partially_fulfilled' ELSE 'fulfilled' END,version=version+1,updated_at=? WHERE id=?").bind(facts.id, facts.fact1, facts.id2),
        readCommerceFulfillments5: (facts: {
            fulfillmentId: string;
        }) => db.prepare("SELECT * FROM commerce_fulfillments WHERE id=?").bind(facts.fulfillmentId).first<Row>(),
        readFinanceJournals1: facts => CommerceFinanceFacts(db).readFinanceJournals1(facts),
        readCommerceFulfillmentLines3: (facts: {
            fulfillmentId: string;
        }) => db.prepare("SELECT SUM(subtotal_minor) subtotal,SUM(tax_minor) tax FROM commerce_fulfillment_lines WHERE fulfillment_id=?").bind(facts.fulfillmentId).first<{
            subtotal: number;
            tax: number;
        }>(),
        prepareFinanceInvoices4: facts => CommerceFinanceFacts(db).prepareFinanceInvoices4(facts),
        prepareCommerceFulfillments4: (facts: {
            invoice: string;
            fulfillmentId: string;
        }) => db.prepare("UPDATE commerce_fulfillments SET invoice_id=?,version=version+1 WHERE id=?").bind(facts.invoice, facts.fulfillmentId),
        readFinanceInvoices1: facts => CommerceFinanceFacts(db).readFinanceInvoices1(facts),
        readCommerceFulfillments6: (facts: {
            source_id: string | number | null;
        }) => db.prepare("SELECT order_id FROM commerce_fulfillments WHERE id=?").bind(facts.source_id).first<{
            order_id: string;
        }>(),
        readFinanceExpenses1: facts => CommerceFinanceFacts(db).readFinanceExpenses1(facts),
        readFinanceSupplierInstructions1: facts => CommerceFinanceFacts(db).readFinanceSupplierInstructions1(facts),
        readAttachmentSets1: facts => CommerceFilesFacts(db).readAttachmentSets1(facts),
        readFinanceAccounts2: facts => CommerceFinanceFacts(db).readFinanceAccounts2(facts),
        prepareFinancePayments1: facts => CommerceFinanceFacts(db).prepareFinancePayments1(facts),
        prepareFinanceAllocations1: facts => CommerceFinanceFacts(db).prepareFinanceAllocations1(facts),
        prepareFinanceInvoices5: facts => CommerceFinanceFacts(db).prepareFinanceInvoices5(facts),
        prepareFinanceExpenses1: facts => CommerceFinanceFacts(db).prepareFinanceExpenses1(facts),
        prepareCommerceProducts1: (facts: {
            sku: string;
            name: string;
            unit: string;
            trackingMode: "none" | "serial" | "lot";
            itemType: "stock" | "service";
            fact4: number;
            taxBps: number;
            status: "active" | "archived";
            id: string;
        }) => db.prepare("UPDATE commerce_products SET sku=?,name=?,unit=?,tracking_mode=?,item_type=?,price_minor=?,tax_bps=?,status=?,version=version+1 WHERE id=?").bind(facts.sku, facts.name, facts.unit, facts.trackingMode, facts.itemType, facts.fact4, facts.taxBps, facts.status, facts.id),
        prepareCommerceProducts2: (facts: {
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
        }) => db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,unit,tracking_mode,item_type,price_minor,tax_bps,status) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(facts.record, facts.companyId, facts.sku, facts.name, facts.unit, facts.trackingMode, facts.itemType, facts.fact6, facts.taxBps, facts.status),
        prepareFinanceParties2: facts => CommerceFinanceFacts(db).prepareFinanceParties2(facts),
        prepareFinanceParties3: facts => CommerceFinanceFacts(db).prepareFinanceParties3(facts),
        prepareCommerceWarehouses1: (facts: {
            code: string;
            name: string;
            status: "active" | "archived";
            id: string;
        }) => db.prepare("UPDATE commerce_warehouses SET code=?,name=?,status=?,version=version+1 WHERE id=?").bind(facts.code, facts.name, facts.status, facts.id),
        prepareCommerceWarehouses2: (facts: {
            record: string;
            companyId: string;
            branchId: string;
            code: string;
            name: string;
            status: "active" | "archived";
        }) => db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,?)").bind(facts.record, facts.companyId, facts.branchId, facts.code, facts.name, facts.status),
        readInventoryInTransit1: facts => CommerceInventoryFacts(db).readInventoryInTransit1(facts),
        guardRule1: (facts: {
            key: string;
            guardId: string;
            values: (string | number)[];
            table: string;
            states: string[];
        }) => facts.table === "finance_expenses" ? commerceExpensePaymentEffects(db).version(facts.key, facts.guardId, facts.values, facts.states) : facts.table === "finance_invoices" ? CommerceFinanceFacts(db).invoiceVersion(facts.key, facts.guardId, facts.values, facts.states) : assertion(db, facts.key, facts.guardId, `EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND version=? AND state IN (${facts.states.map(() => "?").join(",")}))`, facts.values),
        guardRule2: facts => commerceInventoryJoinedFacts(db).guardRule2(facts),
        guardRule3: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_warehouses w JOIN commerce_products p ON p.company_id=w.company_id WHERE w.id=? AND p.id=? AND w.company_id=? AND w.status='active' AND p.status='active')", facts.values),
        guardRule4: facts => commerceFinanceJoinedFacts(db).guardRule4(facts),
        guardRule5: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, "(SELECT quantity_milli-COALESCE((SELECT SUM(rl.quantity_milli) FROM commerce_return_lines rl JOIN commerce_returns r ON r.id=rl.return_id WHERE rl.fulfillment_line_id=? AND r.status!='rejected'),0)>=? FROM commerce_fulfillment_lines WHERE id=? AND fulfillment_id=?)", facts.values),
        guardRule6: (facts: {
            key: string;
            guardId: string;
            values: (string | number)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_returns WHERE id=? AND company_id=? AND status='requested' AND version=?)", facts.values),
        guardRule7: facts => CommerceFinanceFacts(db).guardRule7(facts),
        guardRule8: (facts: {
            key: string;
            guardId: string;
            values: (string | number)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_returns WHERE id=? AND company_id=? AND status='requested' AND version=?)", facts.values),
        guardRule9: facts => CommerceInventoryFacts(db).guardRule9(facts),
        guardRule10: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, `EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=?)`, facts.values),
        guardRule11: facts => CommerceInventoryFacts(db).guardRule11(facts),
        guardRule12: facts => CommerceInventoryFacts(db).guardRule12(facts),
        guardRule13: facts => CommerceFinanceFacts(db).guardRule13(facts),
        guardRule14: facts => commerceFinanceJoinedFacts(db).guardRule14(facts),
        guardRule15: facts => CommerceFinanceFacts(db).guardRule15(facts),
        guardRule16: facts => commerceFinanceJoinedFacts(db).guardRule16(facts),
        guardRule17: facts => CommerceFinanceFacts(db).guardRule17(facts),
        guardRule18: facts => CommerceSalesFacts(db).guardRule18(facts),
        guardRule19: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND version=? AND status='active')", facts.values),
        guardRule20: facts => CommerceFinanceFacts(db).guardRule20(facts),
        guardRule21: facts => CommerceFinanceFacts(db).guardRule21(facts),
        guardRule22: facts => CommerceFinanceFacts(db).guardRule22(facts),
        guardRule23: facts => CommerceFilesFacts(db).guardRule23(facts),
        guardRule24: facts => CommerceInventoryFacts(db).guardRule24(facts),
        guardRule25: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_order_lines WHERE id=? AND fulfilled_milli=? AND quantity_milli-fulfilled_milli>=?)", facts.values),
        guardRule26: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, `EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=?)`, facts.values),
        guardRule27: facts => CommerceInventoryFacts(db).guardRule27(facts),
        guardRule28: facts => CommerceInventoryFacts(db).guardRule28(facts),
        guardRule29: facts => CommerceInventoryFacts(db).guardRule29(facts),
        guardRule30: (facts: {
            key: string;
            guardId: string;
            values: (string | number)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_fulfillments WHERE id=? AND version=? AND invoice_id IS NULL)", facts.values),
        guardRule31: facts => CommerceFinanceFacts(db).guardRule31(facts),
        guardRule32: facts => CommerceFilesFacts(db).guardRule32(facts),
        guardRule33: facts => CommerceFinanceFacts(db).guardRule33(facts),
        guardRule34: facts => CommerceFinanceFacts(db).guardRule34(facts),
        guardRule35: facts => CommerceFinanceFacts(db).guardRule35(facts),
        guardRule36: facts => CommerceFinanceFacts(db).guardRule36(facts),
        guardRule37: facts => CommerceFinanceFacts(db).guardRule37(facts),
        guardRule38: (facts: {
            key: string;
            guardId: string;
            values: string[];
        }) => commerceExpensePaymentEffects(db).independentClaimant(facts.key, facts.guardId, facts.values),
        guardRule39: (facts: {
            key: string;
            guardId: string;
            values: string[];
        }) => commerceExpensePaymentEffects(db).unclaimed(facts.key, facts.guardId, facts.values),
        guardRule40: (facts: {
            key: string;
            guardId: string;
            values: (string | number)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND version=?)", facts.values),
        guardRule41: facts => CommerceFinanceFacts(db).guardRule41(facts),
        guardRule42: facts => CommerceOrganizationFacts(db).guardRule42(facts),
        guardRule43: (facts: {
            key: string;
            guardId: string;
            values: (string | number)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND version=?)", facts.values),
        guardRule44: (facts: {
            key: string;
            guardId: string;
            values: string[];
        }) => assertion(db, facts.key, facts.guardId, `NOT EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND quantity_milli>0)`, facts.values),
        snapshotMoney: (arg0: WorkContext, arg1: {
            amountUnits: string;
            currencyScale: number;
            sourceType: string;
            sourceId: string;
            amountKind: string;
            transactionDate: string;
            currency: string;
        }) => snapshotMoney(db, arg0, arg1),
        prepareAttachmentBinding: (arg0: WorkContext, arg1: {
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
        }) => prepareAttachmentBinding(db, arg0, arg1),
        prepareBusinessEvidence: (arg0: WorkContext, arg1: {
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
        }) => prepareBusinessEvidence(db, arg0, arg1),
        prepareReturnCredit: (arg0: WorkContext, arg1: {
            key: string;
            invoiceId: string;
            returnId: string;
            periodId: string;
            entryDate: string;
            postings: Posting[];
        }) => prepareReturnCredit(db, arg0, arg1),
        prepareReturnGoodsFx: (arg0: WorkContext, arg1: {
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
        }) => prepareReturnGoodsFx(db, arg0, arg1),
        snapshotMoneyReversal: (arg0: WorkContext, arg1: {
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
        }) => snapshotMoneyReversal(db, arg0, arg1),
        receiptQualityHoldSteps: (arg0: WorkContext, arg1: {
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
        }) => receiptQualityHoldSteps(db, arg0, arg1),
        prepareJournal: (arg0: WorkContext, arg1: {
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
        }, arg2: string, arg3: "posted" | "draft") => prepareJournal(db, arg0, arg1, arg2, arg3),
        refundNotifications: (arg0: string, arg1: string, arg2: "approval", arg3: "Posted return requires independent refund approval", arg4: string) => refundNotifications(db, arg0, arg1, arg2, arg3, arg4),
        getEmployeeRequest: (arg0: WorkContext, arg1: string) => getEmployeeRequest(db, arg0, arg1),
        prepareRequestOrderHandoff: (arg0: WorkContext, arg1: string, arg2: string, arg3: string, arg4: number | undefined, arg5: boolean, arg6: string | undefined) => prepareRequestOrderHandoff(db, arg0, arg1, arg2, arg3, arg4, arg5, arg6),
        resolveSalesPricing: (arg0: WorkContext, arg1: {
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
        }) => resolveSalesPricing(db, arg0, arg1),
        prepareRfqOrderHandoff: (arg0: WorkContext, arg1: {
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
        }, arg2: string, arg3: string, arg4: boolean) => prepareRfqOrderHandoff(db, arg0, arg1, arg2, arg3, arg4),
        confirmedFunctionalFx: (arg0: WorkContext, arg1: string, arg2: string, arg3: string) => confirmedFunctionalFx(db, arg0, arg1, arg2, arg3),
        supplierBillInvoiceHandoff: (arg0: WorkContext, arg1: Row, arg2: {
            expectedVersion: number;
            date: string;
            dueDate: string;
            matchId?: string | undefined;
            matchVersion?: number | undefined;
        }, arg3: string, arg4: string) => supplierBillInvoiceHandoff(db, arg0, arg1, arg2, arg3, arg4),
        assertControlledThaiTaxPaymentOwner: (arg0: WorkContext, arg1: string) => assertControlledThaiTaxPaymentOwner(db, arg0, arg1),
        replayCommand: (...args) => replayCommand(db, ...args),
        executeCommand: (...args) => executeCommand(db, ...args),
        pages: (actor, query) => commercePages(db, actor, query)
    };
}
type ManufacturingStockMasterMoneyCapture={money:import("@erpx/server-finance/public/manufacturing-initial-physical-stock-master-sources").ManufacturingInitialPhysicalStockProductMoney|null;errorCode?:string};
function applicationFor(owner?:SqlDatabase,masterCapture?:ManufacturingStockMasterMoneyCapture){return createCommerceApplication<SqlStatement>({clock:{now:()=>Date.now()},require:requirePermission,error:(status,code,message)=>new ApiError(status,code,message),hash:sha256,forActor:async actor=>{
 const original=unit(owner??(await holdingResources(actor.holdingId)).write,actor);
 if(!masterCapture)return original;
 return {...original,snapshotMoney:async(quoteActor,input)=>{const plan=await original.snapshotMoney(quoteActor,input);if(masterCapture.money)throw new ApiError(409,masterCapture.errorCode??'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original product command produced more than one price capture.');masterCapture.money={input:{...input},amountUnits:plan.amountUnits,usdUnits:plan.usdUnits,currencyScale:plan.currencyScale,rateId:plan.rateId,usedLatestFallback:plan.usedLatestFallback};return plan;}};
},forContext:async()=>{throw new Error('No registry context for this service');},attachmentReadable:attachmentsV2Readable,returnCompletionValues});}
/** Same-owner composition only. The public closed master commands supply their
 * already owned transaction; this bridge has no pool fallback or public port. */
export async function createManufacturingInitialPhysicalStockMasterInTransaction(database:SqlDatabase,actor:WorkContext,kind:'products'|'warehouses',input:unknown,key:string){
 const capture:ManufacturingStockMasterMoneyCapture={money:null};
 const result=await applicationFor(database,capture).saveMaster(actor,kind,null,input,key);
 return {result,money:capture.money};
}
const application = applicationFor();
export function commerceOverview(actor: WorkContext) { return application.commerceOverview(actor); }
export async function workOrderRows(actor: WorkContext, provided?: SqlDatabase) { const database=provided??(await holdingResources(actor.holdingId)).write;return application.workOrderRows(actor,unit(database,actor)); }

export function changeOrderReservation(actor: WorkContext, id: string, raw: unknown, key: string, ownerWrite?: SqlDatabase) {
 const input=raw as {action?:string};
 return withCommercePurchasedCommand(actor,{operation:"commerce.stock.reserve",key,target:{type:"order",id},purpose:input?.action==="release"?"settle-existing":"new-work",permissions:["inventory.prepare"],stock:true},database=>applicationFor(database).changeOrderReservation(actor,id,raw,key,unit(database)),ownerWrite);
}
export function requestCommerceReturn(actor: WorkContext, fulfillmentId: string, input: z.input<typeof returnRequestSchema>, key: string, ownerWrite?: SqlDatabase) {
 return withCommercePurchasedCommand(actor,{operation:"commerce.return.request",kindAuthority:true,key,target:{type:"fulfillment",id:fulfillmentId},purpose:"settle-existing",permissions:[]},database=>applicationFor(database).requestCommerceReturn(actor,fulfillmentId,input,key,unit(database)),ownerWrite);
}
export function decideCommerceReturn(actor: WorkContext, returnId: string, action: "approve" | "reject", input: z.infer<typeof returnDecisionSchema>, key: string) {
 return withCommercePurchasedCommand(actor,{operation:`commerce.return.${action}`,key,target:{type:"return",id:returnId},purpose:"settle-existing",permissions:["commerce.approve"],stock:action==="approve",financial:action==="approve"},database=>applicationFor(database).decideCommerceReturn(actor,returnId,action,input,key));
}
export function completeCommerceReturn(actor: WorkContext, returnId: string, input: z.infer<typeof returnCompleteSchema>, key: string) {
 return withCommercePurchasedCommand(actor,{operation:"commerce.return.complete",key,target:{type:"return",id:returnId},purpose:"settle-existing",permissions:["finance.write","finance.approve"],financial:true},database=>applicationFor(database).completeCommerceReturn(actor,returnId,input,key));
}
export function saveOrder(actor: WorkContext, id: string | null, input: z.infer<typeof orderSchema>, key: string, ownerWrite?: SqlDatabase) {
 return withCommercePurchasedCommand(actor,{operation:id?"commerce.order.update":"commerce.order.create",kindAuthority:true,key,target:id?{type:"order",id}:{type:"new",id:`order-${key}`,kind:input.kind},purpose:"new-work",permissions:[input.kind==="purchase"?"commerce.purchase":"commerce.sales"]},database=>applicationFor(database).saveOrder(actor,id,input,key,unit(database)),ownerWrite);
}
export function transitionOrder(actor: WorkContext, id: string, input: z.infer<typeof transitionSchema>, key: string, ownerWrite?: SqlDatabase) {
 return withCommercePurchasedCommand(actor,{operation:`commerce.order.${input.action}`,kindAuthority:true,key,target:{type:"order",id},purpose:["cancel","return"].includes(input.action)?"settle-existing":"new-work",permissions:input.action==="approve"?["commerce.approve"]:[],convert:input.action==="convert"},database=>applicationFor(database).transitionOrder(actor,id,input,key,unit(database)),ownerWrite);
}
export function fulfillOrder(actor: WorkContext, id: string, input: z.input<typeof fulfillmentSchema>, key: string) {
 return withCommercePurchasedCommand(actor,{operation:"commerce.order.fulfill",key,target:{type:"order",id},purpose:"settle-existing",permissions:input.completionMode==="service"?["commerce.sales"]:input.completionMode==="mixed"?["inventory.prepare","commerce.sales"]:["inventory.prepare"],transactional:true,stockLines:input.lines.map(line=>line.lineId),financial:true,receiving:true},database=>applicationFor(database).fulfillOrder(actor,id,input,key));
}
export function invoiceFulfillment(actor: WorkContext, fulfillmentId: string, input: z.infer<typeof invoiceFromDeliverySchema>, key: string) {
 return withCommercePurchasedCommand(actor,{operation:"commerce.invoice.create",key,target:{type:"fulfillment",id:fulfillmentId},purpose:"settle-existing",permissions:["finance.write"],transactional:true,financial:true},database=>applicationFor(database).invoiceFulfillment(actor,fulfillmentId,input,key));
}
export function settleInvoice(actor: WorkContext, invoiceId: string, input: z.input<typeof settlementSchema>, key: string) {
 return withCommercePurchasedCommand(actor,{operation:"commerce.invoice.settle",key,target:{type:"invoice",id:invoiceId},purpose:"settle-existing",permissions:["finance.write"],transactional:true,financial:true},async database=>{const result=await applicationFor(database).settleInvoice(actor,invoiceId,input,key);await ensureCustomerReceipt(database,actor,result.paymentId);return result;});
}
export function prepareInvoiceSettlement(write: SqlDatabase, actor: WorkContext, invoice: Row, expense: Row | null, input: z.infer<typeof settlementSchema>, key: string) { return application.prepareInvoiceSettlement(unit(write), actor, invoice, expense, input, key); }
export async function saveMaster(actor: WorkContext, kind: "products" | "parties" | "warehouses", id: string | null, input: unknown, key: string) {
    try { return await application.saveMaster(actor, kind, id, input, key); }
    catch (error) {
        if (error instanceof Error && error.message.includes("ITEM_TYPE_LOCKED")) throw new ApiError(409, "ITEM_TYPE_LOCKED", "This item has document or stock history. Create a new catalogue item to use a different type.");
        throw error;
    }
}
export function commercePage(actor: WorkContext, query: {
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
 if(["quote","sales","purchase"].includes(query.view))return holdingResources(actor.holdingId).then(async({write})=>{await requireCommercePurchasedRead(write,actor,query.view as "quote"|"sales"|"purchase");return application.commercePage(actor,query);});
 return application.commercePage(actor, query);
}

/** Same Commerce owner, same actual native application and supplied transaction.
 * This bridge is private to the two closed POS master commands. */
export async function createPosStartupMasterInTransaction(database:SqlDatabase,actor:WorkContext,kind:'products'|'warehouses',input:unknown,key:string){
 const capture:ManufacturingStockMasterMoneyCapture={money:null,errorCode:'POS_STARTUP_SOURCE_CHANGED'};
 const result=await applicationFor(database,capture).saveMaster(actor,kind,null,input,key);
 return {result,money:capture.money};
}

/** Sales service catalogue joins the original product writer in its owned unit. */
export async function createSalesServiceProductInTransaction(database:SqlDatabase,actor:WorkContext,input:unknown,key:string){
 const capture:ManufacturingStockMasterMoneyCapture={money:null,errorCode:'SALES_SERVICE_PRODUCT_SOURCE_CHANGED'};
 const result=await applicationFor(database,capture).saveMaster(actor,'products',null,input,key);return {result,money:capture.money};
}
