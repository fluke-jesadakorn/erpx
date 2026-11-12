import 'server-only';
import { createHash } from 'node:crypto';
import { z } from 'zod';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { one, run } from '@erpx/server-runtime/public/runtime/sql';
import { canonicalGuestAccount, legacyGuestAccount } from '@erpx/feature-identity_access/lib/guest-account-compat';
import { samePerson } from '@erpx/feature-identity_access/lib/iam';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import { iamMutationGuards } from '@erpx/server-identity_access/public/iam-store';
import { workContextGuards } from '@erpx/server-workflow/public/command-guards';
import { posCoreIdentityAccessFacts } from '@erpx/server-identity_access/public/pos-core-facts';
import { posCoreWorkflowFacts } from '@erpx/server-workflow/public/pos-core-facts';
import { posCoreFilesFacts } from '@erpx/server-files/public/pos-core-facts';
import { posCoreOrganizationFacts } from '@erpx/server-organization/public/pos-core-facts';
import { posCoreFinanceFacts } from '@erpx/server-finance/public/pos-core-facts';
import { posCoreCommerceFacts } from '@erpx/server-commerce/public/pos-core-facts';
import { prepareAttachmentBinding } from '@erpx/server-files/public/attachments/service';
import { assertProductionModuleEnabled } from '@erpx/server-pms/public/feature-availability';
import { posGuestAccountInterface } from '@erpx/server-pms/public/pos-guest-account-interface';
import { hospitalityOwnerProjectionSources as pmsProjectionSources } from '@erpx/server-pms/public/hospitality-owner-projection-sources';
import { snapshotMoney, snapshotMoneyReversal } from '@erpx/server-finance/public/company-money';
import { scaledAmount } from '@erpx/feature-commerce/lib/commerce-math';
import {
  businessDate as evaluateBusinessDate, money, portion, qty, saleAmounts, type PosAccounts,
} from '@erpx/server-pos/public/rules/pos';
import {
  audit, completeRefundPayment, config, enqueue, fail, id, num, openPeriod, pair, posting,
  receivePayment, releaseUnpaidReservations, reserve, saleRow, scopeRow, str, type PosRow,
} from '@erpx/server-pos/public/pos-core';
import { assertPosDayWritable } from '@erpx/server-pos/public/pos-daily';
import { posPackageEnforcementMode, withPosPurchasedCommand } from '@erpx/server-pos/adapters/purchase-boundary';

/**
 * Test/seed-only SQL oracle for the POS shift, sale, tender and refund commands.
 *
 * The production owner is the Rust crate (`erpx-server-pos`); the TypeScript
 * command branches for these operations were retired once no product route
 * called them. Retained fixtures, development seeds and integration/provider
 * tests still need real shift, sale, tender and refund rows, so this helper
 * reproduces exactly the durable owner effects the retired TS command wrote:
 * `pos_shifts`/`pos_cash_movements`, `pos_sales`/`pos_lines`,
 * `pos_fulfillments`/`pos_reservations`/`pos_stock_moves`, `pos_payments`,
 * `pos_refunds`/`pos_refund_payments`, `pos_postings`, `pos_receipts`,
 * `pos_audit`, `pos_jobs` plus the Finance journals/payments/invoices,
 * Commerce stock and money snapshots each command posted.
 *
 * It keeps the original command envelope (:permissions, department scope,
 * purchase gates, advisory lock, idempotent `pos_receipts` replay, attachment
 * binding, audit and receipt) and the original validation so fixtures fail
 * loudly with the same codes the retired command used. It is not production
 * code and must never run in a request path.
 */
export const retiredPosCommandNames = [
  'shift.open', 'cash.create', 'cash.approve', 'shift.close', 'shift.review',
  'sale.create', 'sale.add', 'sale.discount', 'sale.hold', 'sale.resume', 'sale.cancel',
  'payment.create', 'payment.verify_transfer', 'refund.request', 'refund.approve',
] as const;
export type RetiredPosCommand = (typeof retiredPosCommandNames)[number];
const retired = new Set<string>(retiredPosCommandNames);

export function isRetiredPosCommand(operation: string): operation is RetiredPosCommand { return retired.has(operation); }

/** A fixture binds its database once and keeps the retired `posCommand` call shape. */
export function posNativeSeed(database: PostgresDatabase) {
  return (actor: WorkContext, operation: RetiredPosCommand | string, input: unknown, key: string) =>
    seedPosCommand(database, actor, operation, input, key);
}

/** Routes to this oracle only for retired commands, leaving retained commands on `posCommand`. */
export function posSeedCommand(database: PostgresDatabase) {
  return async (actor: WorkContext, operation: string, input: unknown, key: string) => {
    if (isRetiredPosCommand(operation)) return seedPosCommand(database, actor, operation, input, key);
    const { posCommand } = await import('@erpx/server-pos/public/pos-core');
    return posCommand(database, actor, operation, input, key);
  };
}

const businessDate = (date = new Date(Date.now())) => evaluateBusinessDate('Asia/Bangkok', date);
const digest = (value: unknown) => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const keySchema = z.string().regex(/^[a-zA-Z0-9:_.-]{8,180}$/);
const base = z.object({ id: z.string().min(1), expectedVersion: z.number().int().positive() });

function permissionFor(op: string) {
  return op === 'day.confirm' || op === 'payment.verify_transfer' ? 'finance.approve'
    : op === 'refund.approve' || op === 'shift.review' || op === 'cash.approve' ? 'pos.approve'
      : 'pos.sell';
}

