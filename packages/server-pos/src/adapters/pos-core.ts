import { nativePosOne, nativePosRun } from './native-owner-write';
import { nativePosAuditChargeToRoomStatement, nativePosAuditKitchenStartStatement } from './native-pos-audit-reads';
import { nativePosOpenShiftByIdStatement, nativePosOpenShiftByRegisterStatement, nativePosPaymentByIdPlainStatement, nativePosPostingJournalStatement, nativePosRegisterByIdStatement, nativePosRegisterByNameStatement, nativePosRegisterCatalogStatement, nativePosRegistersByKindStatement, nativePosRestaurantRegisterPropertyStatement, nativePosSaleByIdStatement, nativePosTableByNameStatement } from './native-pos-core-statement';
import { nativePosFulfilledLinesStatement, nativePosFirstPaymentIdStatement, nativePosLinesBySaleStatement, nativePosLinesTotalStatement, nativePosOpenPaymentIdStatement, nativePosPaymentIdBySaleStatement, nativePosPendingPaymentIdStatement, nativePosRefundablePaymentsStatement, nativePosSucceededPaymentTotalStatement } from './native-pos-sale-child';
import { posGuestAccountInterface } from "@erpx/server-pms/public/pos-guest-account-interface";
import { hospitalityOwnerProjectionSources as pmsProjectionSources } from "@erpx/server-pms/public/hospitality-owner-projection-sources";
import 'server-only';
import { createHash, randomUUID } from 'node:crypto';
import { type PosAccounts } from "@erpx/server-pos/public/rules/pos";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { requirePosImageSource } from "@erpx/server-pos/public/pos-image-source";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import { ApiError } from "@erpx/server-runtime/public/http";
import { applyPosRoomCharge, reversePosRoomCharge } from "../../../../packages/server-pms/src/public/pms";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {snapshotMoney,snapshotMoneyCarry,snapshotMoneyReversal} from '@erpx/server-finance/public/company-money';
import { capturePosReceipt, createPosCustomer, posDayCommand, assertPosDayWritable } from "@erpx/server-pos/public/pos-daily";
import type { SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { createPosApplication, type PosUnit, type PosRepository, type PosRow } from "@erpx/server-pos/application/pos-core";
import { withPosPurchasedCommand, posPackageEnforcementMode } from "@erpx/server-pos/adapters/purchase-boundary";
export type { PosRow } from "@erpx/server-pos/application/pos-core";
import {one,all,run}from '@erpx/server-runtime/public/runtime/sql';
import { posCoreFinanceFacts } from "@erpx/server-finance/public/pos-core-facts";
import { posCoreCommerceFacts } from "@erpx/server-commerce/public/pos-core-facts";
import { posCoreInventoryFacts } from "@erpx/server-inventory/public/pos-core-facts";
import { posCoreIdentityAccessFacts } from "@erpx/server-identity_access/public/pos-core-facts";
import { posCoreWorkflowFacts } from "@erpx/server-workflow/public/pos-core-facts";
import { posCoreOrganizationFacts } from "@erpx/server-organization/public/pos-core-facts";
function unit(db: SqlDatabase): PosUnit<SqlStatement> {
    return {
        assertHotelDay: async (company,registerId,date) => {
            const register=await one(db,'SELECT property_id,branch_id FROM pos_registers WHERE company_id=? AND id=?',company,registerId);
            if(register) await posGuestAccountInterface(db).assertDay(company,register.property_id,register.branch_id,date);
        },
        assertFulfillmentHotelDay: async (company,fulfillmentId,date) => {
            const rows=await all(db,'SELECT DISTINCT r.property_id,r.branch_id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND l.fulfillment_id=?',company,fulfillmentId);
            for(const row of rows) await posGuestAccountInterface(db).assertDay(company,row.property_id,row.branch_id,date);
        },
        reversalRequest: id => one(db, 'SELECT * FROM pos_room_charge_reversal_requests WHERE id=? FOR UPDATE', id),
        pendingReversal: saleId => one(db, "SELECT id FROM pos_room_charge_reversal_requests WHERE sale_id=? AND state IN ('requested','pos_approved')", saleId),
        saveReversalRequest: async row => { await nativePosRun(db, String(row.company_id ?? ''), 'core_savereversalrequest_41', [row.id, row.sale_id, row.company_id, row.branch_id, row.property_id, row.sale_version, row.property_version, row.amount_minor, row.reason, row.restock_line_ids_json, row.requested_by, row.created_at, row.created_at]); },
        reviewReversalRequest: async (id,state,user,reason,now) => { await nativePosRun(db, '', 'core_reviewreversalrequest_42', [state, state, user, state, user, state, user, reason, now, id]); },
        prepareAccounting: async (company,accounts,user) => { await nativePosRun(db, company, 'core_prepareaccounting_43', [JSON.stringify(accounts), user, company]); },
        linkRegister: async (id,propertyId) => { await nativePosRun(db, '', 'core_linkregister_44', [propertyId, id]); },
        readPosSales1: (facts: {
            saleId: string;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE', facts.saleId),
        readPosSettings1: (facts: {
            companyId: string;
        }) => one(db, 'SELECT * FROM pos_settings WHERE company_id=?', facts.companyId),
        readFinancePeriods1: facts => posCoreFinanceFacts(db).readFinancePeriods1(facts),
        readPosPostings1: (facts: {
            sourceKey: string;
        }) => nativePosPostingJournalStatement(db, facts.sourceKey),
        readFinanceCharts1: facts => posCoreFinanceFacts(db).readFinanceCharts1(facts),
        readFinanceAccounts1: facts => posCoreFinanceFacts(db).readFinanceAccounts1(facts),
        writeFinanceJournals1: facts => posCoreFinanceFacts(db).writeFinanceJournals1(facts),
        writeFinanceJournalLines1: facts => posCoreFinanceFacts(db).writeFinanceJournalLines1(facts),
        writePosPostings1: (facts: {
            sourceKey: string;
            companyId: string;
            journal: `${string}-${string}-${string}-${string}-${string}`;
        }) => nativePosRun(db, facts.companyId, 'core_writepospostings1_63', [facts.sourceKey, facts.companyId, facts.journal]),
        writePosAudit1: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            companyId: string;
            branchId: string | null;
            actorId: string;
            operation: string;
            recordId: string;
            fact6: string;
            fact7: number;
        }) => nativePosRun(db, facts.companyId, 'core_writeposaudit1_73', [facts.fact0, facts.companyId, facts.branchId, facts.actorId, facts.operation, facts.recordId, facts.fact6, facts.fact7]),
        writePosJobs1: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            companyId: string;
            kind: string;
            recordId: string;
            fact4: string;
            fact5: number;
        }) => nativePosRun(db, facts.companyId, 'core_writeposjobs1_81', [facts.fact0, facts.companyId, facts.kind, facts.recordId, facts.fact4, facts.fact5]),
        readCommerceStock1: facts => posCoreCommerceFacts(db).readCommerceStock1(facts),
        readInventoryActiveReservations1: facts => posCoreInventoryFacts(db).readInventoryActiveReservations1(facts),
        writePosReservations1: (facts: {
            id: string | number | boolean | null;
            warehouse_id: string | number | boolean | null;
            productId: string;
            quantityMilli: number;
        }) => nativePosRun(db, '', 'core_writeposreservations1_89', [facts.id, facts.warehouse_id, facts.productId, facts.quantityMilli]),
        writePosFulfillments1: (facts: {
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposfulfillments1_92', [facts.id]),
        readInventoryLedger1: facts => posCoreInventoryFacts(db).readInventoryLedger1(facts),
        writePosReservations2: (facts: {
            id: string | number | boolean | null;
            productId: string;
        }) => nativePosRun(db, '', 'core_writeposreservations2_97', [facts.id, facts.productId]),
        writeCommerceStock1: facts => posCoreCommerceFacts(db).writeCommerceStock1(facts),
        writePosStockMoves1: (facts: {
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
        }) => nativePosRun(db, String(facts.company_id ?? ''), 'core_writeposstockmoves1_112', [facts.fact0, facts.company_id, facts.warehouse_id, facts.productId, facts.id, facts.id2, facts.fact6, facts.fact7, facts.fact8, facts.fact9, facts.business_date, facts.fact11]),
        writePosFulfillments2: (facts: {
            cost: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposfulfillments2_116', [facts.cost, facts.id]),
        readPosSales2: (facts: {
            sale_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE', facts.sale_id),
        writeFinancePayments1: facts => posCoreFinanceFacts(db).writeFinancePayments1(facts),
        writePosPayments1: (facts: {
            financeId: `${string}-${string}-${string}-${string}-${string}`;
            fact1: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writepospayments1_125', [facts.financeId, facts.fact1, facts.id]),
        writePosSales2: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writepossales2_129', [facts.fact0, facts.id]),
        readPosLines1: (facts: {
            id: string | number | boolean | null;
        }) => nativePosLinesBySaleStatement(db, facts.id),
        readPosPayments2: (facts: {
            id: string | number | boolean | null;
        }) => nativePosSucceededPaymentTotalStatement(db, facts.id),
        readPosPayments3: (facts: {
            id: string | number | boolean | null;
        }) => nativePosPendingPaymentIdStatement(db, facts.id),
        readPosFulfillments1: (facts: {
            itemId: string;
        }) => one(db, 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE', facts.itemId),
        writePosFulfillments3: (facts: {
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposfulfillments3_144', [facts.id]),
        writeFinanceParties1: facts => posCoreFinanceFacts(db).writeFinanceParties1(facts),
        writeFinanceInvoices1: facts => posCoreFinanceFacts(db).writeFinanceInvoices1(facts),
        readPosPayments4: (facts: {
            id: string | number | boolean | null;
        }) => nativePosRefundablePaymentsStatement(db, facts.id),
        writeFinancePayments2: facts => posCoreFinanceFacts(db).writeFinancePayments2(facts),
        writeFinanceAllocations1: facts => posCoreFinanceFacts(db).writeFinanceAllocations1(facts),
        writePosSales3: (facts: {
            invoice: `${string}-${string}-${string}-${string}-${string}`;
            party: string | number | true;
            fact2: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writepossales3_157', [facts.invoice, facts.party, facts.fact2, facts.id]),
        readAssignments1: facts => posCoreIdentityAccessFacts(db).readAssignments1(facts),
        writeContext1: (facts: {
            companyId: string;
        }) => run(db, "SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))", facts.companyId),
        readPosReceipts1: (facts: {
            key: string;
        }) => one(db, 'SELECT * FROM pos_receipts WHERE id=?', facts.key),
        writeCommandAssertions1: facts => posCoreWorkflowFacts(db).writeCommandAssertions1(facts),
        writePosReceipts1: (facts: {
            key: string;
            companyId: string;
            hash: string;
            fact3: string;
            fact4: number;
        }) => nativePosRun(db, facts.companyId, 'core_writeposreceipts1_172', [facts.key, facts.companyId, facts.hash, facts.fact3, facts.fact4]),
        readOrganizationNodes1: facts => posCoreOrganizationFacts(db).readOrganizationNodes1(facts),
        readPosSettings2: (facts: {
            company: string;
        }) => one(db, 'SELECT company_id FROM pos_settings WHERE company_id=? FOR UPDATE', facts.company),
        readPosRegisters1: (facts: {
            company: string;
            mode: "retail" | "restaurant";
        }) => all(db, "SELECT * FROM pos_registers WHERE company_id=? AND mode=? ORDER BY name,id FOR UPDATE", facts.company, facts.mode),
        readCommerceSettings1: facts => posCoreCommerceFacts(db).readCommerceSettings1(facts),
        readFinanceCharts2: facts => posCoreFinanceFacts(db).readFinanceCharts2(facts),
        readFinanceAccounts2: facts => posCoreFinanceFacts(db).readFinanceAccounts2(facts),
        readCommerceWarehouses1: facts => posCoreCommerceFacts(db).readCommerceWarehouses1(facts),
        readCommerceWarehouses2: facts => posCoreCommerceFacts(db).readCommerceWarehouses2(facts),
        readFinanceAccounts3: facts => posCoreFinanceFacts(db).readFinanceAccounts3(facts),
        readFinanceAccounts4: facts => posCoreFinanceFacts(db).readFinanceAccounts4(facts),
        writeFinanceAccounts1: facts => posCoreFinanceFacts(db).writeFinanceAccounts1(facts),
        readPosRegisters2: (facts: {
            company: string;
            branchId: string;
            registerName: string;
        }) => nativePosRegisterByNameStatement(db, facts),
        writePosSettings1: (facts: {
            company: string;
            currency: string;
            fact2: string;
        }) => nativePosRun(db, facts.company, 'core_writepossettings1_198', [facts.company, facts.currency, facts.fact2]),
        writePosRegisters1: (facts: {
            registerId: string;
            company: string;
            branchId: string;
            warehouseId: string;
            registerName: string;
            mode: "retail" | "restaurant";
        }) => nativePosRun(db, facts.company, 'core_writeposregisters1_206', [facts.registerId, facts.company, facts.branchId, facts.warehouseId, facts.registerName, facts.mode]),
        writePosOauthStates1: (facts: {
            state: `${string}-${string}-${string}-${string}-${string}`;
            company: string;
            user: string;
            fact3: number;
            livemode: boolean;
        }) => nativePosRun(db, facts.company, 'core_writeposoauthstates1_213', [facts.state, facts.company, facts.user, facts.fact3, facts.livemode]),
        readPosSettings3: (facts: {
            company: string;
        }) => one(db, 'SELECT * FROM pos_settings WHERE company_id=? FOR UPDATE', facts.company),
        readFinanceAccounts5: facts => posCoreFinanceFacts(db).readFinanceAccounts5(facts),
        readFinanceCharts3: facts => posCoreFinanceFacts(db).readFinanceCharts3(facts),
        readCommerceSettings2: facts => posCoreCommerceFacts(db).readCommerceSettings2(facts),
        writePosSettings2: (facts: {
            company: string;
            enabled: boolean;
            currency: string;
            timezone: "Asia/Bangkok";
            fact4: string;
            cashierDiscountBps: number;
        }) => nativePosRun(db, facts.company, 'core_writepossettings2_227', [facts.company, facts.enabled, facts.currency, facts.timezone, facts.fact4, facts.cashierDiscountBps]),
        readCommerceWarehouses3: facts => posCoreCommerceFacts(db).readCommerceWarehouses3(facts),
        readPosRegisters3: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=? FOR UPDATE', facts.id),
        readPosShifts1: (facts: {
            id: string;
        }) => nativePosOpenShiftByRegisterStatement(db, facts.id),
        writePosRegisters2: (facts: {
            warehouseId: string;
            branch_id: string | number | boolean | null;
            name: string;
            mode: "retail" | "restaurant";
            active: boolean;
            rid: string;
        }) => nativePosRun(db, '', 'core_writeposregisters2_242', [facts.warehouseId, facts.branch_id, facts.name, facts.mode, facts.active, facts.rid]),
        writePosRegisters3: (facts: {
            rid: string;
            company: string;
            branch_id: string | number | boolean | null;
            warehouseId: string;
            name: string;
            mode: "retail" | "restaurant";
            active: boolean;
        }) => nativePosRun(db, facts.company, 'core_writeposregisters3_251', [facts.rid, facts.company, facts.branch_id, facts.warehouseId, facts.name, facts.mode, facts.active]),
        readPosRegisters4: (facts: {
            registerId: string;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=? FOR UPDATE', facts.registerId),
        readPosTables1: (facts: {
            registerId: string;
            name: string;
        }) => nativePosTableByNameStatement(db, facts),
        readPosCatalog1: (facts: {
            productId: string;
            company: string;
        }) => one(db, 'SELECT * FROM pos_catalog WHERE product_id=? AND company_id=? FOR UPDATE', facts.productId, facts.company),
        readPosRegisterCatalog1: (facts: {
            productId: string;
            branchId: string;
            company: string;
        }) => nativePosRegisterCatalogStatement(db, facts.productId, facts.branchId, facts.company),
        writePosCatalog1: (facts: {
            imageId: string;
            productId: string;
        }) => nativePosRun(db, '', 'core_writeposcatalog1_271', [facts.imageId, facts.productId]),
        readCommerceProducts1: facts => posCoreCommerceFacts(db).readCommerceProducts1(facts),
        readCommerceProducts2: facts => posCoreCommerceFacts(db).readCommerceProducts2(facts),
        readPosCatalog2: (facts: {
            productId: string;
        }) => one(db, 'SELECT * FROM pos_catalog WHERE product_id=? FOR UPDATE', facts.productId),
        readPosRegisters5: (facts: {
            company: string;
            branchId: string | null;
            branchId2: string | null;
            kind: "stock" | "recipe" | "service";
        }) => nativePosRegistersByKindStatement(db, facts),
        readPosRegisters6: (facts: {
            rid: string;
        }) => nativePosRegisterByIdStatement(db, facts.rid),
        writePosCatalog2: (facts: {
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
        }) => nativePosRun(db, facts.company, 'core_writeposcatalog2_298', [facts.productId, facts.company, facts.fact2, facts.category, facts.kind, facts.station, facts.fact6, facts.fact7, facts.taxInclusive, facts.enabled, facts.fact10]),
        writePosRegisterCatalog1: (facts: {
            productId: string;
            company: string;
            branchId: string;
        }) => nativePosRun(db, facts.company, 'core_writeposregistercatalog1_303', [facts.productId, facts.company, facts.branchId]),
        writePosRegisterCatalog2: (facts: {
            productId: string;
        }) => nativePosRun(db, '', 'core_writeposregistercatalog2_306', [facts.productId]),
        writePosRegisterCatalog3: (facts: {
            rid: string;
            productId: string;
        }) => nativePosRun(db, '', 'core_writeposregistercatalog3_310', [facts.rid, facts.productId]),
        readPosDayCloses1: (facts: {
            company_id: string | number | boolean | null;
            branch_id: string | number | boolean | null;
            business_date: string | number | boolean | null;
        }) => one(db, "SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=? AND state='prepared'", facts.company_id, facts.branch_id, facts.business_date),
        readPosAudit1: (facts: {
            id: string | number | boolean | null;
        }) => nativePosAuditChargeToRoomStatement(db, facts),
        readPosRoomChargeReversals1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT sale_id FROM pos_room_charge_reversals WHERE sale_id=?', facts.id),
        readPosPayments6: (facts: {
            id: string | number | boolean | null;
        }) => nativePosPaymentIdBySaleStatement(db, facts.id),
        readPmsSources1: (facts: {
            fact0: string;
            company: string;
            branch_id: string | number | boolean | null;
        }) => one(db, `SELECT ps.property_id,ps.record_id FROM (${pmsProjectionSources.pms_sources}) ps JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=ps.property_id WHERE ps.source_key=? AND p.company_id=? AND p.branch_id=?`, facts.fact0, facts.company, facts.branch_id),
        readPosLines2: (facts: {
            id: string | number | boolean | null;
        }) => nativePosFulfilledLinesStatement(db, facts.id),
        readPosLines3: (facts: {
            fulfillment_id: string | number | boolean | null;
            id: string | number | boolean | null;
        }) => one(db, 'SELECT COALESCE(SUM(quantity_milli),0) n FROM pos_lines WHERE fulfillment_id=? AND id<?', facts.fulfillment_id, facts.id),
        readCommerceStock3: facts => posCoreCommerceFacts(db).readCommerceStock3(facts),
        writeCommerceStock2: facts => posCoreCommerceFacts(db).writeCommerceStock2(facts),
        writePosStockMoves2: (facts: {
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
        }) => nativePosRun(db, facts.company, 'core_writeposstockmoves2_352', [facts.fact0, facts.company, facts.warehouse_id, facts.product_id, facts.fulfillment_id, facts.id, facts.number, facts.fact7, facts.quantity_milli, facts.cost, facts.date, facts.fact11]),
        writePosRoomChargeReversals1: (facts: {
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
        }) => nativePosRun(db, facts.company, 'core_writeposroomchargereversals1_365', [facts.id, facts.company, facts.propertyId, facts.entryId, facts.reversalEntryId, facts.reason, facts.fact6, facts.restored, facts.wasted, facts.user, facts.fact10]),
        writePosSales6: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writepossales6_369', [facts.fact0, facts.id]),
        readPosRegisters9: (facts: {
            register_id: string | number | boolean | null;
        }) => nativePosRestaurantRegisterPropertyStatement(db, facts.register_id),
        readPosShifts4: (facts: {
            shift_id: string | number | boolean | null;
        }) => nativePosOpenShiftByIdStatement(db, facts.shift_id),
        readPosPayments7: (facts: {
            id: string | number | boolean | null;
        }) => nativePosFirstPaymentIdStatement(db, facts.id),
        readPosLines5: (facts: {
            id: string | number | boolean | null;
        }) => nativePosLinesTotalStatement(db, facts.id),
        readPmsProperties2: (facts: {
            propertyId: string;
            company: string;
        }) => one(db, `SELECT branch_id,business_date,version,state FROM (${pmsProjectionSources.pms_properties}) pms_properties WHERE id=? AND company_id=?`, facts.propertyId, facts.company),
        writePosSales7: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writepossales7_389', [facts.fact0, facts.id]),
        writePosReservations4: (facts: {
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposreservations4_392', [facts.id]),
        readPosFulfillments5: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE', facts.id),
        readPosLines11: (facts: {
            id: string | number | boolean | null;
            user: string;
        }) => one(db, 'SELECT s.created_by FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.created_by=? LIMIT 1', facts.id, facts.user),
        readPosAudit2: (facts: {
            id: string | number | boolean | null;
        }) => nativePosAuditKitchenStartStatement(db, facts),
        readPosLines12: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT l.id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.state='completed' AND l.returned_milli<l.quantity_milli", facts.id),
        writePosFulfillments10: (facts: {
            state: string;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposfulfillments10_409', [facts.state, facts.id]),
        readFinanceBankTransactions1: facts => posCoreFinanceFacts(db).readFinanceBankTransactions1(facts),
        readPosPayments13: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        readPosJobs1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT state FROM pos_jobs WHERE record_id=? AND kind='checkout' FOR UPDATE", facts.id),
        writePosPayments5: (facts: {
            payerEmail: string;
            fact1: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writepospayments5_421', [facts.payerEmail, facts.fact1, facts.id]),
        writePosJobs2: (facts: {
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposjobs2_424', [facts.id]),
        readPosConnections2: (facts: {
            company: string;
        }) => one(db, "SELECT * FROM pos_connections WHERE company_id=? AND state='ready' FOR UPDATE", facts.company),
        readPosSettlements1: (facts: {
            id: string;
            company: string;
        }) => one(db, "SELECT * FROM pos_settlements WHERE id=? AND company_id=? AND state='unmapped' FOR UPDATE", facts.id, facts.company),
        readFinanceJournals1: facts => posCoreFinanceFacts(db).readFinanceJournals1(facts),
        readPosSettlements2: (facts: {
            journalId: string;
        }) => one(db, 'SELECT id FROM pos_settlements WHERE journal_id=?', facts.journalId),
        readFinanceJournalLines1: facts => posCoreFinanceFacts(db).readFinanceJournalLines1(facts),
        writePosSettlements1: (facts: {
            fact0: "paid" | "mapped";
            journalId: string;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writepossettlements1_441', [facts.fact0, facts.journalId, facts.id]),
        readPosSettlements3: (facts: {
            id: string;
            company: string;
        }) => one(db, "SELECT * FROM pos_settlements WHERE id=? AND company_id=? AND kind='payout' AND state='paid' FOR UPDATE", facts.id, facts.company),
        readFinanceBankMatchTargets1: facts => posCoreFinanceFacts(db).readFinanceBankMatchTargets1(facts),
        readFinanceJournalLines2: facts => posCoreFinanceFacts(db).readFinanceJournalLines2(facts),
        writeFinanceBankTransactions2: facts => posCoreFinanceFacts(db).writeFinanceBankTransactions2(facts),
        writePosSettlements2: (facts: {
            id: string | number | boolean | null;
            id2: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writepossettlements2_452', [facts.id, facts.id2]),
        readPosJobs2: (facts: {
            id: string;
            company: string;
        }) => one(db, "SELECT * FROM pos_jobs WHERE id=? AND company_id=? AND state='exception' FOR UPDATE", facts.id, facts.company),
        readPosRefundPayments2: (facts: {
            providerId: string;
            record_id: string | number | boolean | null;
        }) => nativePosOne(db, '', 'core_readposrefundpayments2_460', [facts.providerId, facts.record_id]),
        readPosPayments14: (facts: {
            providerId: string;
            record_id: string | number | boolean | null;
        }) => nativePosOne(db, '', 'core_readpospayments14_464', [facts.providerId, facts.record_id]),
        writePosJobs3: (facts: {
            fact0: string;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposjobs3_468', [facts.fact0, facts.id]),
        readPosRefundPayments3: (facts: {
            record_id: string | number | boolean | null;
            company: string;
            company2: string;
        }) => one(db, "SELECT r.id,r.version,r.state refund_state,rp.state payout_state,rp.provider_id,p.method,p.payer_email FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND r.company_id=? AND p.company_id=? FOR UPDATE OF rp,r", facts.record_id, facts.company, facts.company2),
        writePosRefunds1: (facts: {
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposrefunds1_476', [facts.id]),
        readPosRefundPayments4: (facts: {
            record_id: string | number | boolean | null;
        }) => nativePosOne(db, '', 'core_readposrefundpayments4_479', [facts.record_id]),
        writePosJobs4: (facts: {
            fact0: string;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposjobs4_483', [facts.fact0, facts.id]),
        readPosRefundPayments5: (facts: {
            refundPaymentId: string;
        }) => one(db, 'SELECT * FROM pos_refund_payments WHERE id=? FOR UPDATE', facts.refundPaymentId),
        readPosRefunds2: (facts: {
            refund_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_refunds WHERE id=? FOR UPDATE', facts.refund_id),
        readPosPayments16: (facts: {
            payment_id: string | number | boolean | null;
        }) => nativePosPaymentByIdPlainStatement(db, facts.payment_id),
        readPosSales5: (facts: {
            sale_id: string | number | boolean | null;
        }) => nativePosSaleByIdStatement(db, facts.sale_id),
        writeFinancePayments3: facts => posCoreFinanceFacts(db).writeFinancePayments3(facts),
        writePosRefundPayments2: (facts: {
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposrefundpayments2_499', [facts.id]),
        writePosRefunds5: (facts: {
            id: string | number | boolean | null;
            id2: string | number | boolean | null;
        }) => nativePosRun(db, '', 'core_writeposrefunds5_503', [facts.id, facts.id2]),
        readPosPayments17: (facts: {
            saleId: string;
        }) => nativePosOpenPaymentIdStatement(db, facts.saleId),
        readPosRefundPayments6: (facts: {
            saleId: string;
        }) => one(db, "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE p.sale_id=? AND rp.state<>'succeeded'", facts.saleId),
        writePosReservations6: (facts: {
            saleId: string;
        }) => nativePosRun(db, '', 'core_writeposreservations6_512', [facts.saleId]),
        writePosFulfillments12: (facts: {
            saleId: string;
        }) => nativePosRun(db, '', 'core_writeposfulfillments12_515', [facts.saleId]),
        writePosSales21: (facts: {
            saleId: string;
        }) => nativePosRun(db, '', 'core_writepossales21_518', [facts.saleId]),
        snapshotMoneyCarry: (arg0: {
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
        }) => snapshotMoneyCarry(db, arg0, arg1),
        capturePosReceipt: (arg0: string) => capturePosReceipt(db, arg0),
        snapshotMoney: (arg0: {
            companyId: string;
        }, arg1: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
            transactionDate: string;
            currency: string;
            currencyScale: number;
            amountUnits: string;
        }) => snapshotMoney(db, arg0, arg1),
        snapshotMoneyReversal: (arg0: {
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
        }) => snapshotMoneyReversal(db, arg0, arg1),
        assertProductionModuleEnabled: (arg0: string, arg1: "pos") => assertProductionModuleEnabled(db, arg0, arg1),
        workContextGuards: (arg0: WorkContext, arg1: string) => workContextGuards(db, arg0, arg1),
        iamMutationGuards: (arg0: WorkContext, arg1: string) => iamMutationGuards(db, arg0, arg1),
        posDayCommand: (arg0: WorkContext, arg1: string, arg2: unknown) => posDayCommand(db, arg0, arg1, arg2),
        createPosCustomer: (arg0: WorkContext, arg1: unknown) => createPosCustomer(db, arg0, arg1),
        requirePosImageSource: (arg0: WorkContext, arg1: string, arg2: string, arg3: string) => requirePosImageSource(db, arg0, arg1, arg2, arg3),
        assertPosDayWritable: (arg0: string, arg1: string, arg2: string) => assertPosDayWritable(db, arg0, arg1, arg2),
        reversePosRoomCharge: (arg0: WorkContext, arg1: {
            propertyId: string;
            sourceKey: string;
            expectedPropertyVersion: number;
            reason: string;
            total: number;
        }) => reversePosRoomCharge(db, arg0, arg1),
        applyPosRoomCharge: (arg0: WorkContext, arg1: {
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
        }, arg2: number) => applyPosRoomCharge(db, arg0, arg1, arg2), batch: effects => db.batch(effects)
    };
}
function repository(db: PostgresDatabase): PosRepository<SqlStatement> { return { ...unit(db), transaction: work => db.transaction(tx => work(unit(tx))) }; }
const application = createPosApplication<SqlStatement>({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), isBusinessError: (error): error is ApiError => error instanceof ApiError, environment: key => process.env[key] });
export const id = application.id;
export const num = application.num;
export const str = application.str;
export const pair = application.pair;
export const posPermissions = application.posPermissions;
export function json<T>(value: unknown): T { return application.json<T>(value); }
export function fail(code: string, message: string, status = 409): never { return application.fail(code, message, status); }
export function scopeRow(actor: WorkContext, row: PosRow | null) { return application.scopeRow(actor, row); }
export function saleRow(db: SqlDatabase, actor: WorkContext, saleId: string) { return application.saleRow(unit(db), actor, saleId); }
export function config(db: SqlDatabase, companyId: string) { return application.config(unit(db), companyId); }
export function openPeriod(db: SqlDatabase, companyId: string, date: string) { return application.openPeriod(unit(db), companyId, date); }
export function posting(db: SqlDatabase, companyId: string, branchId: string | null, accounts: PosAccounts, date: string, currency: string, sourceKey: string, sourceId: string, actorId: string, entries: Array<{
    account: string;
    debit: number;
    credit: number;
}>) { return application.posting(unit(db), companyId, branchId, accounts, date, currency, sourceKey, sourceId, actorId, entries); }
export function audit(db: SqlDatabase, companyId: string, branchId: string | null, actorId: string, operation: string, recordId: string, details: unknown) { return application.audit(unit(db), companyId, branchId, actorId, operation, recordId, details); }
export function enqueue(db: SqlDatabase, companyId: string, kind: string, recordId: string, payload: unknown = {}) { return application.enqueue(unit(db), companyId, kind, recordId, payload); }
export function reserve(db: SqlDatabase, f: PosRow) { return application.reserve(unit(db), f); }
export function receivePayment(db: SqlDatabase, payment: PosRow, actorId: string, date?: string) { return application.receivePayment(unit(db), payment, actorId, date); }
export function completeSale(db: SqlDatabase, sale: PosRow, actorId: string, date?: string) { return application.completeSale(unit(db), sale, actorId, date); }
export async function posCommand(db: PostgresDatabase, actor: WorkContext, operation: string, input: unknown, key: string) {
    const mode = await posPackageEnforcementMode(), commands = repository(db);
    if (mode !== "disabled") commands.transaction = work => withPosPurchasedCommand(db, actor, operation, input, key, mode, transaction => work(unit(transaction)));
    return application.posCommand(commands, actor, operation, input, key);
}
export function completeRefundPayment(db: SqlDatabase, refundPaymentId: string, actorId: string, date?: string) { return application.completeRefundPayment(unit(db), refundPaymentId, actorId, date); }
export function releaseUnpaidReservations(db: SqlDatabase, saleId: string) { return application.releaseUnpaidReservations(unit(db), saleId); }
