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
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { snapshotMoney, snapshotMoneyCarry, snapshotMoneyReversal } from "@erpx/server-finance/public/company-money";
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
import { posCoreFilesFacts } from "@erpx/server-files/public/pos-core-facts";
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
        saveReversalRequest: async row => { await run(db, "INSERT INTO pos_room_charge_reversal_requests(id,sale_id,company_id,branch_id,property_id,sale_version,property_version,amount_minor,reason,restock_line_ids_json,state,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,'requested',?,?,?)", row.id,row.sale_id,row.company_id,row.branch_id,row.property_id,row.sale_version,row.property_version,row.amount_minor,row.reason,row.restock_line_ids_json,row.requested_by,row.created_at,row.created_at); },
        reviewReversalRequest: async (id,state,user,reason,now) => { await run(db, "UPDATE pos_room_charge_reversal_requests SET state=?,version=version+1,pos_approved_by=CASE WHEN ?='pos_approved' THEN ? ELSE pos_approved_by END,pms_approved_by=CASE WHEN ?='approved' THEN ? ELSE pms_approved_by END,rejected_by=CASE WHEN ?='rejected' THEN ? ELSE rejected_by END,review_reason=?,updated_at=? WHERE id=?", state,state,user,state,user,state,user,reason,now,id); },
        prepareAccounting: async (company,accounts,user) => { await run(db, 'UPDATE pos_settings SET accounts_json=?,accounting_prepared_by=?,accounting_approved_by=NULL WHERE company_id=?', JSON.stringify(accounts),user,company); },
        approveAccounting: async (company,user) => { await run(db, 'UPDATE pos_settings SET accounting_approved_by=?,version=version+1 WHERE company_id=?',user,company); },
        saveOperationalSettings: async (company,enabled,discount) => { await run(db, 'UPDATE pos_settings SET enabled=?,cashier_discount_bps=?,version=version+1 WHERE company_id=?',enabled,discount,company); },
        linkRegister: async (id,propertyId) => { await run(db, 'UPDATE pos_registers SET property_id=? WHERE id=?',propertyId,id); },
        readPosSales1: (facts: {
            saleId: string;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE', facts.saleId),
        readPosSettings1: (facts: {
            companyId: string;
        }) => one(db, 'SELECT * FROM pos_settings WHERE company_id=?', facts.companyId),
        readFinancePeriods1: facts => posCoreFinanceFacts(db).readFinancePeriods1(facts),
        readPosPostings1: (facts: {
            sourceKey: string;
        }) => one(db, 'SELECT journal_id FROM pos_postings WHERE source_key=?', facts.sourceKey),
        readFinanceCharts1: facts => posCoreFinanceFacts(db).readFinanceCharts1(facts),
        readFinanceAccounts1: facts => posCoreFinanceFacts(db).readFinanceAccounts1(facts),
        writeFinanceJournals1: facts => posCoreFinanceFacts(db).writeFinanceJournals1(facts),
        writeFinanceJournalLines1: facts => posCoreFinanceFacts(db).writeFinanceJournalLines1(facts),
        writePosPostings1: (facts: {
            sourceKey: string;
            companyId: string;
            journal: `${string}-${string}-${string}-${string}-${string}`;
        }) => run(db, 'INSERT INTO pos_postings VALUES(?,?,?)', facts.sourceKey, facts.companyId, facts.journal),
        writePosAudit1: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            companyId: string;
            branchId: string | null;
            actorId: string;
            operation: string;
            recordId: string;
            fact6: string;
            fact7: number;
        }) => run(db, 'INSERT INTO pos_audit VALUES(?,?,?,?,?,?,?,?)', facts.fact0, facts.companyId, facts.branchId, facts.actorId, facts.operation, facts.recordId, facts.fact6, facts.fact7),
        writePosJobs1: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            companyId: string;
            kind: string;
            recordId: string;
            fact4: string;
            fact5: number;
        }) => run(db, "INSERT INTO pos_jobs(id,company_id,kind,record_id,payload_json,created_at) VALUES(?,?,?,?,?,?) ON CONFLICT(kind,record_id) DO NOTHING", facts.fact0, facts.companyId, facts.kind, facts.recordId, facts.fact4, facts.fact5),
        readPosPayments1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT id FROM pos_payments WHERE sale_id=? AND (state IN ('requested','pending','exception') OR state='succeeded' AND amount_minor>refunded_minor) LIMIT 1", facts.id),
        writePosSales1: (facts: {
            saleId: string;
            saleId2: string;
            saleId3: string;
            saleId4: string;
            fact4: number;
            saleId5: string;
            clearApproval: boolean;
        }) => run(db, `UPDATE pos_sales SET subtotal_minor=(SELECT COALESCE(SUM(subtotal_minor),0) FROM pos_lines WHERE sale_id=?), tax_minor=(SELECT COALESCE(SUM(tax_minor),0) FROM pos_lines WHERE sale_id=?),discount_minor=(SELECT COALESCE(SUM(discount_minor),0) FROM pos_lines WHERE sale_id=?),total_minor=(SELECT COALESCE(SUM(subtotal_minor+tax_minor),0) FROM pos_lines WHERE sale_id=?),version=version+1,updated_at=?${facts.clearApproval ? ',discount_approved_by=NULL,discount_reason=NULL' : ''} WHERE id=?`, facts.saleId, facts.saleId2, facts.saleId3, facts.saleId4, facts.fact4, facts.saleId5),
        readCommerceStock1: facts => posCoreCommerceFacts(db).readCommerceStock1(facts),
        readInventoryActiveReservations1: facts => posCoreInventoryFacts(db).readInventoryActiveReservations1(facts),
        writePosReservations1: (facts: {
            id: string | number | boolean | null;
            warehouse_id: string | number | boolean | null;
            productId: string;
            quantityMilli: number;
        }) => run(db, 'INSERT INTO pos_reservations VALUES(?,?,?,?)', facts.id, facts.warehouse_id, facts.productId, facts.quantityMilli),
        writePosFulfillments1: (facts: {
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_fulfillments SET reserved=true WHERE id=?', facts.id),
        readCommerceStock2: facts => posCoreCommerceFacts(db).readCommerceStock2(facts),
        readInventoryLedger1: facts => posCoreInventoryFacts(db).readInventoryLedger1(facts),
        writePosReservations2: (facts: {
            id: string | number | boolean | null;
            productId: string;
        }) => run(db, 'DELETE FROM pos_reservations WHERE fulfillment_id=? AND product_id=?', facts.id, facts.productId),
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
        }) => run(db, 'INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)', facts.fact0, facts.company_id, facts.warehouse_id, facts.productId, facts.id, facts.id2, facts.fact6, facts.fact7, facts.fact8, facts.fact9, facts.business_date, facts.fact11),
        writePosFulfillments2: (facts: {
            cost: number;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_fulfillments SET reserved=false,cost_minor=? WHERE id=?', facts.cost, facts.id),
        readPosSales2: (facts: {
            sale_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE', facts.sale_id),
        writeFinancePayments1: facts => posCoreFinanceFacts(db).writeFinancePayments1(facts),
        writePosPayments1: (facts: {
            financeId: `${string}-${string}-${string}-${string}-${string}`;
            fact1: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_payments SET state='succeeded',finance_payment_id=?,updated_at=?,version=version+1 WHERE id=?", facts.financeId, facts.fact1, facts.id),
        writePosSales2: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_sales SET state='payment_pending',version=version+1,updated_at=? WHERE id=?", facts.fact0, facts.id),
        readPosLines1: (facts: {
            id: string | number | boolean | null;
        }) => all(db, 'SELECT * FROM pos_lines WHERE sale_id=?', facts.id),
        readPosPayments2: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT COALESCE(SUM(amount_minor-refunded_minor),0) amount FROM pos_payments WHERE sale_id=? AND state='succeeded'", facts.id),
        readPosPayments3: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT id FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','exception')", facts.id),
        readPosFulfillments1: (facts: {
            itemId: string;
        }) => one(db, 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE', facts.itemId),
        writePosFulfillments3: (facts: {
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_fulfillments SET state='delivered',version=version+1 WHERE id=?", facts.id),
        writeFinanceParties1: facts => posCoreFinanceFacts(db).writeFinanceParties1(facts),
        writeFinanceInvoices1: facts => posCoreFinanceFacts(db).writeFinanceInvoices1(facts),
        readPosPayments4: (facts: {
            id: string | number | boolean | null;
        }) => all(db, "SELECT * FROM pos_payments WHERE sale_id=? AND state='succeeded' AND amount_minor>refunded_minor", facts.id),
        writeFinancePayments2: facts => posCoreFinanceFacts(db).writeFinancePayments2(facts),
        writeFinanceAllocations1: facts => posCoreFinanceFacts(db).writeFinanceAllocations1(facts),
        writePosSales3: (facts: {
            invoice: `${string}-${string}-${string}-${string}-${string}`;
            party: string | number | true;
            fact2: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_sales SET state='completed',invoice_id=?,party_id=?,table_id=NULL,version=version+1,updated_at=? WHERE id=?", facts.invoice, facts.party, facts.fact2, facts.id),
        readAssignments1: facts => posCoreIdentityAccessFacts(db).readAssignments1(facts),
        writeContext1: (facts: {
            companyId: string;
        }) => run(db, "SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))", facts.companyId),
        readPosReceipts1: (facts: {
            key: string;
        }) => one(db, 'SELECT * FROM pos_receipts WHERE id=?', facts.key),
        writeCommandAssertions1: facts => posCoreWorkflowFacts(db).writeCommandAssertions1(facts),
        readAttachmentSets1: facts => posCoreFilesFacts(db).readAttachmentSets1(facts),
        writePosReceipts1: (facts: {
            key: string;
            companyId: string;
            hash: string;
            fact3: string;
            fact4: number;
        }) => run(db, 'INSERT INTO pos_receipts VALUES(?,?,?,?,?)', facts.key, facts.companyId, facts.hash, facts.fact3, facts.fact4),
        writeCommandAssertions2: facts => posCoreWorkflowFacts(db).writeCommandAssertions2(facts),
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
        }) => one(db, 'SELECT id FROM pos_registers WHERE company_id=? AND branch_id=? AND name=?', facts.company, facts.branchId, facts.registerName),
        writePosSettings1: (facts: {
            company: string;
            currency: string;
            fact2: string;
        }) => run(db, "INSERT INTO pos_settings(company_id,enabled,currency,timezone,accounts_json,cashier_discount_bps) VALUES(?,true,?,'Asia/Bangkok',?,0)", facts.company, facts.currency, facts.fact2),
        writePosRegisters1: (facts: {
            registerId: string;
            company: string;
            branchId: string;
            warehouseId: string;
            registerName: string;
            mode: "retail" | "restaurant";
        }) => run(db, "INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES(?,?,?,?,?,?,true)", facts.registerId, facts.company, facts.branchId, facts.warehouseId, facts.registerName, facts.mode),
        writePosOauthStates1: (facts: {
            state: `${string}-${string}-${string}-${string}-${string}`;
            company: string;
            user: string;
            fact3: number;
            livemode: boolean;
        }) => run(db, 'INSERT INTO pos_oauth_states(id,company_id,user_id,expires_at,livemode) VALUES(?,?,?,?,?)', facts.state, facts.company, facts.user, facts.fact3, facts.livemode),
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
        }) => run(db, 'INSERT INTO pos_settings(company_id,enabled,currency,timezone,accounts_json,cashier_discount_bps) VALUES(?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,accounts_json=excluded.accounts_json,cashier_discount_bps=excluded.cashier_discount_bps,version=pos_settings.version+1', facts.company, facts.enabled, facts.currency, facts.timezone, facts.fact4, facts.cashierDiscountBps),
        readCommerceWarehouses3: facts => posCoreCommerceFacts(db).readCommerceWarehouses3(facts),
        readPosRegisters3: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=? FOR UPDATE', facts.id),
        readPosShifts1: (facts: {
            id: string;
        }) => one(db, "SELECT id FROM pos_shifts WHERE register_id=? AND state='open'", facts.id),
        writePosRegisters2: (facts: {
            warehouseId: string;
            branch_id: string | number | boolean | null;
            name: string;
            mode: "retail" | "restaurant";
            active: boolean;
            rid: string;
        }) => run(db, 'UPDATE pos_registers SET warehouse_id=?,branch_id=?,name=?,mode=?,active=?,version=version+1 WHERE id=?', facts.warehouseId, facts.branch_id, facts.name, facts.mode, facts.active, facts.rid),
        writePosRegisters3: (facts: {
            rid: string;
            company: string;
            branch_id: string | number | boolean | null;
            warehouseId: string;
            name: string;
            mode: "retail" | "restaurant";
            active: boolean;
        }) => run(db, 'INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES(?,?,?,?,?,?,?)', facts.rid, facts.company, facts.branch_id, facts.warehouseId, facts.name, facts.mode, facts.active),
        readPosRegisters4: (facts: {
            registerId: string;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=? FOR UPDATE', facts.registerId),
        readPosTables1: (facts: {
            registerId: string;
            name: string;
        }) => one(db, 'SELECT id FROM pos_tables WHERE register_id=? AND name=?', facts.registerId, facts.name),
        writePosTables1: (facts: {
            tid: `${string}-${string}-${string}-${string}-${string}`;
            registerId: string;
            name: string;
            seats: number;
        }) => run(db, 'INSERT INTO pos_tables VALUES(?,?,?,?)', facts.tid, facts.registerId, facts.name, facts.seats),
        readPosCatalog1: (facts: {
            productId: string;
            company: string;
        }) => one(db, 'SELECT * FROM pos_catalog WHERE product_id=? AND company_id=? FOR UPDATE', facts.productId, facts.company),
        readPosRegisterCatalog1: (facts: {
            productId: string;
            branchId: string;
            company: string;
        }) => one(db, 'SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=? AND r.branch_id=? AND r.company_id=?', facts.productId, facts.branchId, facts.company),
        writePosCatalog1: (facts: {
            imageId: string;
            productId: string;
        }) => run(db, 'UPDATE pos_catalog SET image_id=?,version=version+1 WHERE product_id=?', facts.imageId, facts.productId),
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
        }) => all(db, "SELECT id FROM pos_registers WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?<>'recipe' OR mode='restaurant')", facts.company, facts.branchId, facts.branchId2, facts.kind),
        readPosRegisters6: (facts: {
            rid: string;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=?', facts.rid),
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
        }) => run(db, 'INSERT INTO pos_catalog(product_id,company_id,barcode,category,kind,station,recipe_json,modifiers_json,tax_inclusive,enabled,image_id) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(product_id) DO UPDATE SET barcode=excluded.barcode,category=excluded.category,kind=excluded.kind,station=excluded.station,recipe_json=excluded.recipe_json,modifiers_json=excluded.modifiers_json,tax_inclusive=excluded.tax_inclusive,enabled=excluded.enabled,image_id=excluded.image_id,version=pos_catalog.version+1', facts.productId, facts.company, facts.fact2, facts.category, facts.kind, facts.station, facts.fact6, facts.fact7, facts.taxInclusive, facts.enabled, facts.fact10),
        writePosRegisterCatalog1: (facts: {
            productId: string;
            company: string;
            branchId: string;
        }) => run(db, 'DELETE FROM pos_register_catalog WHERE product_id=? AND register_id IN (SELECT id FROM pos_registers WHERE company_id=? AND branch_id=?)', facts.productId, facts.company, facts.branchId),
        writePosRegisterCatalog2: (facts: {
            productId: string;
        }) => run(db, 'DELETE FROM pos_register_catalog WHERE product_id=?', facts.productId),
        writePosRegisterCatalog3: (facts: {
            rid: string;
            productId: string;
        }) => run(db, 'INSERT INTO pos_register_catalog(register_id,product_id) VALUES(?,?)', facts.rid, facts.productId),
        readPosRegisters7: (facts: {
            registerId: string;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=? AND active=true FOR UPDATE', facts.registerId),
        readPmsProperties1: (facts: {
            company: string;
            branch_id: string | number | boolean | null;
        }) => one(db, `SELECT state,business_date FROM (${pmsProjectionSources.pms_properties}) pms_properties WHERE company_id=? AND branch_id=? FOR SHARE`, facts.company, facts.branch_id),
        writePosShifts1: (facts: {
            sid: `${string}-${string}-${string}-${string}-${string}`;
            id: string | number | boolean | null;
            company: string;
            branch_id: string | number | boolean | null;
            user: string;
            fact5: number;
            openingMinor: number;
            fact7: string;
        }) => run(db, 'INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,business_date) VALUES(?,?,?,?,?,?,?,?)', facts.sid, facts.id, facts.company, facts.branch_id, facts.user, facts.fact5, facts.openingMinor, facts.fact7),
        readPosShifts2: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_shifts WHERE id=? FOR UPDATE', facts.id),
        writePosCashMovements1: (facts: {
            mid: `${string}-${string}-${string}-${string}-${string}`;
            id: string | number | boolean | null;
            amountMinor: number;
            reason: string;
            user: string;
            null: null;
            fact6: number;
        }) => run(db, 'INSERT INTO pos_cash_movements VALUES(?,?,?,?,?,?,?)', facts.mid, facts.id, facts.amountMinor, facts.reason, facts.user, facts.null, facts.fact6),
        writePosShifts2: (facts: {
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_shifts SET version=version+1 WHERE id=?', facts.id),
        readPosCashMovements1: (facts: {
            movementId: string | undefined;
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_cash_movements WHERE id=? AND shift_id=? FOR UPDATE', facts.movementId, facts.id),
        writePosCashMovements2: (facts: {
            user: string;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_cash_movements SET approved_by=? WHERE id=?', facts.user, facts.id),
        readPosSales3: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT id FROM pos_sales WHERE shift_id=? AND state IN ('open','held','payment_pending')", facts.id),
        readPosCashMovements2: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT id FROM pos_cash_movements WHERE shift_id=? AND approved_by IS NULL', facts.id),
        readPosPayments5: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT COALESCE(SUM(p.amount_minor),0) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.shift_id=? AND p.method='cash' AND p.state='succeeded'", facts.id),
        readPosRefundPayments1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT COALESCE(SUM(rp.amount_minor),0) n FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id JOIN pos_refunds r ON r.id=rp.refund_id WHERE r.payout_shift_id=? AND p.method='cash' AND rp.state='succeeded'", facts.id),
        readPosCashMovements3: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT COALESCE(SUM(amount_minor),0) n FROM pos_cash_movements WHERE shift_id=? AND approved_by IS NOT NULL', facts.id),
        writePosShifts3: (facts: {
            countedMinor: number;
            expected: number;
            fact2: number;
            fact3: number;
            user: string;
            fact5: string | null;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_shifts SET state='closed',counted_minor=?,expected_minor=?,variance_minor=?,closed_at=?,closed_by=?,denominations_json=?::jsonb WHERE id=?", facts.countedMinor, facts.expected, facts.fact2, facts.fact3, facts.user, facts.fact5, facts.id),
        writePosShifts4: (facts: {
            user: string;
            reason: string;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_shifts SET state='reviewed',reviewed_by=?,review_reason=? WHERE id=?", facts.user, facts.reason, facts.id),
        writePosShifts5: (facts: {
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_shifts SET version=version+1 WHERE id=?', facts.id),
        readPosShifts3: (facts: {
            shiftId: string;
        }) => one(db, "SELECT * FROM pos_shifts WHERE id=? AND state='open' FOR UPDATE", facts.shiftId),
        readCompanyFxRates1: facts => posCoreFinanceFacts(db).readCompanyFxRates1(facts),
        readPosRegisters8: (facts: {
            register_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=? AND active=true', facts.register_id),
        readPosTables2: (facts: {
            tableId: string;
            id: string | number | boolean | null;
        }) => one(db, 'SELECT id FROM pos_tables WHERE id=? AND register_id=?', facts.tableId, facts.id),
        readFinanceParties1: facts => posCoreFinanceFacts(db).readFinanceParties1(facts),
        writePosSales4: (facts: {
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
        }) => run(db, 'INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,party_id,table_id,guests,service_mode,created_by,created_at,updated_at,accounts_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)', facts.sid, facts.company, facts.branch_id, facts.id, facts.id2, facts.fact5, facts.currency, facts.fact7, facts.fact8, facts.fact9, facts.guests, facts.serviceMode, facts.user, facts.fact13, facts.fact14, facts.accounts_json),
        readOrganizationNodes2: facts => posCoreOrganizationFacts(db).readOrganizationNodes2(facts),
        writePosSales5: (facts: {
            fact0: string;
            sid: `${string}-${string}-${string}-${string}-${string}`;
        }) => run(db, 'UPDATE pos_sales SET issuer_json=? WHERE id=?', facts.fact0, facts.sid),
        readPosDayCloses1: (facts: {
            company_id: string | number | boolean | null;
            branch_id: string | number | boolean | null;
            business_date: string | number | boolean | null;
        }) => one(db, "SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=? AND state='prepared'", facts.company_id, facts.branch_id, facts.business_date),
        readPosAudit1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT actor_id FROM pos_audit WHERE operation='sale.charge_to_room' AND record_id=? ORDER BY created_at DESC LIMIT 1", facts.id),
        readPosRoomChargeReversals1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT sale_id FROM pos_room_charge_reversals WHERE sale_id=?', facts.id),
        readPosPayments6: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT id FROM pos_payments WHERE sale_id=?', facts.id),
        readPmsSources1: (facts: {
            fact0: string;
            company: string;
            branch_id: string | number | boolean | null;
        }) => one(db, `SELECT ps.property_id,ps.record_id FROM (${pmsProjectionSources.pms_sources}) ps JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=ps.property_id WHERE ps.source_key=? AND p.company_id=? AND p.branch_id=?`, facts.fact0, facts.company, facts.branch_id),
        readPosLines2: (facts: {
            id: string | number | boolean | null;
        }) => all(db, 'SELECT l.*,f.kind,f.state fulfillment_state,f.cost_minor fulfillment_cost,f.quantity_milli fulfillment_quantity,f.warehouse_id,f.product_id FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id', facts.id),
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
        }) => run(db, 'INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)', facts.fact0, facts.company, facts.warehouse_id, facts.product_id, facts.fulfillment_id, facts.id, facts.number, facts.fact7, facts.quantity_milli, facts.cost, facts.date, facts.fact11),
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
        }) => run(db, 'INSERT INTO pos_room_charge_reversals(sale_id,company_id,property_id,original_entry_id,reversal_entry_id,reason,restocked_lines_json,restored_cost_minor,wasted_cost_minor,reversed_by,reversed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)', facts.id, facts.company, facts.propertyId, facts.entryId, facts.reversalEntryId, facts.reason, facts.fact6, facts.restored, facts.wasted, facts.user, facts.fact10),
        writePosSales6: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_sales SET version=version+1,updated_at=? WHERE id=?', facts.fact0, facts.id),
        readPosRegisters9: (facts: {
            register_id: string | number | boolean | null;
        }) => one(db, "SELECT id,property_id FROM pos_registers WHERE id=? AND mode='restaurant'", facts.register_id),
        readPosShifts4: (facts: {
            shift_id: string | number | boolean | null;
        }) => one(db, "SELECT id FROM pos_shifts WHERE id=? AND state='open'", facts.shift_id),
        readPosPayments7: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT id FROM pos_payments WHERE sale_id=? LIMIT 1', facts.id),
        readPosLines4: (facts: {
            id: string | number | boolean | null;
        }) => all(db, 'SELECT * FROM pos_lines WHERE sale_id=?', facts.id),
        readPosLines5: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0) n FROM pos_lines WHERE sale_id=?', facts.id),
        readPmsProperties2: (facts: {
            propertyId: string;
            company: string;
        }) => one(db, `SELECT branch_id,business_date,version,state FROM (${pmsProjectionSources.pms_properties}) pms_properties WHERE id=? AND company_id=?`, facts.propertyId, facts.company),
        readPosFulfillments2: (facts: {
            itemId: string;
        }) => one(db, 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE', facts.itemId),
        writePosFulfillments4: (facts: {
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_fulfillments SET state='delivered',version=version+1 WHERE id=?", facts.id),
        writePosSales7: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_sales SET state='completed',table_id=NULL,version=version+1,updated_at=? WHERE id=?", facts.fact0, facts.id),
        writePosSales8: (facts: {
            user: string;
            reason: string;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_sales SET discount_approved_by=?,discount_reason=?,version=version+1 WHERE id=?', facts.user, facts.reason, facts.id),
        writePosSales9: (facts: {
            user: string;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_sales SET discount_requested_by=? WHERE id=?', facts.user, facts.id),
        readCommerceProducts3: facts => posCoreCommerceFacts(db).readCommerceProducts3(facts),
        readPosRegisters10: (facts: {
            register_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=?', facts.register_id),
        writePosFulfillments5: (facts: {
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
        }) => run(db, 'INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,note,recipe_json,accounts_json,currency,business_date,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)', facts.fid, facts.company, facts.branch_id, facts.warehouse_id, facts.productId, facts.quantityMilli, facts.kind, facts.fact7, facts.note, facts.fact9, facts.accounts_json, facts.currency, facts.business_date, facts.fact13),
        writePosLines1: (facts: {
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
        }) => run(db, 'INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)', facts.lid, facts.id, facts.fid, facts.productId, facts.name, facts.quantityMilli, facts.price, facts.tax_bps, facts.fact8, facts.discountMinor, facts.subtotal, facts.tax, facts.fact12, facts.note),
        readFinanceParties2: facts => posCoreFinanceFacts(db).readFinanceParties2(facts),
        writePosSales10: (facts: {
            partyId: string | null;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_sales SET party_id=? WHERE id=?', facts.partyId, facts.id),
        readPosLines6: (facts: {
            lineId: string;
            id: string | number | boolean | null;
        }) => one(db, 'SELECT l.*,f.state fulfillment_state,f.quantity_milli fulfillment_quantity FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.id=? AND l.sale_id=? FOR UPDATE OF l,f', facts.lineId, facts.id),
        writePosFulfillments6: (facts: {
            quantityMilli: number;
            note: string;
            fulfillment_id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_fulfillments SET quantity_milli=?,note=?,version=version+1 WHERE id=?', facts.quantityMilli, facts.note, facts.fulfillment_id),
        writePosLines2: (facts: {
            quantityMilli: number;
            discount: number;
            subtotal: number;
            tax: number;
            note: string;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_lines SET quantity_milli=?,discount_minor=?,subtotal_minor=?,tax_minor=?,note=? WHERE id=?', facts.quantityMilli, facts.discount, facts.subtotal, facts.tax, facts.note, facts.id),
        writePosSales11: (facts: {
            user: string;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_sales SET discount_requested_by=? WHERE id=?', facts.user, facts.id),
        readPosLines7: (facts: {
            id: string | number | boolean | null;
        }) => all(db, 'SELECT * FROM pos_lines WHERE sale_id=? ORDER BY id FOR UPDATE', facts.id),
        writePosLines3: (facts: {
            discount: number;
            extra: number;
            subtotal: number;
            tax: number;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_lines SET discount_minor=?,bill_discount_minor=?,subtotal_minor=?,tax_minor=? WHERE id=?', facts.discount, facts.extra, facts.subtotal, facts.tax, facts.id),
        writePosSales12: (facts: {
            user: string;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_sales SET discount_requested_by=? WHERE id=?', facts.user, facts.id),
        readPosLines8: (facts: {
            lineId: string;
            id: string | number | boolean | null;
        }) => one(db, 'SELECT l.*,f.state fulfillment_state FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.id=? AND l.sale_id=? FOR UPDATE OF l,f', facts.lineId, facts.id),
        writePosReservations3: (facts: {
            fulfillment_id: string | number | boolean | null;
        }) => run(db, 'DELETE FROM pos_reservations WHERE fulfillment_id=?', facts.fulfillment_id),
        writePosLines4: (facts: {
            id: string | number | boolean | null;
        }) => run(db, 'DELETE FROM pos_lines WHERE id=?', facts.id),
        writePosSales13: (facts: {
            fact0: "held" | "open";
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_sales SET state=? WHERE id=?', facts.fact0, facts.id),
        readPosTables3: (facts: {
            tableId: string;
            register_id: string | number | boolean | null;
        }) => one(db, 'SELECT id FROM pos_tables WHERE id=? AND register_id=?', facts.tableId, facts.register_id),
        writePosSales14: (facts: {
            tableId: string;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_sales SET table_id=? WHERE id=?', facts.tableId, facts.id),
        writePosLines5: (facts: {
            id: string | number | boolean | null;
            id2: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_lines SET sale_id=? WHERE sale_id=?', facts.id, facts.id2),
        writePosSales15: (facts: {
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_sales SET state='merged',table_id=NULL WHERE id=?", facts.id),
        readPosLines9: (facts: {
            lineId: string;
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_lines WHERE id=? AND sale_id=? FOR UPDATE', facts.lineId, facts.id),
        readPosFulfillments3: (facts: {
            fid: string;
        }) => one(db, 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE', facts.fid),
        writePosSales16: (facts: {
            target: `${string}-${string}-${string}-${string}-${string}`;
            fact1: string;
            user: string;
            fact3: number;
            fact4: number;
            id: string | number | boolean | null;
        }) => run(db, "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,party_id,guests,service_mode,created_by,created_at,updated_at,accounts_json) SELECT ?,company_id,branch_id,register_id,shift_id,?,currency,business_date,party_id,guests,service_mode,?,?,?,accounts_json FROM pos_sales WHERE id=?", facts.target, facts.fact1, facts.user, facts.fact3, facts.fact4, facts.id),
        writePosLines6: (facts: {
            target: `${string}-${string}-${string}-${string}-${string}`;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_lines SET sale_id=? WHERE id=?', facts.target, facts.id),
        writePosFulfillments7: (facts: {
            newFid: string;
            quantityMilli: number;
            fact2: number;
            fid: string;
        }) => run(db, 'INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,note,recipe_json,accounts_json,currency,business_date,created_at) SELECT ?,company_id,branch_id,warehouse_id,product_id,?,kind,station,note,recipe_json,accounts_json,currency,business_date,? FROM pos_fulfillments WHERE id=?', facts.newFid, facts.quantityMilli, facts.fact2, facts.fid),
        writePosFulfillments8: (facts: {
            quantityMilli: number;
            fid: string;
        }) => run(db, 'UPDATE pos_fulfillments SET quantity_milli=quantity_milli-?,version=version+1 WHERE id=?', facts.quantityMilli, facts.fid),
        writePosLines7: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            target: `${string}-${string}-${string}-${string}-${string}`;
            newFid: string;
            quantityMilli: number;
            discount: number;
            billDiscount: number;
            amount: number;
            tax: number;
            id: string | number | boolean | null;
        }) => run(db, 'INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,bill_discount_minor,subtotal_minor,tax_minor,modifiers_json,note) SELECT ?,?,?,product_id,name,?,price_minor,tax_bps,tax_inclusive,?,?,?,?,modifiers_json,note FROM pos_lines WHERE id=?', facts.fact0, facts.target, facts.newFid, facts.quantityMilli, facts.discount, facts.billDiscount, facts.amount, facts.tax, facts.id),
        writePosLines8: (facts: {
            quantityMilli: number;
            amount: number;
            tax: number;
            discount: number;
            billDiscount: number;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_lines SET quantity_milli=quantity_milli-?,subtotal_minor=subtotal_minor-?,tax_minor=tax_minor-?,discount_minor=discount_minor-?,bill_discount_minor=bill_discount_minor-? WHERE id=?', facts.quantityMilli, facts.amount, facts.tax, facts.discount, facts.billDiscount, facts.id),
        writePosSales17: (facts: {
            issuer_json: string | number | boolean | null;
            user: string;
            target: `${string}-${string}-${string}-${string}-${string}`;
        }) => run(db, 'UPDATE pos_sales SET issuer_json=?,discount_requested_by=? WHERE id=?', facts.issuer_json, facts.user, facts.target),
        readPosFulfillments4: (facts: {
            id: string | number | boolean | null;
        }) => all(db, 'SELECT DISTINCT f.* FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?', facts.id),
        readPosLines10: (facts: {
            id: string | number | boolean | null;
            id2: string | number | boolean | null;
        }) => one(db, 'SELECT id FROM pos_lines WHERE fulfillment_id=? AND sale_id<>?', facts.id, facts.id2),
        writePosReservations4: (facts: {
            id: string | number | boolean | null;
        }) => run(db, 'DELETE FROM pos_reservations WHERE fulfillment_id=?', facts.id),
        writePosFulfillments9: (facts: {
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_fulfillments SET state='cancelled',reserved=false,version=version+1 WHERE id=?", facts.id),
        writePosSales18: (facts: {
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_sales SET state='cancelled',table_id=NULL WHERE id=?", facts.id),
        readPosSales4: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT version FROM pos_sales WHERE id=?', facts.id),
        readPosFulfillments5: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE', facts.id),
        readPosLines11: (facts: {
            id: string | number | boolean | null;
            user: string;
        }) => one(db, 'SELECT s.created_by FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.created_by=? LIMIT 1', facts.id, facts.user),
        readPosAudit2: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT actor_id FROM pos_audit WHERE operation='kitchen.start' AND record_id=? ORDER BY created_at DESC LIMIT 1", facts.id),
        readPosLines12: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT l.id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.state='completed' AND l.returned_milli<l.quantity_milli", facts.id),
        writePosReservations5: (facts: {
            id: string | number | boolean | null;
        }) => run(db, 'DELETE FROM pos_reservations WHERE fulfillment_id=?', facts.id),
        writePosFulfillments10: (facts: {
            state: string;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_fulfillments SET state=?,reserved=false,version=version+1 WHERE id=?', facts.state, facts.id),
        writePosFulfillments11: (facts: {
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_fulfillments SET reserved=true WHERE id=?', facts.id),
        readPosShifts5: (facts: {
            shift_id: string | number | boolean | null;
        }) => one(db, "SELECT id FROM pos_shifts WHERE id=? AND state='open'", facts.shift_id),
        readPosLines13: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0) n FROM pos_lines WHERE sale_id=?', facts.id),
        readPosPayments8: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT COALESCE(SUM(amount_minor-refunded_minor),0) n FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','succeeded','exception')", facts.id),
        readPosFulfillments6: (facts: {
            id: string | number | boolean | null;
        }) => all(db, 'SELECT DISTINCT f.* FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?', facts.id),
        readPosConnections1: (facts: {
            company: string;
        }) => one(db, "SELECT * FROM pos_connections WHERE company_id=? AND state='ready'", facts.company),
        writePosPayments2: (facts: {
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
        }) => run(db, "INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,stripe_account,livemode,created_by,created_at,updated_at,payer_email) VALUES(?,?,?,?,?,?,?,'requested',?,?,?,?,?,?)", facts.pid, facts.id, facts.company, facts.method, facts.amountMinor, facts.tenderedMinor, facts.reference, facts.fact7, facts.fact8, facts.user, facts.fact10, facts.fact11, facts.fact12),
        writePosPayments3: (facts: {
            fact0: number;
            pid: `${string}-${string}-${string}-${string}-${string}`;
        }) => run(db, "UPDATE pos_payments SET state='pending',version=version+1,updated_at=? WHERE id=?", facts.fact0, facts.pid),
        readPosPayments9: (facts: {
            pid: `${string}-${string}-${string}-${string}-${string}`;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=?', facts.pid),
        writePosSales19: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_sales SET state='payment_pending',version=version+1,updated_at=? WHERE id=?", facts.fact0, facts.id),
        readPosPayments10: (facts: {
            id: string;
            company: string;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? AND company_id=? FOR UPDATE', facts.id, facts.company),
        readFinanceBankTransactions1: facts => posCoreFinanceFacts(db).readFinanceBankTransactions1(facts),
        readPosPayments11: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT finance_payment_id FROM pos_payments WHERE id=?', facts.id),
        writeFinanceBankTransactions1: facts => posCoreFinanceFacts(db).writeFinanceBankTransactions1(facts),
        readPosPayments12: (facts: {
            id: string;
            company: string;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? AND company_id=? FOR UPDATE', facts.id, facts.company),
        writePosPayments4: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=? WHERE id=?", facts.fact0, facts.id),
        writePosSales20: (facts: {
            fact0: number;
            sale_id: string | number | boolean | null;
            sale_id2: string | number | boolean | null;
        }) => run(db, "UPDATE pos_sales SET state='open',version=version+1,updated_at=? WHERE id=? AND NOT EXISTS(SELECT 1 FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','exception','succeeded'))", facts.fact0, facts.sale_id, facts.sale_id2),
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
        }) => run(db, 'UPDATE pos_payments SET payer_email=?,version=version+1,updated_at=? WHERE id=?', facts.payerEmail, facts.fact1, facts.id),
        writePosJobs2: (facts: {
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,last_error=NULL WHERE record_id=? AND kind IN ('checkout','check_payment') AND state='exception'", facts.id),
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
        }) => run(db, "UPDATE pos_settlements SET state=?,journal_id=?,version=version+1 WHERE id=?", facts.fact0, facts.journalId, facts.id),
        readPosSettlements3: (facts: {
            id: string;
            company: string;
        }) => one(db, "SELECT * FROM pos_settlements WHERE id=? AND company_id=? AND kind='payout' AND state='paid' FOR UPDATE", facts.id, facts.company),
        readFinanceBankMatchTargets1: facts => posCoreFinanceFacts(db).readFinanceBankMatchTargets1(facts),
        readFinanceBankTransactions2: facts => posCoreFinanceFacts(db).readFinanceBankTransactions2(facts),
        readFinanceJournalLines2: facts => posCoreFinanceFacts(db).readFinanceJournalLines2(facts),
        writeFinanceBankTransactions2: facts => posCoreFinanceFacts(db).writeFinanceBankTransactions2(facts),
        writePosSettlements2: (facts: {
            id: string | number | boolean | null;
            id2: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_settlements SET bank_transaction_id=?,version=version+1 WHERE id=?', facts.id, facts.id2),
        readPosJobs2: (facts: {
            id: string;
            company: string;
        }) => one(db, "SELECT * FROM pos_jobs WHERE id=? AND company_id=? AND state='exception' FOR UPDATE", facts.id, facts.company),
        readPosRefundPayments2: (facts: {
            providerId: string;
            record_id: string | number | boolean | null;
        }) => one(db, "UPDATE pos_refund_payments SET provider_id=?,state='pending' WHERE id=? AND provider_id IS NULL RETURNING id", facts.providerId, facts.record_id),
        readPosPayments14: (facts: {
            providerId: string;
            record_id: string | number | boolean | null;
        }) => one(db, "UPDATE pos_payments SET provider_id=?,state='pending' WHERE id=? AND provider_id IS NULL RETURNING id", facts.providerId, facts.record_id),
        writePosJobs3: (facts: {
            fact0: string;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,payload_json=? WHERE id=?", facts.fact0, facts.id),
        readPosJobs3: (facts: {
            id: string;
            company: string;
        }) => one(db, "SELECT * FROM pos_jobs WHERE id=? AND company_id=? AND state='exception' FOR UPDATE", facts.id, facts.company),
        readPosRefundPayments3: (facts: {
            record_id: string | number | boolean | null;
            company: string;
            company2: string;
        }) => one(db, "SELECT r.id,r.version,r.state refund_state,rp.state payout_state,rp.provider_id,p.method,p.payer_email FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND r.company_id=? AND p.company_id=? FOR UPDATE OF rp,r", facts.record_id, facts.company, facts.company2),
        writePosRefunds1: (facts: {
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_refunds SET version=version+1 WHERE id=?', facts.id),
        readPosRefundPayments4: (facts: {
            record_id: string | number | boolean | null;
        }) => one(db, "UPDATE pos_refund_payments SET provider_id=NULL,provider_attempt=provider_attempt+1,state='requested' WHERE id=? AND state='failed' RETURNING id", facts.record_id),
        writePosJobs4: (facts: {
            fact0: string;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,last_error=NULL,payload_json=? WHERE id=?", facts.fact0, facts.id),
        readPosDayCloses2: (facts: {
            company_id: string | number | boolean | null;
            branch_id: string | number | boolean | null;
            business_date: string | number | boolean | null;
        }) => one(db, "SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=? AND state='prepared'", facts.company_id, facts.branch_id, facts.business_date),
        readPmsSources2: (facts: {
            fact0: string;
        }) => one(db, `SELECT source_key FROM (${pmsProjectionSources.pms_sources}) pms_sources WHERE source_key=?`, facts.fact0),
        readPosLines14: (facts: {
            lineId: string;
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_lines WHERE id=? AND sale_id=?', facts.lineId, facts.id),
        writePosRefunds2: (facts: {
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
        }) => run(db, 'INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,payout_shift_id,kind,reason,lines_json,amount_minor,requested_by,created_at,accounting_date,tax_minor) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)', facts.rid, facts.id, facts.company_id, facts.branch_id, facts.fact4, facts.fact5, facts.reason, facts.fact7, facts.amount, facts.user, facts.fact10, facts.fact11, facts.tax),
        readPosRefunds1: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_refunds WHERE id=? FOR UPDATE', facts.id),
        writePosRefunds3: (facts: {
            user: string;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_refunds SET state='rejected',approved_by=?,version=version+1 WHERE id=?", facts.user, facts.id),
        readPosPayments15: (facts: {
            id: string | number | boolean | null;
        }) => all(db, "SELECT * FROM pos_payments WHERE sale_id=? AND state='succeeded' ORDER BY created_at DESC,id FOR UPDATE", facts.id),
        readPosLines15: (facts: {
            lineId: string;
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_lines WHERE id=? AND sale_id=? FOR UPDATE', facts.lineId, facts.id),
        readPosFulfillments7: (facts: {
            fulfillment_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE', facts.fulfillment_id),
        readPosStockMoves1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT COALESCE(SUM(quantity_milli),0) qty,COALESCE(SUM(value_minor),0) cost FROM pos_stock_moves WHERE fulfillment_id=? AND kind='return'", facts.id),
        writeCommerceStock3: facts => posCoreCommerceFacts(db).writeCommerceStock3(facts),
        writeCommerceStock4: facts => posCoreCommerceFacts(db).writeCommerceStock4(facts),
        writePosStockMoves3: (facts: {
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
        }) => run(db, 'INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)', facts.fact0, facts.company_id, facts.warehouse_id, facts.product_id, facts.id, facts.id2, facts.number, facts.fact7, facts.quantityMilli, facts.restoredCost, facts.date, facts.fact11),
        writePosLines9: (facts: {
            quantityMilli: number;
            fact1: number;
            taxMinor: number;
            restoredCost: number;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_lines SET returned_milli=returned_milli+?,refunded_minor=refunded_minor+?,refunded_tax_minor=refunded_tax_minor+?,returned_cost_minor=returned_cost_minor+? WHERE id=?', facts.quantityMilli, facts.fact1, facts.taxMinor, facts.restoredCost, facts.id),
        writePosRefundPayments1: (facts: {
            rpid: `${string}-${string}-${string}-${string}-${string}`;
            id: string | number | boolean | null;
            id2: string | number | boolean | null;
            amount: number;
            fact4: "cash" | "transfer" | null;
        }) => run(db, 'INSERT INTO pos_refund_payments(id,refund_id,payment_id,amount_minor,manual_method) VALUES(?,?,?,?,?)', facts.rpid, facts.id, facts.id2, facts.amount, facts.fact4),
        writePosPayments6: (facts: {
            amount: number;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_payments SET refunded_minor=refunded_minor+?,version=version+1 WHERE id=?', facts.amount, facts.id),
        readPosShifts6: (facts: {
            payout_shift_id: string | number | boolean | null;
            companyId: string;
            branch_id: string | number | boolean | null;
        }) => one(db, "SELECT * FROM pos_shifts WHERE id=? AND company_id=? AND branch_id=? AND state='open'", facts.payout_shift_id, facts.companyId, facts.branch_id),
        writePosRefunds4: (facts: {
            user: string;
            id: string | number | boolean | null;
            id2: string | number | boolean | null;
        }) => run(db, "UPDATE pos_refunds SET approved_by=?,state=CASE WHEN EXISTS(SELECT 1 FROM pos_refund_payments WHERE refund_id=? AND state<>'succeeded') THEN 'processing' ELSE 'completed' END,version=version+1 WHERE id=?", facts.user, facts.id, facts.id2),
        readPosRefundPayments5: (facts: {
            refundPaymentId: string;
        }) => one(db, 'SELECT * FROM pos_refund_payments WHERE id=? FOR UPDATE', facts.refundPaymentId),
        readPosRefunds2: (facts: {
            refund_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_refunds WHERE id=? FOR UPDATE', facts.refund_id),
        readPosPayments16: (facts: {
            payment_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=?', facts.payment_id),
        readPosSales5: (facts: {
            sale_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=?', facts.sale_id),
        writeFinancePayments3: facts => posCoreFinanceFacts(db).writeFinancePayments3(facts),
        writePosRefundPayments2: (facts: {
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_refund_payments SET state='succeeded' WHERE id=?", facts.id),
        writePosRefunds5: (facts: {
            id: string | number | boolean | null;
            id2: string | number | boolean | null;
        }) => run(db, "UPDATE pos_refunds SET state=CASE WHEN EXISTS(SELECT 1 FROM pos_refund_payments WHERE refund_id=? AND state<>'succeeded') THEN 'processing' ELSE 'completed' END,version=version+1 WHERE id=?", facts.id, facts.id2),
        readPosPayments17: (facts: {
            saleId: string;
        }) => one(db, "SELECT id FROM pos_payments WHERE sale_id=? AND (state IN ('requested','pending','exception') OR state='succeeded' AND amount_minor>refunded_minor)", facts.saleId),
        readPosRefundPayments6: (facts: {
            saleId: string;
        }) => one(db, "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE p.sale_id=? AND rp.state<>'succeeded'", facts.saleId),
        writePosReservations6: (facts: {
            saleId: string;
        }) => run(db, "DELETE FROM pos_reservations WHERE fulfillment_id IN (SELECT f.id FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=? AND f.state='draft')", facts.saleId),
        writePosFulfillments12: (facts: {
            saleId: string;
        }) => run(db, "UPDATE pos_fulfillments SET reserved=false WHERE state='draft' AND id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=?)", facts.saleId),
        writePosSales21: (facts: {
            saleId: string;
        }) => run(db, "UPDATE pos_sales SET state='open',version=version+1 WHERE id=? AND state='payment_pending'", facts.saleId),
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