export async function seedPosCommand(
  database: PostgresDatabase,
  rawActor: WorkContext,
  operation: string,
  rawInput: unknown,
  key: string,
): Promise<Record<string, unknown>> {
  if (!isRetiredPosCommand(operation)) throw new Error(`POS_ORACLE_COMMAND_NOT_RETIRED:${operation}`);
  keySchema.parse(key);
  const input = canonicalGuestAccount(rawInput), actor = requirePermission(rawActor, permissionFor(operation));
  if (actor.departmentId) fail('POS_BRANCH_REQUIRED', 'Select a company or branch for POS', 403);
  const hash = digest({ user: actor.userId, assignment: actor.assignmentId, company: actor.companyId, branch: actor.branchId, operation, input });
  const mode = await posPackageEnforcementMode();
  const command = async (transaction: SqlDatabase) => {
    await assertProductionModuleEnabled(transaction, actor.companyId, 'pos');
    if (!await posCoreIdentityAccessFacts(transaction).readAssignments1({
      assignmentId: actor.assignmentId, userId: actor.userId, companyId: actor.companyId, role: actor.role,
      fact4: Date.now(), fact5: Date.now(), branchId: actor.branchId,
    })) fail('POS_ACCESS_CHANGED', 'Permissions changed. Select the role again', 403);
    await transaction.batch([...workContextGuards(transaction, actor, key), ...iamMutationGuards(transaction, actor, key)]);
    // All user commands in one Company serialize with preparation/confirmation.
    await run(transaction, "SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))", actor.companyId);
    const receipt = await one(transaction, 'SELECT * FROM pos_receipts WHERE id=?', key);
    if (receipt) {
      if (receipt.request_hash !== hash && receipt.request_hash !== digest({ user: actor.userId, assignment: actor.assignmentId, company: actor.companyId, branch: actor.branchId, operation, input: legacyGuestAccount(input) }))
        fail('IDEMPOTENCY_CONFLICT', 'Command number is already used for other data');
      await posCoreWorkflowFacts(transaction).writeCommandAssertions1({ key });
      return canonicalGuestAccount(JSON.parse(String(receipt.result_json)) as Record<string, unknown>);
    }
    const attachmentInput = z.object({
      id: z.string().min(1), expectedVersion: z.number().int().positive(), attachmentSetIds: z.array(z.string().min(1)).max(20).default([]),
    }).passthrough().safeParse(input);
    const attachmentDescriptor = operation === 'sale.cancel' ? { targetType: 'pos_sale', purposeKey: 'void_evidence', bindingType: 'pos_void', componentRef: 'void' }
      : operation === 'refund.approve' ? { targetType: 'pos_refund', purposeKey: 'refund_evidence', bindingType: 'pos_refund_decision', componentRef: 'refund-decision' }
        : operation === 'shift.review' ? { targetType: 'pos_shift', purposeKey: 'cash_variance', bindingType: 'pos_cash_variance_review', componentRef: 'variance-review' } : null;
    const attachmentData = attachmentInput.success ? attachmentInput.data : null, attachmentSetIds = attachmentData?.attachmentSetIds ?? [];
    if (attachmentSetIds.length && !attachmentDescriptor)
      fail('POS_ATTACHMENT_ACTION', 'Files can be bound only to a void, refund decision, or cash-variance review.', 422);
    if (attachmentSetIds.length && !attachmentData)
      fail('POS_ATTACHMENT_INPUT', 'Choose evidence for the current record revision.', 422);
    if (attachmentSetIds.length) {
      const placeholders = attachmentSetIds.map(() => '?').join(',');
      const sets = (await posCoreFilesFacts(transaction).readAttachmentSets1({ companyId: actor.companyId, fact1: attachmentSetIds, placeholders })).results;
      if (sets.length !== new Set(attachmentSetIds).size || sets.some(set => set.component_kind !== 'action' || set.purpose_key !== attachmentDescriptor!.purposeKey))
        fail('POS_ATTACHMENT_PURPOSE', 'Choose only the evidence purpose for this POS action.', 422);
    }
    const attachmentBinding = attachmentSetIds.length ? await prepareAttachmentBinding(transaction, actor, {
      attachmentSetIds,
      bindingContract: {
        targetType: attachmentDescriptor!.targetType, bindingType: attachmentDescriptor!.bindingType,
        contexts: [{ purposeKey: attachmentDescriptor!.purposeKey, componentKind: 'action', componentRef: attachmentDescriptor!.componentRef }],
      },
      targetId: attachmentData!.id, expectedTargetVersion: attachmentData!.expectedVersion, commandId: key,
      bindingId: `${attachmentData!.id}:${operation}:${attachmentData!.expectedVersion + 1}`,
    }) : null;
    if (attachmentBinding) await transaction.batch(attachmentBinding.guards);
    const result = await perform(transaction, actor, operation, input);
    if (attachmentBinding) await transaction.batch(attachmentBinding.steps);
    await run(transaction, 'INSERT INTO pos_receipts VALUES(?,?,?,?,?)', key, actor.companyId, hash, JSON.stringify(result), Date.now());
    await audit(transaction, actor.companyId, actor.branchId, actor.userId, operation, String(result.id ?? key), { input, result, assignmentId: actor.assignmentId });
    await posCoreWorkflowFacts(transaction).writeCommandAssertions1({ key });
    return result;
  };
  try {
    return mode === 'disabled' ? await database.transaction(command)
      : await withPosPurchasedCommand(database, actor, operation, input, key, mode, command);
  } catch (error) {
    if (error instanceof Error && error.message === 'QUALITY_HOLD')
      fail('QUALITY_HOLD', 'This product is quarantined. Quality inspection and independent release are required');
    if (['23505', '23514', '40001', '40P01'].includes((error as { code?: string }).code ?? ''))
      fail('POS_CONFLICT', 'Data or reserved amounts changed. Load the latest data and try again');
    throw error;
  }
}

function version(row: PosRow, input: { expectedVersion?: number }) {
  if (num(row, 'version') !== input.expectedVersion) fail('POS_VERSION_CONFLICT', 'Record changed. Load the latest data');
}

async function assertHotelDay(db: SqlDatabase, company: string, registerId: unknown, date: string) {
  const register = await one(db, 'SELECT property_id,branch_id FROM pos_registers WHERE company_id=? AND id=?', company, registerId);
  if (register) await posGuestAccountInterface(db).assertDay(company, register.property_id, register.branch_id, date);
}

