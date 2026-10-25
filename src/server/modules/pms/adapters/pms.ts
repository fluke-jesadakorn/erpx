import {decodePmsRecord,type PmsStoredRecord} from "../domain/stored-record";
import { roomChargeReversalWork } from "../../pos/public/room-charge-reversal-queue";
import { lastAuditOwnerProjectionSources as financeLastAuditProjectionSources } from "../../finance/public/last-audit-owner-projection-sources";
import { hospitalityOwnerProjectionSources as inventoryProjectionSources } from "../../inventory/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as marketingProjectionSources } from "../../marketing/public/hospitality-owner-projection-sources";
import "server-only";
import { preparePmsAccounting } from "@/server/modules/finance/public/hospitality-accounting";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import { type Effects } from "@/lib/pms-engine";
import { type Snapshot } from "@/server/modules/pms/public/rules/pms";
import { roomChargeSchema } from "@/lib/pms-room-charge";
import { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import { hospitalityDayControls } from "@/server/modules/pms/public/hospitality-day-controls";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { prepareJournal } from "@/server/modules/finance/public/finance";
import { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import { ApiError } from "@/server/modules/runtime/public/http";
import { snapshotMoney, snapshotMoneyBatch, snapshotMoneyCarry, snapshotMoneyReversal } from "@/server/modules/finance/public/company-money";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { readPmsInventoryCommand, readAllPmsRecords, readPmsCalendar, readPmsDashboard, readPmsDetail, readPmsSetupFacts, readPmsOperationsList, readPmsConfiguration, readPmsChoices } from "@/server/modules/pms/public/pms/read-repository";
import { type PmsReadSelection } from "@/lib/pms-read-selection";
import { isTransactionalSqlDatabase, type SqlDatabase, type TransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { SqlStatement } from "../../runtime/public/runtime/types";
import type { Kind } from "../domain/pms-model";
import { createPmsApplication, type PmsUnit, type PropertyRow, type TransferRequestRow, type ChargeCorrectionRow } from "../application/pms";
import { pmsOperationalWork } from "../public/pms/home";
const query = async <T>(db: SqlDatabase, sql: string, ...values: unknown[]) => (await db
    .prepare(sql)
    .bind(...values)
    .all<T>()).results;
const run = async (db: SqlDatabase, sql: string, ...values: unknown[]) => db
    .prepare(sql)
    .bind(...values)
    .run();
import { pmsIdentityAccessFacts } from "../../identity_access/public/pms-command-facts";
import { pmsOrganizationFacts } from "../../organization/public/pms-command-facts";
import { pmsFinanceFacts } from "../../finance/public/pms-command-facts";
import { pmsCommerceFacts } from "../../commerce/public/pms-command-facts";
import { pmsPosFacts } from "../../pos/public/pms-command-facts";
import { pmsWorkflowFacts } from "../../workflow/public/pms-command-facts";
import { pmsFilesFacts } from "../../files/public/pms-command-facts";
function unit(db: SqlDatabase): PmsUnit<SqlStatement> {
    const identity_access = pmsIdentityAccessFacts(db);
    const organization = pmsOrganizationFacts(db);
    const finance = pmsFinanceFacts(db);
    const commerce = pmsCommerceFacts(db);
    const pos = pmsPosFacts(db);
    const workflow = pmsWorkflowFacts(db);
    const files = pmsFilesFacts(db);
    return {
        readPmsProperties: (facts: {
            id: string;
            companyId: string;
            lock: boolean;
        }) => db.prepare(facts.lock
            ? `WITH command_context AS MATERIALIZED (SELECT set_config('erpx.pms_write','on',true),set_config('erpx.pms_property',?::text,true))
               SELECT p.* FROM pms_properties p CROSS JOIN command_context
               WHERE p.id=? AND p.company_id=? FOR UPDATE OF p`
            : "SELECT * FROM pms_properties WHERE id=? AND company_id=?")
            .bind(...(facts.lock ? [facts.id,facts.id,facts.companyId] : [facts.id,facts.companyId])).first<PropertyRow>(),
        readPmsProperties2: (facts: {
            companyId: string;
            fact1: readonly string[];
            actor: WorkContext;
        }) => query<PropertyRow>(db, `SELECT * FROM pms_properties WHERE company_id=?${facts.actor.branchId ? " AND branch_id=?" : ""} ORDER BY name`, facts.companyId, ...facts.fact1),
        readAssignments: facts => identity_access.readAssignments(facts),
        readOrganizationNodes: facts => organization.readOrganizationNodes(facts),
        readFinanceAccounts: facts => finance.readFinanceAccounts(facts),
        readFinanceCharts: facts => finance.readFinanceCharts(facts),
        readFinanceParties: facts => finance.readFinanceParties(facts),
        readCommerceProducts: facts => commerce.readCommerceProducts(facts),
        readCommerceWarehouses: facts => commerce.readCommerceWarehouses(facts),
        readPmsDocuments: (facts: {
            id: string;
            fact1: string | null;
            fact2: string | null;
        }) => query<{
            id: string;
            guest_account_id: string;
            kind: string;
            created_at: number;
        }>(db, "SELECT id,guest_account_id,kind,created_at FROM pms_documents WHERE property_id=? AND (?::jsonb IS NULL OR guest_account_id IN (SELECT jsonb_array_elements_text(?::jsonb))) ORDER BY created_at DESC,id LIMIT 200", facts.id, facts.fact1, facts.fact2),
        readFinanceJournalLines: facts => finance.readFinanceJournalLines(facts),
        readPmsRecords: (facts: {
            id: string;
        }) => db.prepare(`SELECT COALESCE(SUM((data_json::jsonb->>'due')::bigint),0)::double precision due,
      COALESCE(SUM((data_json::jsonb->>'advance')::bigint),0)::double precision advance,
      COALESCE(SUM((data_json::jsonb->>'deposit')::bigint),0)::double precision deposit
      FROM pms_records WHERE property_id=? AND kind='guest_account'`).bind(facts.id).first<{
            due: number;
            advance: number;
            deposit: number;
        }>(),
        readPmsTransferRequests: (facts: {
            companyId: string;
            id: string;
        }) => query<TransferRequestRow>(db, "SELECT * FROM pms_transfer_requests WHERE company_id=? AND property_id=? ORDER BY created_at DESC LIMIT 100", facts.companyId, facts.id),
        readPmsChargeCorrections: (facts: {
            companyId: string;
            id: string;
            fact2: readonly string[];
            frontMode: boolean;
        }) => query<{
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
        }>(db, `SELECT c.id,c.entry_id,c.guest_account_id,c.status,c.version,c.reason,c.requested_by,c.reviewed_by,c.review_reason,c.reversal_entry_id,c.created_at,
        e.data_json::jsonb->>'kind' entry_kind,e.data_json::jsonb->>'description' entry_description,(e.data_json::jsonb->>'total')::bigint entry_total
        FROM pms_charge_corrections c JOIN pms_records e ON e.id=c.entry_id AND e.property_id=c.property_id
        WHERE c.company_id=? AND c.property_id=? ${facts.frontMode ? " AND c.requested_by=?" : " AND c.status='pending'"} ORDER BY c.created_at DESC LIMIT 100`, facts.companyId, facts.id, ...facts.fact2),
        readPmsBookingEmails: (facts: {
            id: string;
        }) => query<{
            id: string;
            reservation_id: string;
            state: string;
            attempts: number;
            last_error: string | null;
        }>(db, "SELECT id,reservation_id,state,attempts,last_error FROM pms_booking_emails WHERE property_id=? AND state!='sent' ORDER BY created_at DESC LIMIT 50", facts.id),
        readPosConnections: facts => pos.readPosConnections(facts),
        readPmsStripeRefunds: propertyId => query(db,"SELECT id,property_id,guest_account_id,amount_minor,refund_amount_minor,refund_state,last_error,refund_reason,payer_email FROM pms_stripe_payments WHERE property_id=? AND refund_state IS NOT NULL ORDER BY updated_at DESC LIMIT 100",propertyId),
        readPmsStripePayments: (facts: {
            id: string;
        }) => query<{
            id: string;
            guest_account_id: string;
            state: string;
            kind: string;
            amount_minor: number;
            refund_state: string | null;
            payer_email: string | null;
        }>(db, "SELECT id,guest_account_id,state,kind,amount_minor,refund_state,payer_email FROM pms_stripe_payments WHERE property_id=? ORDER BY created_at DESC LIMIT 100", facts.id),
        readPmsRecords2: (facts: {
            companyId: string;
            propertyId: string;
        }) => query<{
            id: string;
            property_id: string;
            amount_minor: number;
            status: string;
            refund_state: string;
            created_at: number;
            last_error: string | null;
            needs_receipt_recovery: boolean;
        }>(db, "SELECT r.id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.created_at,r.last_error, (p.status='refund_review' AND r.state='requested' AND r.allocations_json::jsonb='[]'::jsonb AND NOT EXISTS(SELECT 1 FROM pms_records e WHERE e.property_id=p.property_id AND e.kind='entry' AND e.data_json::jsonb->>'sourceKey'='online-payment:'||p.id)) needs_receipt_recovery FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE p.company_id=? AND p.property_id=? ORDER BY r.created_at DESC LIMIT 100", facts.companyId,facts.propertyId),
        readFinanceBankTransactions: facts => finance.readFinanceBankTransactions(facts),
        readPosConnections2: facts => pos.readPosConnections2(facts),
        readFinanceCharts2: facts => finance.readFinanceCharts2(facts),
        readFinanceAccounts2: facts => finance.readFinanceAccounts2(facts),
        readFinanceAccounts3: facts => finance.readFinanceAccounts3(facts),
        readFinanceAccounts4: facts => finance.readFinanceAccounts4(facts),
        readFinancePeriods: facts => finance.readFinancePeriods(facts),
        insertPmsPostings: (facts: {
            id: `${string}-${string}-${string}-${string}-${string}`;
            id2: string;
            source: string;
            recordId: string;
            id3: string;
            date: string;
        }) => run(db, "INSERT INTO pms_postings(id,property_id,source_key,record_id,journal_id,business_date) VALUES(?,?,?,?,?,?)", facts.id, facts.id2, facts.source, facts.recordId, facts.id3, facts.date),
        deleteCommandAssertions: facts => workflow.deleteCommandAssertions(facts),
        readCommerceWarehouses2: facts => commerce.readCommerceWarehouses2(facts),
        readCommerceStock: facts => commerce.readCommerceStock(facts),
        readInventoryLedger: (facts: {
            warehouseId: string;
            productId: string;
            date: string;
        }) => db.prepare(`SELECT 1 FROM (${inventoryProjectionSources.inventory_ledger}) inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>? LIMIT 1`).bind(facts.warehouseId, facts.productId, facts.date).first(),
        updateCommerceStock: facts => commerce.updateCommerceStock(facts),
        insertPmsStockMoves: (facts: {
            id: string;
            id2: string;
            warehouseId: string;
            productId: string;
            fact4: number;
            fact5: number;
            date: string;
            fact7: number;
            description: string;
        }) => run(db, "INSERT INTO pms_stock_moves(id,property_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at,source_number) VALUES(?,?,?,?,?,?,?,?,?)", facts.id, facts.id2, facts.warehouseId, facts.productId, facts.fact4, facts.fact5, facts.date, facts.fact7, facts.description),
        insertPmsSources: (facts: {
            id: string;
            key: string;
            id2: string;
        }) => run(db, "INSERT INTO pms_sources(property_id,source_key,record_id) VALUES(?,?,?)", facts.id, facts.key, facts.id2),
        insertFinanceParties: facts => finance.insertFinanceParties(facts),
        readPmsPostings: (facts: {
            id: string;
            id2: string;
            id3: string;
        }) => db.prepare("SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='entry' AND data_json::jsonb->>'guestAccountId'=?) AND source_key LIKE '%:charge' ORDER BY business_date,source_key LIMIT 1").bind(facts.id, facts.id2, facts.id3).first<{
            journal_id: string;
        }>(),
        insertFinanceInvoices: facts => finance.insertFinanceInvoices(facts),
        readPmsPostings2: (facts: {
            id: string;
            entryId: string | undefined;
        }) => db.prepare("SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id=? ORDER BY source_key LIMIT 1").bind(facts.id, facts.entryId).first<{
            journal_id: string;
        }>(),
        insertFinancePayments: facts => finance.insertFinancePayments(facts),
        insertPmsDocuments: (facts: {
            id: string;
            id2: string;
            id3: string;
            kind: "refund" | "receipt" | "invoice" | "credit_note";
            financeId: string | null;
            fact5: string;
            now: number;
        }) => run(db, "INSERT INTO pms_documents(id,property_id,guest_account_id,kind,finance_id,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?)", facts.id, facts.id2, facts.id3, facts.kind, facts.financeId, facts.fact5, facts.now),
        readPmsImportBalances: (facts: {
            id: string;
            guestAccountId: string;
        }) => db.prepare(`SELECT b.finance_invoice_id id,b.amount_minor total FROM pms_import_balances b
    JOIN pms_records r ON r.id=b.entry_id AND r.property_id=b.property_id
    WHERE b.property_id=? AND r.data_json::jsonb->>'guestAccountId'=? AND b.pocket='due' AND b.finance_invoice_id IS NOT NULL ORDER BY b.source_key`).bind(facts.id, facts.guestAccountId).all<{
            id: string;
            total: number;
        }>(),
        readPmsDocuments2: (facts: {
            id: string;
            guestAccountId: string;
        }) => query<{
            id: string;
            snapshot_json: string;
        }>(db, "SELECT COALESCE(finance_id,id) id,snapshot_json FROM pms_documents WHERE property_id=? AND guest_account_id=? AND (kind='receipt' OR (kind='opening' AND finance_id IS NOT NULL)) ORDER BY created_at,id", facts.id, facts.guestAccountId),
        deleteFinanceAllocations: facts => finance.deleteFinanceAllocations(facts),
        updateFinanceInvoices: facts => finance.updateFinanceInvoices(facts),
        insertFinanceAllocations: facts => finance.insertFinanceAllocations(facts),
        readAssignments2: facts => identity_access.readAssignments2(facts),
        readFinanceParties2: facts => finance.readFinanceParties2(facts),
        readCommerceProducts2: facts => commerce.readCommerceProducts2(facts),
        readContext: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readOrganizationNodes2: facts => organization.readOrganizationNodes2(facts),
        insertPmsProperties: (facts: {
            target: string;
            companyId: string;
            branchId: string;
            name: string;
            businessDate: string;
            fact5: string;
            userId: string;
            fact7: number;
        }) => run(db, "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at) VALUES(?,?,?,?,?,?,?,?)", facts.target, facts.companyId, facts.branchId, facts.name, facts.businessDate, facts.fact5, facts.userId, facts.fact7),
        readPmsChargeCorrections2: (facts: {
            id: string;
        }) => db.prepare("SELECT 1 FROM pms_charge_corrections WHERE entry_id=? AND status='pending'").bind(facts.id).first(),
        insertPmsChargeCorrections: (facts: {
            id: string;
            companyId: string;
            id2: string;
            id3: string;
            guestAccountId: string;
            reason: string;
            userId: string;
            now: number;
            now2: number;
        }) => db.prepare(`INSERT INTO pms_charge_corrections(id,company_id,property_id,entry_id,guest_account_id,status,version,reason,requested_by,created_at,updated_at)
            VALUES(?,?,?,?,?,'pending',1,?,?,?,?)`).bind(facts.id, facts.companyId, facts.id2, facts.id3, facts.guestAccountId, facts.reason, facts.userId, facts.now, facts.now2),
        insertPmsChargeCorrectionEvents: (facts: {
            fact0: string;
            id: string;
            companyId: string;
            id2: string;
            id3: string;
            userId: string;
            reason: string;
            now: number;
        }) => db.prepare(`INSERT INTO pms_charge_correction_events(id,correction_id,company_id,property_id,entry_id,action,actor_id,correction_version,reason,created_at)
            VALUES(?,?,?,?,?,'requested',?,1,?,?)`).bind(facts.fact0, facts.id, facts.companyId, facts.id2, facts.id3, facts.userId, facts.reason, facts.now),
        updatePmsProperties: (facts: {
            nextVersion: number;
            now: number;
            id: string;
            companyId: string;
            version: number;
        }) => db.prepare("UPDATE pms_properties SET version=?,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(facts.nextVersion, facts.now, facts.id, facts.companyId, facts.version),
        readPmsChargeCorrections3: (facts: {
            id: string;
            companyId: string;
            id2: string;
        }) => db.prepare("SELECT * FROM pms_charge_corrections WHERE id=? AND company_id=? AND property_id=? FOR UPDATE").bind(facts.id, facts.companyId, facts.id2).first<ChargeCorrectionRow>(),
        updatePmsChargeCorrections: (facts: {
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
        }) => db.prepare(`UPDATE pms_charge_corrections SET status=?,version=?,reviewed_by=?,review_reason=?,reversal_entry_id=?,reviewed_at=?,updated_at=?
            WHERE id=? AND company_id=? AND property_id=? AND status='pending' AND version=?`).bind(facts.status, facts.correctionVersion, facts.userId, facts.reason, facts.reversalEntryId, facts.now, facts.now2, facts.id, facts.companyId, facts.id2, facts.expectedVersion),
        insertPmsChargeCorrectionEvents2: (facts: {
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
        }) => db.prepare(`INSERT INTO pms_charge_correction_events(id,correction_id,company_id,property_id,entry_id,action,actor_id,correction_version,reason,reversal_entry_id,created_at)
            VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(facts.fact0, facts.id, facts.companyId, facts.id2, facts.entry_id, facts.status, facts.userId, facts.correctionVersion, facts.reason, facts.reversalEntryId, facts.now),
        updatePmsProperties2: (facts: {
            propertyVersionAfter: number;
            now: number;
            id: string;
            companyId: string;
            propertyVersion: number;
        }) => db.prepare("UPDATE pms_properties SET version=?,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(facts.propertyVersionAfter, facts.now, facts.id, facts.companyId, facts.propertyVersion),
        readAttachmentSets: facts => files.readAttachmentSets(facts),
        insertPmsTransferRequests: (facts: {
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
        }) => db.prepare(`INSERT INTO pms_transfer_requests(id,company_id,property_id,guest_account_id,shift_id,kind,business_date,currency,amount_minor,reference,state,recorded_by,created_at,updated_at)
            VALUES(?,?,?,?,?,?,?,?,?,?,'pending',?,?,?)`).bind(facts.id, facts.companyId, facts.id2, facts.guestAccountId, facts.shiftId, facts.kind, facts.businessDate, facts.currency, facts.amount, facts.reference, facts.userId, facts.now, facts.now2),
        readPmsTransferRequests2: (facts: {
            id: string;
            companyId: string;
            id2: string;
        }) => db.prepare('SELECT * FROM pms_transfer_requests WHERE id=? AND company_id=? AND property_id=? FOR UPDATE').bind(facts.id, facts.companyId, facts.id2).first<TransferRequestRow>(),
        updatePmsTransferRequests: (facts: {
            userId: string;
            fact1: number;
            id: string;
            version: number;
        }) => db.prepare("UPDATE pms_transfer_requests SET state='cancelled',cancelled_by=?,version=version+1,updated_at=? WHERE id=? AND state='pending' AND version=?").bind(facts.userId, facts.fact1, facts.id, facts.version),
        readCompanyFxRates: (facts: {
            companyId: string;
            requestedCurrency: string;
        }) => db.prepare(`SELECT id FROM (${financeLastAuditProjectionSources.company_fx_rates}) company_fx_rates WHERE company_id=? AND currency=? ORDER BY effective_date DESC,confirmed_at DESC LIMIT 1`).bind(facts.companyId, facts.requestedCurrency).first(),
        readMarketingCampaigns: (facts: {
            companyId: string;
            fact1: string;
        }) => db.prepare(`SELECT 1 FROM (${marketingProjectionSources.marketing_campaigns}) marketing_campaigns WHERE company_id=? AND code=?`).bind(facts.companyId, facts.fact1).first(),
        readPmsTransferRequests3: (facts: {
            id: string;
            companyId: string;
            id2: string;
        }) => db.prepare('SELECT * FROM pms_transfer_requests WHERE id=? AND company_id=? AND property_id=? FOR UPDATE').bind(facts.id, facts.companyId, facts.id2).first<TransferRequestRow>(),
        updateFinanceBankTransactions: facts => finance.updateFinanceBankTransactions(facts),
        readPmsBookingPayments: (facts: {
            id: string;
        }) => db.prepare("SELECT id FROM pms_booking_payments WHERE property_id=? AND status='payment_pending' LIMIT 1").bind(facts.id).first(),
        readPmsBookingAllocations: (facts: {
            id: string;
            toGuestAccountId: string;
        }) => db.prepare('SELECT id FROM pms_booking_allocations WHERE guest_account_id IN (?,?) LIMIT 1').bind(facts.id, facts.toGuestAccountId).first(),
        readPmsBookingPayments2: (facts: {
            reservationId: string;
        }) => db.prepare("SELECT id FROM pms_booking_payments WHERE reservation_id=? AND status='payment_pending' LIMIT 1").bind(facts.reservationId).first(),
        readPmsBookingPayments3: (facts: {
            reservationId: string;
        }) => db.prepare("SELECT id FROM pms_booking_payments WHERE reservation_id=? AND status='confirmed' LIMIT 1").bind(facts.reservationId).first(),
        readAttachmentPublications: facts => files.readAttachmentPublications(facts),
        readPosConnections3: facts => pos.readPosConnections3(facts),
        updatePmsBookingEmails: (facts: {
            fact0: number;
            emailId: string;
            id: string;
        }) => db.prepare("UPDATE pms_booking_emails SET state='pending',attempts=0,next_attempt_at=?,lease_until=NULL,last_error=NULL WHERE id=? AND property_id=? AND state='failed' RETURNING id").bind(facts.fact0, facts.emailId, facts.id).first(),
        updatePmsDocuments: (facts: {
            toGuestAccountId: string;
            id: string;
            id2: string;
        }) => run(db, "UPDATE pms_documents SET guest_account_id=? WHERE property_id=? AND guest_account_id=?", facts.toGuestAccountId, facts.id, facts.id2),
        updatePmsTransferRequests2: (facts: {
            userId: string;
            matchedBankId: string;
            id: string;
            id2: string;
            fact4: number;
            id3: string;
        }) => run(db, "UPDATE pms_transfer_requests SET state='verified',verified_by=?,bank_transaction_id=?,entry_id=?,finance_payment_id=?,version=version+1,updated_at=? WHERE id=? AND state='pending'", facts.userId, facts.matchedBankId, facts.id, facts.id2, facts.fact4, facts.id3),
        updateFinanceBankTransactions2: facts => finance.updateFinanceBankTransactions2(facts),
        updatePmsProperties3: (facts: {
            name: string;
            businessDate: string;
            state: "setup" | "open" | "closing";
            fact3: string;
            version: number;
            fact5: number;
            id: string;
        }) => run(db, "UPDATE pms_properties SET name=?,business_date=?,state=?,config_json=?,version=?,updated_at=? WHERE id=?", facts.name, facts.businessDate, facts.state, facts.fact3, facts.version, facts.fact5, facts.id),
        insertPmsJobs: (facts: {
            nightJob: string;
            id: string;
            businessDate: string;
            fact3: string;
            fact4: number;
        }) => run(db, "INSERT INTO pms_jobs(id,property_id,business_date,actor_json,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(property_id,business_date) DO UPDATE SET state='pending',actor_json=excluded.actor_json,error_code=NULL,updated_at=excluded.updated_at", facts.nightJob, facts.id, facts.businessDate, facts.fact3, facts.fact4),
        updatePmsJobs: (facts: {
            fact0: "pending" | "completed";
            fact1: number;
            id: string;
        }) => run(db, "UPDATE pms_jobs SET state=?,error_code=NULL,updated_at=? WHERE id=?", facts.fact0, facts.fact1, facts.id),
        readPmsJobs: (facts: {
            fact0: string | null;
            fact1: string | null;
            fact2: string | null;
            fact3: string | null;
        }) => query<{
            id: string;
            property_id: string;
            actor_json: string;
        }>(db, "SELECT id,property_id,actor_json FROM pms_jobs WHERE state IN ('pending','running') AND (?::text IS NULL OR property_id=?) AND (?::text IS NULL OR id=?) ORDER BY updated_at LIMIT 4", facts.fact0, facts.fact1, facts.fact2, facts.fact3),
        readPmsProperties3: (facts: {
            property_id: string;
        }) => db.prepare("SELECT id FROM pms_properties WHERE id=? FOR UPDATE").bind(facts.property_id).first(),
        updatePmsJobs2: (facts: {
            code: string;
            fact1: number;
            id: string;
        }) => run(db, "UPDATE pms_jobs SET state='blocked',error_code=?,updated_at=? WHERE id=? AND state!='completed'", facts.code, facts.fact1, facts.id),
        updatePmsRecords: (facts: {
            code: string;
            fact1: number;
            id: string;
        }) => run(db, "UPDATE pms_records SET data_json=jsonb_set(jsonb_set(data_json::jsonb,'{state}','\"blocked\"'),'{message}',to_jsonb(?::text))::text,version=version+1,updated_at=? WHERE id=? AND data_json::jsonb->>'state'!='completed'", facts.code, facts.fact1, facts.id),
        readContext2: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        updatePmsProperties4: (facts: {
            version: number;
            fact1: number;
            id: string;
        }) => run(db, "UPDATE pms_properties SET version=?,updated_at=? WHERE id=?", facts.version, facts.fact1, facts.id),
        readContext3: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        updatePmsProperties5: (facts: {
            version: number;
            fact1: number;
            id: string;
        }) => run(db, "UPDATE pms_properties SET version=?,updated_at=? WHERE id=?", facts.version, facts.fact1, facts.id),
        insertPmsRecordsBatch: (facts: Array<{
            id: string;
            id2: string;
            kind: Kind;
            fact3: string;
            version: number;
            updatedAt: number;
        }>) => run(db, `INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at)
          SELECT row->>'id',row->>'id2',row->>'kind',row->>'fact3',
            (row->>'version')::bigint,(row->>'updatedAt')::bigint
          FROM jsonb_array_elements(?::jsonb) WITH ORDINALITY AS input(row,ordinal)
          ORDER BY ordinal
          ON CONFLICT(id) DO UPDATE SET data_json=excluded.data_json,version=excluded.version,updated_at=excluded.updated_at`, JSON.stringify(facts)),
        readMoneyTransactionSnapshots: (facts: {
            companyId: string;
            fact1: string;
            date: string;
            fact3: string;
        }) => db.prepare(`SELECT source_id FROM (${financeLastAuditProjectionSources.money_transaction_snapshots}) money_transaction_snapshots WHERE company_id=? AND source_type='pms_stay_price'
        AND source_id LIKE ? AND amount_kind=? AND amount_units=?`).bind(facts.companyId, facts.fact1, facts.date, facts.fact3).all<{
            source_id: string;
        }>(),
        readPmsDashboard: (arg0: string, arg1: string) => readPmsDashboard(db, arg0, arg1),
        readPmsOperationsList: (arg0: string, arg1: string, arg2: {
            view: "list";
            section: "inventory" | "reservations" | "front-desk" | "rooms" | "housekeeping" | "guest-accounts" | "cashier" | "guests" | "night-audit";
            page: number;
            q: string;
        building?: string;
        floor?: string;
        typeId?: string;
            status: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            from?: string | undefined;
            to?: string | undefined;
            selectedId?: string | undefined;
        }) => readPmsOperationsList(db, arg0, arg1, arg2),
        readPmsCalendar: (arg0: string, arg1: string, arg2: string) => readPmsCalendar(db, arg0, arg1, arg2),
        readPmsDetail: (arg0: string, arg1: string) => readPmsDetail(db, arg0, arg1),
        readPmsConfiguration: (arg0: string) => readPmsConfiguration(db, arg0),
        readAllPmsRecords: (arg0: string) => readAllPmsRecords(db, arg0),
        readPmsInventoryCommand: (propertyId: string) => readPmsInventoryCommand(db,propertyId),
        readPmsReservationCommand:(propertyId:string,guestId:string,source:string,externalRef:string)=>db.prepare(`
          SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind IN ('room_type','room','rate','block','item','credit')
          UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house')
          UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='guest' AND id=?
          UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='reservation' AND ?<>'' AND data_json::jsonb->>'source'=? AND data_json::jsonb->>'externalRef'=?
          ORDER BY id`).bind(propertyId,propertyId,propertyId,guestId,propertyId,externalRef,source,externalRef).all<PmsStoredRecord>().then(rows=>rows.results.map(decodePmsRecord)),
        readPmsHousekeepingCommand:(propertyId:string,targetId:string,targetKind:'task'|'room')=>db.prepare(`WITH target AS (
          SELECT CASE WHEN kind='task' THEN data_json::jsonb->>'roomId' ELSE id END room_id FROM pms_records WHERE property_id=? AND kind=? AND id=?
        ) SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r JOIN target t ON
          (r.kind='room' AND r.id=t.room_id) OR (r.kind='task' AND r.data_json::jsonb->>'roomId'=t.room_id)
          OR (r.kind='stay' AND r.data_json::jsonb->>'roomId'=t.room_id AND r.data_json::jsonb->>'state'='in_house')
          WHERE r.property_id=? ORDER BY r.id`).bind(propertyId,targetKind,targetId,propertyId).all<PmsStoredRecord>().then(rows=>rows.results.map(decodePmsRecord)),
        readPmsStayCommand: (propertyId, targetId, targetKind, guestId, guestAccountId, roomId) => db.prepare(`WITH scope AS (
          SELECT ?::text property_id, ?::text target_id, ?::text target_kind,
            ?::text guest_id, ?::text account_id, ?::text room_id
        ), target AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
          WHERE r.property_id=q.property_id AND r.kind=q.target_kind AND r.id=q.target_id
        ), booking_ids AS (
          SELECT CASE WHEN kind='reservation' THEN id ELSE data_json::jsonb->>'reservationId' END id FROM target
        ), siblings AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
          WHERE r.property_id=q.property_id AND r.kind='stay'
            AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)
        ), account_candidates AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
          WHERE r.property_id=q.property_id AND r.kind='guest_account'
            AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest_account'
              AND r.id IN (SELECT data_json::jsonb->>'guestAccountId' FROM siblings)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id=q.account_id
        ), accounts AS MATERIALIZED (
          SELECT * FROM account_candidates
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.data_json::jsonb->>'state'='credit'
              AND r.data_json::jsonb->>'partyId' IN (SELECT data_json::jsonb->>'partyId' FROM account_candidates WHERE data_json::jsonb->>'state'='credit')
        ), affected_rooms AS (
          SELECT data_json::jsonb->>'roomId' id FROM target WHERE kind='stay'
          UNION SELECT room_id FROM scope
        ), selected AS (
          SELECT * FROM target
          UNION SELECT * FROM siblings
          UNION SELECT * FROM accounts
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind IN ('room_type','room','rate','block','item','credit')
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('reserved','in_house')
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='reservation' AND r.id IN (SELECT id FROM booking_ids)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund') AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM accounts)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest' AND (r.id=q.guest_id
              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM siblings)
              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts)
              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM pms_records WHERE property_id=q.property_id AND kind='reservation' AND id IN (SELECT id FROM booking_ids)))
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='task' AND r.data_json::jsonb->>'roomId' IN (SELECT id FROM affected_rooms)
        ) SELECT * FROM selected ORDER BY updated_at,id`).bind(propertyId,targetId,targetKind,guestId,guestAccountId,roomId)
            .all<PmsStoredRecord>().then(rows=>rows.results.map(decodePmsRecord)),
        readPmsCashierCommand: (propertyId, targetId, targetKind, shiftId, userId) => db.prepare(`WITH scope AS (
          SELECT ?::text property_id, ?::text target_id, ?::text target_kind, ?::text shift_id, ?::text user_id
        ), target AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
          WHERE r.property_id=q.property_id AND r.kind=CASE WHEN q.target_kind='account_close' THEN 'guest_account' ELSE q.target_kind END AND r.id=q.target_id
        ), account_ids AS (
          SELECT CASE WHEN kind='refund' THEN data_json::jsonb->>'guestAccountId' ELSE id END id
          FROM target WHERE kind IN ('guest_account','refund')
        ), accounts AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
          WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)
        ), selected AS (
          SELECT * FROM target
          UNION SELECT * FROM accounts
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind IN ('rate','credit','item')
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')
              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest'
              AND r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND q.target_kind='account_close' AND r.kind='stay'
              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='shift' AND (r.id=q.shift_id OR
              (q.target_kind='new_shift' AND r.data_json::jsonb->>'userId'=q.user_id AND r.data_json::jsonb->>'state'='open'))
        ) SELECT * FROM selected ORDER BY updated_at,id`)
          .bind(propertyId,targetId,targetKind,shiftId,userId).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        readPmsPosCommand: (propertyId,stayId,sourceKey) => db.prepare(`WITH scope AS (
          SELECT ?::text property_id,?::text stay_id,?::text source_key
        ), source AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey'=q.source_key
        ), stays AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.id=q.stay_id
        ), account_ids AS (
          SELECT data_json::jsonb->>'guestAccountId' id FROM source
          UNION SELECT data_json::jsonb->>'guestAccountId' id FROM stays
        ), accounts AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)
        ), selected AS (
          SELECT * FROM source UNION SELECT * FROM stays UNION SELECT * FROM accounts
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')
              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='entry'
              AND r.data_json::jsonb->>'originalId' IN (SELECT id FROM source)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest'
              AND r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts UNION SELECT data_json::jsonb->>'guestId' FROM stays)
        ) SELECT * FROM selected ORDER BY updated_at,id`).bind(propertyId,stayId,sourceKey)
          .all<PmsStoredRecord>().then(rows=>rows.results.map(decodePmsRecord)),
        readPmsNightCommand: (propertyId, businessDate, targetId) => db.prepare(`WITH scope AS (
          SELECT ?::text property_id,?::text business_date,?::text target_id
        ), stays AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('reserved','in_house')
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='stay'
              AND r.data_json::jsonb->>'departure'>q.business_date AND r.data_json::jsonb->>'arrival'<=q.business_date
        ), accounts AS MATERIALIZED (
          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest_account'
              AND r.id IN (SELECT data_json::jsonb->>'guestAccountId' FROM stays)
        ), selected AS (
          SELECT * FROM stays UNION SELECT * FROM accounts
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind IN ('room_type','room','rate','block','item','credit')
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')
              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM accounts)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='guest' AND r.id IN (
              SELECT data_json::jsonb->>'guestId' FROM stays UNION SELECT data_json::jsonb->>'guestId' FROM accounts)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='night' AND (r.id=q.target_id OR r.data_json::jsonb->>'date'=q.business_date)
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='shift' AND (
              r.data_json::jsonb->>'state'='open' OR (r.data_json::jsonb->>'state'='closed' AND r.data_json::jsonb->'variance' IS DISTINCT FROM '0'::jsonb))
          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q
            WHERE r.property_id=q.property_id AND r.kind='task' AND r.data_json::jsonb->>'type'='stayover'
              AND r.data_json::jsonb->>'date'=(q.business_date::date+1)::text
              AND r.data_json::jsonb->>'roomId' IN (SELECT data_json::jsonb->>'roomId' FROM stays)
        ) SELECT * FROM selected ORDER BY updated_at,id`).bind(propertyId,businessDate,targetId)
          .all<PmsStoredRecord>().then(rows=>rows.results.map(decodePmsRecord)),
        readPmsGuestCommandRecord: (propertyId: string, guestId: string) => db.prepare("SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='guest' AND id=?").bind(propertyId,guestId).all<PmsStoredRecord>().then(rows=>rows.results.map(decodePmsRecord)),
        readPmsChoices: (id,selection) => readPmsChoices(db,id,selection),
        readPmsSetupFacts: (arg0: string) => readPmsSetupFacts(db, arg0),
        hospitalityDayControls: (arg0: string, arg1: string, arg2: string, arg3: string) => hospitalityDayControls(db, arg0, arg1, arg2, arg3),
        prepareJournal: (arg0: WorkContext, arg1: {
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
        }, arg2: `${string}-${string}-${string}-${string}-${string}`, arg3: "posted") => prepareJournal(db, arg0, arg1, arg2, arg3),
        assertProductionModuleEnabled: (arg0: string, arg1: "pms") => assertProductionModuleEnabled(db, arg0, arg1),
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
        preparePmsAccounting: (arg0: WorkContext, arg1: Snapshot, arg2: unknown) => preparePmsAccounting(db, arg0, arg1, arg2),
        captureMoneySnapshots: async (actor, inputs) => {
            const prepared = await snapshotMoneyBatch(db, actor, inputs);
            if (prepared.statements.length) await db.batch(prepared.statements);
        },
        snapshotMoney: (arg0: WorkContext | {
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
        }) => snapshotMoney(db, arg0, arg1),
        pmsOperationalWork: (arg0: WorkContext) => pmsOperationalWork(db, arg0),
        snapshotMoneyReversal: (arg0: {
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
        }) => snapshotMoneyReversal(db, arg0, arg1),
        snapshotMoneyCarry: (arg0: {
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
        }) => snapshotMoneyCarry(db, arg0, arg1), transactional: isTransactionalSqlDatabase(db), transaction: (work,propertyLockId) => { if (!isTransactionalSqlDatabase(db))
            throw new Error("POSTGRES_REQUIRED"); return db.transaction(tx => work(unit(tx)),propertyLockId?{sessionLock:{namespace:'erpx:pms:property',key:propertyLockId},preferIndexedReads:true,pipelineBatches:true}:undefined); }, batch: steps => db.batch(steps), replay: (...args) => replayCommand(db, ...args), execute: (...args) => executeCommand(db, ...args)
    };
}
const application = createPmsApplication<SqlStatement>({ forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write), clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), require: requirePermission, error: (status, code, message,details) => new ApiError(status, code, message,details), isBusinessError: (error): error is ApiError => error instanceof ApiError, environment: key => process.env[key] });
export const pmsPermissions = application.pmsPermissions;
export function loadPms(db: SqlDatabase, actor: WorkContext, id: string, lock = false, selection?: PmsReadSelection) { return application.loadPms(unit(db), actor, id, lock, selection); }
export function pmsWorkspace(actor: WorkContext, propertyId?: string, includeDayControls = false, frontMode = false, selection?: PmsReadSelection) { return application.pmsWorkspace(actor, propertyId, includeDayControls, frontMode, selection); }
export function effects(db: SqlDatabase, actor: WorkContext, s: Snapshot, e: Effects) { return application.effects(unit(db), actor, s, e); }
export function pmsCommand(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string, worker = false) { return application.pmsCommand(unit(db), actor, raw, key, worker); }
export function drainPms(db: TransactionalSqlDatabase, propertyId?: string, jobId?: string) { return application.drainPms(unit(db), propertyId, jobId); }
export async function pmsWork(actor: WorkContext) { return [...await application.pmsWork(actor),...await roomChargeReversalWork(actor,"pms")]; }
export function applyPosRoomCharge(tx: SqlDatabase, actor: WorkContext, raw: z.infer<typeof roomChargeSchema>, expectedPropertyVersion: number) { return application.applyPosRoomCharge(unit(tx), actor, raw, expectedPropertyVersion); }
export function reversePosRoomCharge(tx: SqlDatabase, actor: WorkContext, input: {
    propertyId: string;
    sourceKey: string;
    expectedPropertyVersion: number;
    reason: string;
    total: number;
}) { return application.reversePosRoomCharge(unit(tx), actor, input); }
export function persistPmsRecords(tx: SqlDatabase, s: Snapshot, original: Map<string, string>) { return application.persistPmsRecords(unit(tx), s, original); }
export type PmsWorkspace = Awaited<ReturnType<typeof pmsWorkspace>>;
