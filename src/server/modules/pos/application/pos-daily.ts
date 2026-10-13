import { z } from "zod";
import type { WorkContext } from "../../identity_access/public/authorization";
import { num, str, scopeRow as domainScopeRow, type PosRow } from "../domain/pos-model";
import { samePerson } from "@/lib/identity_access/iam";
import { businessDate as evaluateBusinessDate } from "../domain/pos";
import type { Clock } from "../../shared/application/ports";
export type PosReceiptSnapshot = {
    schema: 1;
    saleId: string;
    number: string;
    currency: string;
    businessDate: string;
    issuer: Record<string, unknown>;
    customer: PosRow | null;
    lines: PosRow[];
    payments: PosRow[];
    subtotalMinor: number;
    discountMinor: number;
    taxMinor: number;
    totalMinor: number;
    roomCharge: PosRow | null;
};
/** Called in the completion transaction. Reprints never consult mutable masters. */
export type PosRows<Row> = {
    results: Row[];
};
export type PosReadEffect = {
    run(): Promise<PosRows<Record<string, unknown>>>;
};
export type PosReadAttachmentBinding<Effect> = {
    guards: Effect[];
    steps: Effect[];
};
export type PosDayUnit<Effect extends PosReadEffect> = {
    readPosSales1(facts: {
        saleId: string;
    }): Promise<PosRow | null>;
    readFinanceParties1(facts: {
        party_id: string | number | true;
        company_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosLines1(facts: {
        saleId: string;
    }): Promise<PosRow[]>;
    readPosPayments1(facts: {
        saleId: string;
    }): Promise<PosRow[]>;
    readPmsSources1(facts: {
        fact0: string;
    }): Promise<PosRow | null>;
    writePosSaleDocuments1(facts: {
        saleId: string;
        company_id: string | number | boolean | null;
        branch_id: string | number | boolean | null;
        fact3: string;
        fact4: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readFinanceParties2(facts: {
        companyId: string;
        q: string;
        pattern: string;
        pattern2: string;
        pattern3: string;
    }): Promise<PosRow[]>;
    readFinanceParties3(facts: {
        companyId: string;
        name: string;
        fact2: string;
        fact3: string;
        email: string;
        email2: string;
    }): Promise<PosRow | null>;
    readPosSettings1(facts: {
        companyId: string;
    }): Promise<PosRow | null>;
    writeFinanceParties1(facts: {
        customer: `${string}-${string}-${string}-${string}-${string}`;
        companyId: string;
        fact2: string;
        name: string;
        currency: string | number | boolean | null;
        fact5: string | null;
        fact6: string | null;
        fact7: string;
        fact8: number;
        fact9: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosDayCloses1(facts: {
        company: string;
        branch: string;
        date: string;
    }): Promise<PosRow | null>;
    readOrganizationNodes1(facts: {
        branch: string;
        companyId: string;
    }): Promise<PosRow | null>;
    readPosSales2(facts: {
        company: string;
        branch: string;
        date: string;
    }): Promise<PosRow[]>;
    readPosShifts1(facts: {
        company: string;
        branch: string;
        date: string;
    }): Promise<PosRow[]>;
    readPosPayments2(facts: {
        company: string;
        branch: string;
        date: string;
    }): Promise<PosRow[]>;
    readPosRefunds1(facts: {
        company: string;
        branch: string;
        date: string;
    }): Promise<PosRow[]>;
    readPosRefundPayments1(facts: {
        company: string;
        branch: string;
        date: string;
    }): Promise<PosRow[]>;
    readPosStockMoves1(facts: {
        company: string;
        date: string;
        company2: string;
        branch: string;
    }): Promise<PosRow[]>;
    readFinanceJournals1(facts: {
        company: string;
        date: string;
        branch: string;
    }): Promise<PosRow[]>;
    readFinanceJournalLines1(facts: {
        company: string;
        date: string;
        branch: string;
    }): Promise<PosRow | null>;
    readFinanceJournalLines2(facts: {
        company: string;
        date: string;
        branch: string;
    }): Promise<PosRow | null>;
    readPosSettings2(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readPosFulfillments1(facts: {
        company: string;
        branch: string;
        date: string;
    }): Promise<PosRow[]>;
    readPosCashMovements1(facts: {
        company: string;
        branch: string;
        date: string;
    }): Promise<PosRow[]>;
    readPosSettlements1(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readFinanceJournalLines3(facts: {
        company: string;
    }): Promise<PosRow | null>;
    writePosDayCloses1(facts: {
        close: `${string}-${string}-${string}-${string}-${string}`;
        companyId: string;
        branchId: string;
        businessDate: string;
        currency: string;
        fact5: string;
        digest: string;
        userId: string;
        fact8: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosDayCloses2(facts: {
        id: string;
    }): Promise<PosRow | null>;
    writePosDayCloses2(facts: {
        userId: string;
        fact1: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    batch(effects: Effect[]): Promise<unknown>;
};
export function createPosDay<Effect extends PosReadEffect>(ports: {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    environment(key: "NODE_ENV" | "ERPX_POS_ROOM_CHARGE_PILOT_ENABLED" | "POS_STRIPE_LIVE_ENABLED"): string | undefined;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    forActor(actor: WorkContext): Promise<PosDayUnit<Effect>>;
}) {
    const businessDate = (timezone = "Asia/Bangkok", date = new Date(ports.clock.now())) => evaluateBusinessDate(timezone, date);
    function fail(code: string, message: string, status = 409): never { throw ports.error(status, code, message); }
    const scopeRow = (actor: WorkContext, row: PosRow | null) => domainScopeRow(actor, row, ports.error);
    async function capturePosReceipt(db: PosDayUnit<Effect>, saleId: string) {
        const sale = (await db.readPosSales1({ saleId: saleId }))!;
        if (sale.state !== 'completed')
            fail('POS_RECEIPT_UNPAID', 'Complete and receive payment before printing');
        const customer = sale.party_id ? await db.readFinanceParties1({ party_id: sale.party_id, company_id: sale.company_id }) : null;
        const lines = await db.readPosLines1({ saleId: saleId });
        const payments = await db.readPosPayments1({ saleId: saleId });
        const roomCharge = await db.readPmsSources1({ fact0: `pos:${saleId}:${saleId}` });
        const snapshot: PosReceiptSnapshot = { schema: 1, saleId, number: str(sale, 'number'), currency: str(sale, 'currency'), businessDate: str(sale, 'business_date'), issuer: JSON.parse(str(sale, 'issuer_json') || '{}'), customer, lines, payments, subtotalMinor: num(sale, 'subtotal_minor'), discountMinor: num(sale, 'discount_minor'), taxMinor: num(sale, 'tax_minor'), totalMinor: num(sale, 'total_minor'), roomCharge };
        await db.writePosSaleDocuments1({ saleId: saleId, company_id: sale.company_id, branch_id: sale.branch_id, fact3: JSON.stringify(snapshot), fact4: ports.clock.now() });
    }
    async function posCustomers(db: PosDayUnit<Effect>, actor: WorkContext, query = '') {
        ports.require(actor, 'pos.read');
        if (actor.departmentId)
            fail('POS_BRANCH_REQUIRED', 'Select a Company or Branch', 403);
        const q = query.trim().slice(0, 100), pattern = `%${q.replace(/[\\%_]/g, '\\$&')}%`;
        return db.readFinanceParties2({ companyId: actor.companyId, q: q, pattern: pattern, pattern2: pattern, pattern3: pattern });
    }
    async function createPosCustomer(db: PosDayUnit<Effect>, actor: WorkContext, input: unknown) {
        const p = z.object({ name: z.string().trim().min(1).max(200), phone: z.string().trim().max(50).default(''), email: z.union([z.email(), z.literal('')]).default(''), address: z.object({ line1: z.string().trim().max(200), line2: z.string().trim().max(200).default(''), city: z.string().trim().max(100), region: z.string().trim().max(100).default(''), postalCode: z.string().trim().max(30), countryCode: z.literal('TH').default('TH') }).nullable().default(null) }).parse(input);
        const duplicate = await db.readFinanceParties3({ companyId: actor.companyId, name: p.name, fact2: p.phone.replace(/\D/g, ''), fact3: p.phone.replace(/\D/g, ''), email: p.email, email2: p.email });
        if (duplicate)
            fail('POS_CUSTOMER_DUPLICATE', `พบลูกค้าที่อาจซ้ำ: ${duplicate.name} กรุณาค้นหาและเลือกลูกค้าเดิม`, 422);
        const customer = ports.uuid(), settings = await db.readPosSettings1({ companyId: actor.companyId });
        if (!settings)
            fail('POS_SETUP_REQUIRED', 'Configure POS before creating customers');
        await db.writeFinanceParties1({ customer: customer, companyId: actor.companyId, fact2: `POS-${customer.slice(0, 8)}`, name: p.name, currency: settings.currency, fact5: p.phone || null, fact6: p.email || null, fact7: JSON.stringify(p.address ?? {}), fact8: ports.clock.now(), fact9: ports.clock.now() });
        return { id: customer, name: p.name };
    }
    async function assertPosDayWritable(db: PosDayUnit<Effect>, company: string, branch: string, date: string) {
        if (await db.readPosDayCloses1({ company: company, branch: branch, date: date }))
            fail('POS_DAY_CLOSED', 'วันธุรกิจนี้เตรียมปิดแล้ว ใช้รายการปรับปรุงในวันปัจจุบันและให้ Finance ตรวจสอบ');
    }
    async function posDayReport(db: PosDayUnit<Effect>, actor: WorkContext, branch: string, date: string) {
        ports.require(actor, 'pos.reports');
        if (actor.departmentId || actor.branchId && actor.branchId !== branch)
            fail('POS_NOT_FOUND', 'Branch is outside the current workspace', 404);
        if (!await db.readOrganizationNodes1({ branch: branch, companyId: actor.companyId }))
            fail('POS_NOT_FOUND', 'Branch not found', 404);
        const company = actor.companyId;
        const sales = await db.readPosSales2({ company: company, branch: branch, date: date });
        const shifts = await db.readPosShifts1({ company: company, branch: branch, date: date });
        const payments = await db.readPosPayments2({ company: company, branch: branch, date: date });
        const refunds = await db.readPosRefunds1({ company: company, branch: branch, date: date });
        const refundPayments = await db.readPosRefundPayments1({ company: company, branch: branch, date: date });
        const stockMoves = await db.readPosStockMoves1({ company: company, date: date, company2: company, branch: branch });
        const journals = await db.readFinanceJournals1({ company: company, date: date, branch: branch });
        const costs = await db.readFinanceJournalLines1({ company: company, date: date, branch: branch });
        const waste = await db.readFinanceJournalLines2({ company: company, date: date, branch: branch });
        const settings = (await db.readPosSettings2({ company: company }))!;
        const blockers: Array<{
            code: string;
            id: string;
            message: string;
        }> = [];
        for (const s of sales)
            if (['open', 'held', 'payment_pending'].includes(str(s, 'state')))
                blockers.push({ code: 'OPEN_SALE', id: str(s, 'id'), message: `จัดการบิล ${s.number} ให้เสร็จ` });
        for (const s of sales)
            if (s.state === 'completed' && !s.room_charge_id && !s.invoice_id)
                blockers.push({ code: 'SALE_POSTING', id: str(s, 'id'), message: 'บิลขายที่สำเร็จต้องมีใบแจ้งหนี้และ journal ต้นทาง' });
        for (const s of shifts)
            if (s.state !== 'reviewed')
                blockers.push({ code: 'SHIFT_REVIEW', id: str(s, 'id'), message: 'ปิดกะและให้ผู้ตรวจอีกคนตรวจยอดเงินสด' });
        for (const p of payments)
            if (['requested', 'pending', 'exception'].includes(str(p, 'state')))
                blockers.push({ code: p.method === 'transfer' ? 'TRANSFER_EVIDENCE' : 'PROVIDER_POSTING', id: str(p, 'id'), message: 'ยืนยันหลักฐานรับเงินและลงบัญชีให้สำเร็จ' });
        for (const p of payments)
            if (p.state === 'succeeded' && !p.finance_payment_id)
                blockers.push({ code: 'RECEIPT_POSTING', id: str(p, 'id'), message: 'รับเงินสำเร็จแต่ยังไม่มีใบรับเงิน Finance' });
        for (const r of refunds)
            if (['requested', 'approved', 'processing', 'exception'].includes(str(r, 'state')))
                blockers.push({ code: 'REFUND_PENDING', id: str(r, 'id'), message: 'จัดการรายการคืนเงินค้าง' });
        const kitchen = await db.readPosFulfillments1({ company: company, branch: branch, date: date });
        for (const f of kitchen)
            blockers.push({ code: 'KITCHEN_PENDING', id: str(f, 'id'), message: 'ส่งมอบหรือยกเลิกรายการครัวพร้อมบันทึกของเสีย' });
        for (const j of journals)
            if (j.status !== 'posted')
                blockers.push({ code: 'JOURNAL_PENDING', id: str(j, 'id'), message: 'Finance ต้องลงบัญชีรายการต้นทาง' });
        const pendingCash = await db.readPosCashMovements1({ company: company, branch: branch, date: date });
        for (const m of pendingCash)
            blockers.push({ code: 'CASH_REVIEW', id: str(m, 'id'), message: 'อนุมัติเงินเข้า/ออกลิ้นชัก' });
        const completed = sales.filter(s => s.state === 'completed' && !s.room_charge_id), sum = (rows: PosRow[], key: string) => rows.reduce((n, r) => n + num(r, key), 0);
        const completedRefunds = refunds.filter(r => r.state === 'completed' && r.kind === 'sale');
        const refundMinor = sum(completedRefunds, 'amount_minor'), refundTaxMinor = sum(completedRefunds, 'tax_minor');
        const received: Record<string, number> = {}, returned: Record<string, number> = {};
        for (const p of payments.filter(p => p.state === 'succeeded'))
            received[str(p, 'method')] = (received[str(p, 'method')] ?? 0) + num(p, 'amount_minor');
        for (const p of refundPayments.filter(p => p.state === 'succeeded'))
            returned[str(p, 'method')] = (returned[str(p, 'method')] ?? 0) + num(p, 'amount_minor');
        const totals = { bills: completed.length, grossMinor: sum(completed, 'subtotal_minor') + sum(completed, 'discount_minor'), discountMinor: sum(completed, 'discount_minor'), salesMinor: sum(completed, 'subtotal_minor'), taxMinor: sum(completed, 'tax_minor'), refundMinor, refundTaxMinor, netTaxMinor: sum(completed, 'tax_minor') - refundTaxMinor, netMinor: sum(completed, 'total_minor') - refundMinor, costMinor: num(costs!, 'n'), wasteMinor: num(waste!, 'n'), cashVarianceMinor: sum(shifts, 'variance_minor'), roomChargeMinor: sum(sales.filter(s => s.state === 'completed' && s.room_charge_id && !s.reversal_entry_id), 'total_minor'), received, returned };
        const pendingPayouts = await db.readPosSettlements1({ company: company });
        const stripeFunds = await db.readFinanceJournalLines3({ company: company });
        const sources = { sales, shifts, payments, refunds, refundPayments, stockMoves, journals, kitchen, pendingCash };
        // Keep the original digest contract so already-prepared immutable snapshots
        // remain confirmable when additional presentation totals are introduced.
        const digestTotals = Object.fromEntries(Object.entries(totals).filter(([key]) => !['refundTaxMinor', 'netTaxMinor'].includes(key)));
        const digest = ports.hash(JSON.stringify({ sources, totals: digestTotals }));
        return { companyId: company, branchId: branch, businessDate: date, currency: str(settings, 'currency'), totals, blockers, sources, pendingPayouts, stripeClearingMinor: num(stripeFunds!, 'n'), digest };
    }
    async function posDayCommand(db: PosDayUnit<Effect>, actor: WorkContext, op: string, input: unknown) {
        if (op === 'day.prepare') {
            ports.require(actor, 'pos.approve');
            ports.require(actor, 'pos.reports');
            const p = z.object({ branchId: z.string().min(1), businessDate: z.iso.date() }).parse(input);
            if (p.businessDate > businessDate())
                fail('POS_FUTURE_DATE', 'Close only the current or an earlier business date', 422);
            await assertPosDayWritable(db, actor.companyId, p.branchId, p.businessDate);
            const report = await posDayReport(db, actor, p.branchId, p.businessDate);
            if (report.blockers.length)
                fail('POS_DAY_BLOCKED', report.blockers.map(b => b.message).join(' · '));
            if (!report.sources.shifts.length)
                fail('POS_DAY_EMPTY', 'No cash sessions exist for this business date', 422);
            const close = ports.uuid();
            await db.writePosDayCloses1({ close: close, companyId: actor.companyId, branchId: p.branchId, businessDate: p.businessDate, currency: report.currency, fact5: JSON.stringify(report), digest: report.digest, userId: actor.userId, fact8: ports.clock.now() });
            return { id: close, state: 'prepared' };
        }
        if (op === 'day.confirm') {
            ports.require(actor, 'finance.approve');
            ports.require(actor, 'pos.reports');
            const p = z.object({ id: z.string().min(1), expectedVersion: z.number().int().positive() }).parse(input);
            const close = scopeRow(actor, await db.readPosDayCloses2({ id: p.id }));
            if (close.version !== p.expectedVersion)
                fail('POS_VERSION_CONFLICT', 'Reload the current close');
            if (close.state !== 'prepared' || samePerson(actor, close.prepared_by))
                fail('POS_INDEPENDENT_REVIEW', 'Finance อีกคนต้องยืนยันรายงานปิดวัน');
            const report = await posDayReport(db, actor, str(close, 'branch_id'), str(close, 'business_date'));
            if (report.blockers.length || report.digest !== close.source_digest)
                fail('POS_DAY_CHANGED', 'รายการต้นทางเปลี่ยนหลังเตรียมปิดวัน ให้ตรวจรายการปรับปรุงกับ Finance');
            await db.writePosDayCloses2({ userId: actor.userId, fact1: ports.clock.now(), id: close.id });
            return { id: close.id, state: 'confirmed' };
        }
        fail('POS_ACTION_UNKNOWN', 'Unknown day close command', 404);
    }
    return { capturePosReceipt, posCustomers, createPosCustomer, assertPosDayWritable, posDayReport, posDayCommand };
}