async function captureMoney(db: SqlDatabase, sale: PosRow, sourceType: string, sourceId: string, amountKind: string, transactionDate: string, amountUnits: string) {
  const evidence = await snapshotMoney(db, { companyId: str(sale, 'company_id') }, {
    sourceType, sourceId, amountKind, transactionDate, currency: str(sale, 'currency'), currencyScale: 2, amountUnits,
  });
  await evidence.statement.run();
}

async function captureMoneyReversal(db: SqlDatabase, sale: PosRow, sourceType: string, sourceId: string, amountKind: string, transactionDate: string, amountUnits: string, original: { sourceType: string; sourceId: string; amountKind: string }) {
  const evidence = await snapshotMoneyReversal(db, { companyId: str(sale, 'company_id') }, {
    sourceType, sourceId, amountKind, transactionDate, amountUnits, original,
  });
  await evidence.statement.run();
}

async function editable(db: SqlDatabase, sale: PosRow) {
  if (!['open', 'held'].includes(str(sale, 'state'))) fail('POS_SALE_LOCKED', 'This bill cannot be edited');
  if (await one(db, "SELECT id FROM pos_payments WHERE sale_id=? AND (state IN ('requested','pending','exception') OR state='succeeded' AND amount_minor>refunded_minor) LIMIT 1", sale.id))
    fail('POS_PAYMENT_STARTED', 'This bill already has a payment');
}

async function refreshSale(db: SqlDatabase, saleId: string) {
  await run(db, `UPDATE pos_sales SET subtotal_minor=(SELECT COALESCE(SUM(subtotal_minor),0) FROM pos_lines WHERE sale_id=?),tax_minor=(SELECT COALESCE(SUM(tax_minor),0) FROM pos_lines WHERE sale_id=?),discount_minor=(SELECT COALESCE(SUM(discount_minor),0) FROM pos_lines WHERE sale_id=?),total_minor=(SELECT COALESCE(SUM(subtotal_minor+tax_minor),0) FROM pos_lines WHERE sale_id=?),version=version+1,updated_at=?,discount_approved_by=NULL,discount_reason=NULL WHERE id=?`,
    saleId, saleId, saleId, saleId, Date.now(), saleId);
}

async function perform(db: SqlDatabase, actor: WorkContext, op: string, input: unknown): Promise<Record<string, unknown>> {
  const company = actor.companyId, user = actor.userId;
  if (op === 'shift.open') {
    const p = z.object({ registerId: z.string(), openingMinor: money }).parse(input);
    const register = scopeRow(actor, await one(db, 'SELECT * FROM pos_registers WHERE id=? AND active=true FOR UPDATE', p.registerId));
    const settings = await config(db, company);
    if (!settings.accounting_approved_by) fail('POS_ACCOUNTING_REVIEW_REQUIRED', 'Finance must approve the posting accounts before selling.');
    if (!settings.enabled) fail('POS_DISABLED', 'POS transactions are not enabled');
    const hotel = register.property_id ? await one(db, `SELECT state,business_date FROM (${pmsProjectionSources.pms_properties}) pms_properties WHERE company_id=? AND branch_id=? FOR SHARE`, company, register.branch_id) : null;
    if (hotel && (hotel.state !== 'open' || hotel.business_date !== businessDate()))
      fail('POS_HOTEL_DAY_CLOSED', 'Finish or recover the hotel day close before opening a restaurant cash session');
    await assertPosDayWritable(db, company, str(register, 'branch_id'), businessDate());
    await openPeriod(db, company, businessDate());
    const sid = id();
    await run(db, 'INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,business_date) VALUES(?,?,?,?,?,?,?,?)',
      sid, register.id, company, register.branch_id, user, Date.now(), p.openingMinor, businessDate());
    return { id: sid };
  }
  if (op.startsWith('shift.') || op.startsWith('cash.')) {
    const p = base.extend({
      amountMinor: z.number().int().min(-99999999999).max(99999999999).optional(), countedMinor: money.optional(),
      denominations: z.record(z.string(), z.number().int().nonnegative().max(100000)).optional(),
      reason: z.string().trim().min(3).max(1000).optional(), movementId: z.string().optional(),
    }).parse(input);
    const shift = scopeRow(actor, await one(db, 'SELECT * FROM pos_shifts WHERE id=? FOR UPDATE', p.id));
    version(shift, p);
    await assertPosDayWritable(db, company, str(shift, 'branch_id'), str(shift, 'business_date'));
    const settings = await config(db, company), a = JSON.parse(String(settings.accounts_json)) as PosAccounts;
    if (op === 'cash.create') {
      if (shift.state !== 'open' || !p.amountMinor || !p.reason)
        fail('POS_CASH_INVALID', 'Enter the amount and reason in an open cash session', 422);
      const mid = id();
      await run(db, 'INSERT INTO pos_cash_movements VALUES(?,?,?,?,?,?,?)', mid, shift.id, p.amountMinor, p.reason, user, null, Date.now());
      await run(db, 'UPDATE pos_shifts SET version=version+1 WHERE id=?', shift.id);
      return { id: mid };
    }
    if (op === 'cash.approve') {
      if (shift.state !== 'open') fail('POS_SHIFT_CLOSED', 'Cash session is closed');
      const m = await one(db, 'SELECT * FROM pos_cash_movements WHERE id=? AND shift_id=? FOR UPDATE', p.movementId, shift.id);
      if (!m || m.approved_by || samePerson(actor, m.created_by)) fail('POS_INDEPENDENT_REVIEW', 'A different approver must review the record');
      await posting(db, company, str(shift, 'branch_id'), a, businessDate(), str(settings, 'currency'), `cash:${m.id}`, str(shift, 'id'), user,
        pair(num(m, 'amount_minor') > 0 ? a.cash : a.cashTransfer, num(m, 'amount_minor') > 0 ? a.cashTransfer : a.cash, Math.abs(num(m, 'amount_minor'))));
      await run(db, 'UPDATE pos_cash_movements SET approved_by=? WHERE id=?', user, m.id);
    }
    else if (op === 'shift.close') {
      if (shift.state !== 'open' || p.countedMinor === undefined) fail('POS_SHIFT_INVALID', 'Enter the actual counted amount', 422);
      if (await one(db, "SELECT id FROM pos_sales WHERE shift_id=? AND state IN ('open','held','payment_pending')", shift.id))
        fail('POS_OPEN_SALES', 'Close or cancel open bills before closing the cash session');
      if (await one(db, 'SELECT id FROM pos_cash_movements WHERE shift_id=? AND approved_by IS NULL', shift.id))
        fail('POS_CASH_REVIEW_PENDING', 'There are unapproved cash movements');
      const cash = await one(db, "SELECT COALESCE(SUM(p.amount_minor),0) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.shift_id=? AND p.method='cash' AND p.state='succeeded'", shift.id);
      const refunds = await one(db, "SELECT COALESCE(SUM(rp.amount_minor),0) n FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id JOIN pos_refunds r ON r.id=rp.refund_id WHERE r.payout_shift_id=? AND p.method='cash' AND rp.state='succeeded'", shift.id);
      const movements = await one(db, 'SELECT COALESCE(SUM(amount_minor),0) n FROM pos_cash_movements WHERE shift_id=? AND approved_by IS NOT NULL', shift.id);
      const expected = num(shift, 'opening_minor') + num(cash!, 'n') - num(refunds!, 'n') + num(movements!, 'n');
      if (p.denominations) {
        const denominations = [100000, 50000, 10000, 5000, 2000, 1000, 500, 200, 100, 50, 25];
        if (Object.keys(p.denominations).some(k => !denominations.includes(Number(k))) || Object.entries(p.denominations).reduce((n, [k, v]) => n + Number(k) * v, 0) !== p.countedMinor)
          fail('POS_CASH_COUNT', 'Denomination counts must match the counted amount', 422);
      }
      await run(db, "UPDATE pos_shifts SET state='closed',counted_minor=?,expected_minor=?,variance_minor=?,closed_at=?,closed_by=?,denominations_json=?::jsonb WHERE id=?",
        p.countedMinor, expected, p.countedMinor - expected, Date.now(), user, p.denominations ? JSON.stringify(p.denominations) : null, shift.id);
    }
    else if (op === 'shift.review') {
      if (shift.state !== 'closed' || samePerson(actor, shift.opened_by) || samePerson(actor, shift.closed_by) || !p.reason)
        fail('POS_INDEPENDENT_REVIEW', 'A different manager must review the total with a reason');
      const variance = num(shift, 'variance_minor');
      await posting(db, company, str(shift, 'branch_id'), a, businessDate(), str(settings, 'currency'), `shift:${shift.id}`, str(shift, 'id'), user,
        pair(variance > 0 ? a.cash : a.cashDifference, variance > 0 ? a.cashDifference : a.cash, Math.abs(variance)));
      await run(db, "UPDATE pos_shifts SET state='reviewed',reviewed_by=?,review_reason=? WHERE id=?", user, p.reason, shift.id);
    }
    else fail('POS_ACTION_UNKNOWN', 'Command not found', 404);
    await run(db, 'UPDATE pos_shifts SET version=version+1 WHERE id=?', shift.id);
    return { id: shift.id };
  }
  if (op === 'sale.create') {
    const p = z.object({
      shiftId: z.string(), tableId: z.string().nullable().optional(), guests: z.number().int().min(1).max(100).default(1),
      partyId: z.string().nullable().optional(), serviceMode: z.enum(['retail', 'dine_in', 'takeaway']),
      currency: z.string().regex(/^[A-Z]{3}$/).optional(),
    }).parse(input);
    const shift = scopeRow(actor, await one(db, "SELECT * FROM pos_shifts WHERE id=? AND state='open' FOR UPDATE", p.shiftId)), settings = await config(db, company);
    if (!settings.accounting_approved_by) fail('POS_ACCOUNTING_REVIEW_REQUIRED', 'Finance must approve the posting accounts before selling.');
    if (!settings.enabled) fail('POS_DISABLED', 'New transactions are temporarily paused');
    if (p.currency && p.currency !== settings.currency)
      fail('POS_CURRENCY_SETUP_REQUIRED', `The selected ${p.currency} currency needs a POS register, supported payment accounts and pricing setup before a new sale can start.`, 422);
    if (p.currency && p.currency !== 'USD' && !await posCoreFinanceFacts(db).readCompanyFxRates1({ company, currency: p.currency }))
      fail('MANUAL_RATE_REQUIRED', `Confirm a manual USD/${p.currency} rate before starting this sale.`, 422);
    const register = await one(db, 'SELECT * FROM pos_registers WHERE id=? AND active=true', shift.register_id);
    await assertHotelDay(db, company, shift.register_id, businessDate());
    await assertPosDayWritable(db, company, str(shift, 'branch_id'), businessDate());
    if (shift.business_date !== businessDate()) fail('POS_SHIFT_DATE', 'Close the previous business day cash session before selling');
    if (!register) fail('POS_REGISTER_DISABLED', 'Sales point is disabled');
    if (p.serviceMode === 'retail' && register.mode !== 'retail' || p.serviceMode !== 'retail' && register.mode !== 'restaurant')
      fail('POS_MODE_INVALID', 'Mode does not match the sales point', 422);
    if (p.tableId && !await one(db, 'SELECT id FROM pos_tables WHERE id=? AND register_id=?', p.tableId, register.id))
      fail('POS_TABLE_INVALID', 'Table is not at this sales point', 422);
    if (p.partyId && !await posCoreFinanceFacts(db).readFinanceParties1({ partyId: p.partyId, company }))
      fail('POS_CUSTOMER_INVALID', 'Customer is not available', 422);
    const sid = id();
    await run(db, 'INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,party_id,table_id,guests,service_mode,created_by,created_at,updated_at,accounts_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
      sid, company, shift.branch_id, register.id, shift.id, `POS-${businessDate().replaceAll('-', '')}-${sid.slice(0, 8).toUpperCase()}`, settings.currency, businessDate(),
      p.partyId ?? null, p.tableId ?? null, p.guests, p.serviceMode, user, Date.now(), Date.now(), settings.accounts_json);
    const issuer = await posCoreOrganizationFacts(db).readOrganizationNodes2({ company });
    await run(db, 'UPDATE pos_sales SET issuer_json=? WHERE id=?', JSON.stringify({ companyName: issuer?.display_name ?? company, registerName: register.name, branchId: shift.branch_id }), sid);
    return { id: sid };
  }
  if (op.startsWith('sale.')) {
    const p = base.passthrough().parse(input), sale = await saleRow(db, actor, p.id);
    version(sale, p);
    await assertHotelDay(db, company, sale.register_id, str(sale, 'business_date'));
    await assertPosDayWritable(db, company, str(sale, 'branch_id'), str(sale, 'business_date'));
    await editable(db, sale);
    if (op === 'sale.add') {
      await run(db, 'UPDATE pos_sales SET discount_requested_by=? WHERE id=?', user, sale.id);
      const q = z.object({
        productId: z.string(), quantityMilli: qty, modifierIds: z.array(z.string()).max(30).default([]),
        discountMinor: money.default(0), note: z.string().max(500).default(''),
      }).parse(p);
      const product = await posCoreCommerceFacts(db).readCommerceProducts3({ register_id: sale.register_id, productId: q.productId, company });
      if (!product) fail('POS_PRODUCT_INVALID', 'สินค้านี้ไม่ได้เปิดขายใน catalog ของจุดขาย กรุณาให้ผู้ดูแลเปิดขายก่อน', 422);
      const validation = z.object({ expectedCatalogVersion: z.number().int().positive().optional(), expectedPriceMinor: money.optional() }).parse(p);
      if (validation.expectedCatalogVersion !== undefined && validation.expectedCatalogVersion !== num(product, 'catalog_version') || validation.expectedPriceMinor !== undefined && validation.expectedPriceMinor !== num(product, 'price_minor'))
        fail('POS_CATALOG_CHANGED', 'ราคา/ตัวเลือกเปลี่ยน กรุณาตรวจตะกร้าก่อนส่งใหม่');
      const modifiers = (JSON.parse(String(product.modifiers_json ?? '[]')) as Array<{ id: string; name: string; priceMinor: number; ingredients: Array<{ productId: string; quantityMilli: number }> }>)
        .filter(m => q.modifierIds.includes(m.id));
      if (modifiers.length !== q.modifierIds.length) fail('POS_MODIFIER_INVALID', 'Invalid modifiers', 422);
      const kind = str(product, 'kind') || 'stock', price = num(product, 'price_minor') + modifiers.reduce((n, m) => n + m.priceMinor, 0),
        amounts = saleAmounts(q.quantityMilli, price, num(product, 'tax_bps'), Boolean(product.tax_inclusive), q.discountMinor);
      if (kind === 'recipe' && sale.service_mode === 'retail') fail('POS_RESTAURANT_REQUIRED', 'Prepared menus require a restaurant sales point', 422);
      const recipe = kind === 'stock' ? [{ productId: q.productId, quantityMilli: 1000 }]
        : kind === 'recipe' ? JSON.parse(String(product.recipe_json)) as Array<{ productId: string; quantityMilli: number }> : [];
      recipe.push(...modifiers.flatMap(m => m.ingredients));
      const register = (await one(db, 'SELECT * FROM pos_registers WHERE id=?', sale.register_id))!, fid = id(), lid = id();
      await run(db, 'INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,note,recipe_json,accounts_json,currency,business_date,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
        fid, company, sale.branch_id, register.warehouse_id, q.productId, q.quantityMilli, kind, str(product, 'station') || 'Kitchen', q.note,
        JSON.stringify(recipe), sale.accounts_json, sale.currency, sale.business_date, Date.now());
      await run(db, 'INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
        lid, sale.id, fid, q.productId, product.name, q.quantityMilli, price, product.tax_bps, Boolean(product.tax_inclusive),
        q.discountMinor, amounts.subtotal, amounts.tax, JSON.stringify(modifiers), q.note);
    }
    else if (op === 'sale.discount') {
      const q = z.object({ amountMinor: money }).parse(p);
      const lines = (await db.prepare('SELECT * FROM pos_lines WHERE sale_id=? ORDER BY id FOR UPDATE').bind(sale.id).all<PosRow>()).results;
      const weights = lines.map(l => scaledAmount(num(l, 'price_minor'), num(l, 'quantity_milli'), 1000) - num(l, 'discount_minor') + num(l, 'bill_discount_minor'));
      const total = weights.reduce((n, w) => n + w, 0);
      if (q.amountMinor > total) fail('POS_DISCOUNT_AMOUNT', 'Discount exceeds the bill amount', 422);
      let previous = 0;
      for (const [i, l] of lines.entries()) {
        const extra = weights[i] && q.amountMinor ? portion(q.amountMinor, previous, weights[i], total) : 0;
        previous += weights[i];
        const discount = num(l, 'discount_minor') - num(l, 'bill_discount_minor') + extra;
        const amounts = saleAmounts(num(l, 'quantity_milli'), num(l, 'price_minor'), num(l, 'tax_bps'), Boolean(l.tax_inclusive), discount);
        await run(db, 'UPDATE pos_lines SET discount_minor=?,bill_discount_minor=?,subtotal_minor=?,tax_minor=? WHERE id=?', discount, extra, amounts.subtotal, amounts.tax, l.id);
      }
      await run(db, 'UPDATE pos_sales SET discount_requested_by=? WHERE id=?', user, sale.id);
    }
    else if (op === 'sale.hold' || op === 'sale.resume')
      await run(db, 'UPDATE pos_sales SET state=? WHERE id=?', op === 'sale.hold' ? 'held' : 'open', sale.id);
    else if (op === 'sale.cancel') {
      for (const f of (await db.prepare('SELECT DISTINCT f.* FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?').bind(sale.id).all<PosRow>()).results) {
        if (!['draft', 'queued', 'cancelled'].includes(str(f, 'state')))
          fail('POS_KITCHEN_ACTIVE', 'Cancel the kitchen task and record waste before cancelling the bill');
        if (await one(db, 'SELECT id FROM pos_lines WHERE fulfillment_id=? AND sale_id<>?', f.id, sale.id))
          fail('POS_SHARED_KITCHEN', 'This kitchen item is on a separate bill. Handle the item first');
        await run(db, 'DELETE FROM pos_reservations WHERE fulfillment_id=?', f.id);
        await run(db, "UPDATE pos_fulfillments SET state='cancelled',reserved=false,version=version+1 WHERE id=?", f.id);
      }
      await run(db, "UPDATE pos_sales SET state='cancelled',table_id=NULL WHERE id=?", sale.id);
    }
    else fail('POS_ACTION_UNKNOWN', 'Command not found', 404);
    await refreshSale(db, str(sale, 'id'));
    return { id: sale.id, version: (await one(db, 'SELECT version FROM pos_sales WHERE id=?', sale.id))!.version };
  }
  if (op === 'payment.create') {
    const p = base.extend({
      method: z.enum(['cash', 'transfer', 'edc', 'stripe_card', 'stripe_promptpay']), amountMinor: money.refine(n => n > 0),
      tenderedMinor: money, reference: z.string().max(150).default(''), payerEmail: z.union([z.email(), z.literal('')]).default(''),
    }).parse(input), sale = await saleRow(db, actor, p.id);
    version(sale, p);
    await assertHotelDay(db, company, sale.register_id, str(sale, 'business_date'));
    await assertPosDayWritable(db, company, str(sale, 'branch_id'), str(sale, 'business_date'));
    if (process.env.NODE_ENV === 'production' && !['cash', 'transfer'].includes(p.method) && process.env.POS_STRIPE_LIVE_ENABLED !== 'true')
      fail('POS_TENDER_COMING_SOON', 'The pilot accepts cash and verified bank transfers only', 403);
    if (!['open', 'held', 'payment_pending'].includes(str(sale, 'state'))) fail('POS_SALE_LOCKED', 'Bill is not ready for payment');
    const settings = await config(db, company);
    if (!settings.accounting_approved_by) fail('POS_ACCOUNTING_REVIEW_REQUIRED', 'Finance must approve the posting accounts before selling.');
    if (!settings.enabled) fail('POS_DISABLED', 'New transactions are temporarily paused');
    await openPeriod(db, company, str(sale, 'business_date'));
    if (!await one(db, "SELECT id FROM pos_shifts WHERE id=? AND state='open'", sale.shift_id)) fail('POS_SHIFT_CLOSED', 'Cash session is closed');
    const gross = await one(db, 'SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0) n FROM pos_lines WHERE sale_id=?', sale.id);
    if (num(sale, 'discount_minor') > scaledAmount(num(gross!, 'n'), num(settings, 'cashier_discount_bps'), 10000) && !sale.discount_approved_by)
      fail('POS_DISCOUNT_APPROVAL', 'This discount must be approved before payment');
    const reserved = await one(db, "SELECT COALESCE(SUM(amount_minor-refunded_minor),0) n FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','succeeded','exception')", sale.id);
    if (p.amountMinor > num(sale, 'total_minor') - num(reserved!, 'n') || p.tenderedMinor < p.amountMinor || p.method !== 'cash' && p.tenderedMinor !== p.amountMinor)
      fail('POS_PAYMENT_AMOUNT', 'Invalid payment or tendered amount', 422);
    if (['transfer', 'edc'].includes(p.method) && p.reference.trim().length < 2) fail('POS_REFERENCE_REQUIRED', 'Enter the payment reference', 422);
    if (p.method === 'stripe_promptpay' && !p.payerEmail)
      fail('POS_PROMPTPAY_EMAIL_REQUIRED', 'กรอกอีเมลผู้จ่ายสำหรับ PromptPay และการคืนเงินก่อนสร้าง QR', 422);
    for (const f of (await db.prepare('SELECT DISTINCT f.* FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?').bind(sale.id).all<PosRow>()).results) {
      if (f.state === 'cancelled') fail('POS_ITEM_CANCELLED', 'Remove cancelled items before taking payment');
      await reserve(db, f);
    }
    let connection: PosRow | null = null;
    if (p.method.startsWith('stripe_')) {
      if (sale.currency !== 'THB') fail('POS_STRIPE_CURRENCY', 'This Stripe POS version accepts THB only');
      connection = await one(db, "SELECT * FROM pos_connections WHERE company_id=? AND state='ready'", company);
      if (connection?.livemode && process.env.POS_STRIPE_LIVE_ENABLED !== 'true')
        fail('POS_LIVE_NOT_ENABLED', 'Complete sandbox testing before accepting live payments');
      if (!connection?.charges_enabled || !connection.payouts_enabled || !(p.method === 'stripe_card' ? connection.card_enabled : connection.promptpay_enabled))
        fail('POS_STRIPE_NOT_READY', 'The Stripe account is not ready for this payment method');
    }
    const pid = id();
    await run(db, "INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,stripe_account,livemode,created_by,created_at,updated_at,payer_email) VALUES(?,?,?,?,?,?,?,'requested',?,?,?,?,?,?)",
      pid, sale.id, company, p.method, p.amountMinor, p.tenderedMinor, p.reference, connection?.account_id ?? null, connection?.livemode ?? false, user, Date.now(), Date.now(), p.payerEmail || null);
    await captureMoney(db, sale, 'pos_payment', pid, 'requested', str(sale, 'business_date'), String(p.amountMinor));
    if (connection) await enqueue(db, company, 'checkout', pid);
    else if (p.method === 'transfer') await run(db, "UPDATE pos_payments SET state='pending',version=version+1,updated_at=? WHERE id=?", Date.now(), pid);
    else await receivePayment(db, (await one(db, 'SELECT * FROM pos_payments WHERE id=?', pid))!, user);
    await run(db, "UPDATE pos_sales SET state='payment_pending',version=version+1,updated_at=? WHERE id=?", Date.now(), sale.id);
    return { id: pid, saleId: sale.id };
  }
  if (op === 'payment.verify_transfer') {
    const p = base.extend({ bankTransactionId: z.string().min(1), bankVersion: z.number().int().positive() }).parse(input);
    const payment = await one(db, 'SELECT * FROM pos_payments WHERE id=? AND company_id=? FOR UPDATE', p.id, company);
    if (!payment) fail('POS_NOT_FOUND', 'Payment not found', 404);
    version(payment, p);
    if (payment.method !== 'transfer' || payment.state !== 'pending') fail('POS_TRANSFER_STATE', 'Only a pending bank transfer can be verified');
    if (samePerson(actor, str(payment, 'created_by'))) fail('POS_INDEPENDENT_VERIFICATION', 'A different Finance user must verify the transfer', 403);
    const sale = await saleRow(db, actor, str(payment, 'sale_id'));
    const accounts = JSON.parse(String(sale.accounts_json)) as PosAccounts;
    const bank = await posCoreFinanceFacts(db).readFinanceBankTransactions1({ bankTransactionId: p.bankTransactionId, company });
    if (!bank || num(bank, 'version') !== p.bankVersion || !['unmatched', 'proposed'].includes(str(bank, 'match_status')) || bank.bank_account !== accounts.bank || bank.currency !== sale.currency || num(bank, 'amount_minor') !== num(payment, 'amount_minor') || str(bank, 'external_id').trim() !== str(payment, 'reference').trim() || str(bank, 'transaction_date') < str(sale, 'business_date'))
      fail('POS_BANK_EVIDENCE_MISMATCH', 'Match the exact imported bank account, reference, date, currency, and amount');
    await receivePayment(db, payment, user, str(bank, 'transaction_date'));
    const settled = await one(db, 'SELECT finance_payment_id FROM pos_payments WHERE id=?', payment.id);
    await posCoreFinanceFacts(db).writeFinanceBankTransactions1({ fact0: settled!.finance_payment_id, id: bank.id });
    return { id: payment.id, saleId: sale.id, state: 'succeeded', bankTransactionId: bank.id };
  }
  if (op === 'refund.request') {
    const p = base.extend({
      reason: z.string().trim().min(3).max(1000),
      lines: z.array(z.object({ lineId: z.string(), quantityMilli: qty, restock: z.boolean() })).max(100).default([]),
      amountMinor: money.optional(), payoutShiftId: z.string().optional(),
    }).parse(input), sale = await saleRow(db, actor, p.id);
    version(sale, p);
    await assertPosDayWritable(db, str(sale, 'company_id'), str(sale, 'branch_id'), businessDate());
    if (await one(db, "SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=? AND state='prepared'", sale.company_id, sale.branch_id, sale.business_date))
      fail('POS_DAY_REVIEW_PENDING', 'Finance must confirm the original day close before a refund can change its source records');
    if (await one(db, `SELECT source_key FROM (${pmsProjectionSources.pms_sources}) pms_sources WHERE source_key=?`, `pos:${sale.id}:${sale.id}`))
      fail('POS_ROOM_CHARGE_REVERSAL', 'Reverse this charge on the hotel guest account; it has no POS payment to refund');
    if (!['completed', 'payment_pending'].includes(str(sale, 'state'))) fail('POS_REFUND_SALE', 'Bill has no payment');
    if (new Set(p.lines.map(l => l.lineId)).size !== p.lines.length) fail('POS_DUPLICATE_LINE', 'Duplicate refund item', 422);
    const advance = sale.state !== 'completed', snapshots: Array<Record<string, unknown>> = [];
    let amount = 0, tax = 0;
    if (advance) {
      if (!p.amountMinor || p.lines.length) fail('POS_ADVANCE_REFUND', 'Enter an advance payment refund amount without selecting items', 422);
      amount = p.amountMinor;
    }
    else for (const part of p.lines) {
      const l = await one(db, 'SELECT * FROM pos_lines WHERE id=? AND sale_id=?', part.lineId, sale.id);
      if (!l || num(l, 'returned_milli') + part.quantityMilli > num(l, 'quantity_milli'))
        fail('POS_REFUND_QUANTITY', 'Refund quantity exceeds the original item', 422);
      const subtotal = portion(num(l, 'subtotal_minor'), num(l, 'returned_milli'), part.quantityMilli, num(l, 'quantity_milli')),
        vat = portion(num(l, 'tax_minor'), num(l, 'returned_milli'), part.quantityMilli, num(l, 'quantity_milli'));
      amount += subtotal + vat;
      tax += vat;
      snapshots.push({ ...part, subtotalMinor: subtotal, taxMinor: vat, previousReturned: num(l, 'returned_milli') });
    }
    if (amount <= 0) fail('POS_REFUND_EMPTY', 'Select an item with a refund amount', 422);
    const rid = id();
    await run(db, 'INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,payout_shift_id,kind,reason,lines_json,amount_minor,requested_by,created_at,accounting_date,tax_minor) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)',
      rid, sale.id, sale.company_id, sale.branch_id, p.payoutShiftId ?? null, advance ? 'advance' : 'sale', p.reason, JSON.stringify(snapshots), amount, user, Date.now(), businessDate(), tax);
    return { id: rid };
  }
  if (op === 'refund.approve') {
    const p = base.extend({ reason: z.string().trim().min(3).max(1000).optional() }).parse(input), refund = scopeRow(actor, await one(db, 'SELECT * FROM pos_refunds WHERE id=? FOR UPDATE', p.id));
    version(refund, p);
    if (refund.state !== 'requested' || samePerson(actor, refund.requested_by)) fail('POS_INDEPENDENT_REVIEW', 'A different manager must approve the pending request');
    const sale = await saleRow(db, actor, str(refund, 'sale_id'));
    await assertHotelDay(db, actor.companyId, sale.register_id, str(refund, 'accounting_date'));
    const a = JSON.parse(String(sale.accounts_json)) as PosAccounts, date = str(refund, 'accounting_date');
    const payments = (await db.prepare("SELECT * FROM pos_payments WHERE sale_id=? AND state='succeeded' ORDER BY created_at DESC,id FOR UPDATE").bind(sale.id).all<PosRow>()).results;
    if (payments.reduce((n, r) => n + num(r, 'amount_minor') - num(r, 'refunded_minor'), 0) < num(refund, 'amount_minor'))
      fail('POS_REFUND_EXCEEDED', 'Refund exceeds the remaining payment amount');
    if (refund.kind === 'advance' && sale.state === 'completed') fail('POS_REFUND_CHANGED', 'Bill is closed. Request a refund from the item');
    if (refund.kind === 'sale' && sale.state !== 'completed') fail('POS_REFUND_CHANGED', 'Bill status changed');
    for (const part of JSON.parse(String(refund.lines_json)) as Array<{ lineId: string; quantityMilli: number; restock: boolean; subtotalMinor: number; taxMinor: number; previousReturned: number }>) {
      const line = (await one(db, 'SELECT * FROM pos_lines WHERE id=? AND sale_id=? FOR UPDATE', part.lineId, sale.id))!;
      if (!line || num(line, 'returned_milli') !== part.previousReturned || part.quantityMilli + num(line, 'returned_milli') > num(line, 'quantity_milli'))
        fail('POS_REFUND_CHANGED', 'This item has already been refunded. Create a new request');
      const f = (await one(db, 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE', line.fulfillment_id))!;
      let restoredCost = 0;
      if (part.restock) {
        if (f.kind !== 'stock' || f.state !== 'delivered') fail('POS_RETURN_NOT_STOCK', 'Only delivered items can be returned to inventory', 422);
        const returned = await one(db, "SELECT COALESCE(SUM(quantity_milli),0) qty,COALESCE(SUM(value_minor),0) cost FROM pos_stock_moves WHERE fulfillment_id=? AND kind='return'", f.id);
        restoredCost = portion(num(f, 'cost_minor'), num(returned!, 'qty'), part.quantityMilli, num(f, 'quantity_milli'));
        await posCoreCommerceFacts(db).writeCommerceStock3({ warehouse_id: f.warehouse_id, product_id: f.product_id });
        await posCoreCommerceFacts(db).writeCommerceStock4({ quantityMilli: part.quantityMilli, restoredCost, warehouse_id: f.warehouse_id, product_id: f.product_id });
        await run(db, 'INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)', id(), sale.company_id, f.warehouse_id, f.product_id, f.id, refund.id, sale.number, 'return', part.quantityMilli, restoredCost, date, Date.now());
        await posting(db, str(sale, 'company_id'), str(sale, 'branch_id'), a, date, str(sale, 'currency'), `return:${refund.id}:${line.id}`, str(refund, 'id'), user, pair(a.inventory, a.cogs, restoredCost));
      }
      await run(db, 'UPDATE pos_lines SET returned_milli=returned_milli+?,refunded_minor=refunded_minor+?,refunded_tax_minor=refunded_tax_minor+?,returned_cost_minor=returned_cost_minor+? WHERE id=?',
        part.quantityMilli, part.subtotalMinor + part.taxMinor, part.taxMinor, restoredCost, line.id);
    }
    await posting(db, str(sale, 'company_id'), str(sale, 'branch_id'), a, date, str(sale, 'currency'), `refund:${refund.id}`, str(refund, 'id'), user, [
      { account: refund.kind === 'advance' ? a.advance : a.revenue, debit: num(refund, 'amount_minor') - num(refund, 'tax_minor'), credit: 0 },
      { account: a.outputTax, debit: num(refund, 'tax_minor'), credit: 0 },
      { account: a.refund, debit: 0, credit: num(refund, 'amount_minor') },
    ]);
    if (refund.kind === 'sale')
      await captureMoneyReversal(db, sale, 'pos_refund', str(refund, 'id'), 'total', date, String(refund.amount_minor), { sourceType: 'pos_sale', sourceId: str(sale, 'id'), amountKind: 'total' });
    let remaining = num(refund, 'amount_minor');
    for (const payment of payments) {
      const amount = Math.min(remaining, num(payment, 'amount_minor') - num(payment, 'refunded_minor'));
      if (!amount) continue;
      const manualPromptPay = payment.method === 'stripe_promptpay' && !payment.payer_email;
      const rpid = id();
      await run(db, 'INSERT INTO pos_refund_payments(id,refund_id,payment_id,amount_minor,manual_method) VALUES(?,?,?,?,?)',
        rpid, refund.id, payment.id, amount, manualPromptPay ? (refund.payout_shift_id ? 'cash' : 'transfer') : null);
      await run(db, 'UPDATE pos_payments SET refunded_minor=refunded_minor+?,version=version+1 WHERE id=?', amount, payment.id);
      if (str(payment, 'method').startsWith('stripe_') && !manualPromptPay) await enqueue(db, str(sale, 'company_id'), 'refund', rpid);
      else {
        if (payment.method === 'cash' || (manualPromptPay && refund.payout_shift_id)) {
          const shift = await one(db, "SELECT * FROM pos_shifts WHERE id=? AND company_id=? AND branch_id=? AND state='open'", refund.payout_shift_id, actor.companyId, sale.branch_id);
          if (!shift) fail('POS_REFUND_SHIFT_REQUIRED', 'Specify an open cash session for cash refund payout');
        }
        // Manual methods require the approver to confirm money was actually returned.
        if (!p.reason) fail('POS_REFUND_PROOF_REQUIRED', 'Enter proof/reference for the cash, transfer, or EDC refund', 422);
        await completeRefundPayment(db, rpid, user);
      }
      remaining -= amount;
    }
    await run(db, "UPDATE pos_refunds SET approved_by=?,state=CASE WHEN EXISTS(SELECT 1 FROM pos_refund_payments WHERE refund_id=? AND state<>'succeeded') THEN 'processing' ELSE 'completed' END,version=version+1 WHERE id=?",
      user, refund.id, refund.id);
    await releaseUnpaidReservations(db, str(sale, 'id'));
    return { id: refund.id };
  }
  fail('POS_ACTION_UNKNOWN', 'POS command not found', 404);
}
