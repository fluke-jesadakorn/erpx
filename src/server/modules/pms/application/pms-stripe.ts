import { canonicalGuestAccount, legacyGuestAccount } from "@/lib/guest-account-compat";
import { z } from 'zod';
import { availableReceiptSources } from '@/lib/pms-receipt-funding';
import { record, type Snapshot } from "../domain/pms";
import { executePms } from '@/lib/pms-engine';
import { samePerson } from '@/lib/iam';
import { hasPermission } from '@/lib/permissions';
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { Effects } from "@/lib/pms-engine";
import type { Clock } from "../../shared/application/ports";
export type PmsProviderRows<Row> = {
    results: Row[];
};
export type PmsProviderWrite = {
    run(): Promise<PmsProviderRows<Record<string, unknown>>>;
};
export type PmsCounterPaymentUnit = {
    authorize(actor: WorkContext, permission: string): Promise<void>;
    readPmsStripePayments(facts: {
        key: string;
    }): Promise<{
        id: string;
        request_hash: string;
    } | null>;
    readPosConnections(facts: {
        companyId: string;
    }): Promise<{
        account_id: string;
        livemode: boolean;
    } | null>;
    readPosSettings(facts: {
        companyId: string;
    }): Promise<{
        accounts_json: string;
    } | null>;
    insertPmsStripePayments(facts: {
        key: string;
        digest: string;
        propertyId: string;
        guestAccountId: string;
        companyId: string;
        account_id: string;
        livemode: boolean;
        kind: "deposit" | "receipt";
        amountMinor: number;
        fact9: string | null;
        fact10: number;
        userId: string;
        now: number;
        now2: number;
        clearing: unknown;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsStripePayments2(facts: {
        id: string;
    }): Promise<Payment | null>;
    updatePmsStripePayments(facts: {
        id: string;
        qr: string;
        now: number;
        id2: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsStripePayments3(facts: {
        id: string;
        companyId: string;
    }): Promise<Payment | null>;
    readPmsStripePayments4(facts: {
        id: string;
    }): Promise<Payment | null>;
    readPmsStripePayments5(facts: {
        id: string;
    }): Promise<Payment | null>;
    updatePmsStripePayments2(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments3(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments4(facts: {
        succeededAt: number;
        succeededAt2: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readWriteContext(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readPmsStripePayments6(facts: {
        id: string;
    }): Promise<Payment | null>;
    updatePmsStripePayments5(facts: {
        id: string;
        now: number;
        amount_minor: number;
        now2: number;
        id2: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsProperties(facts: {
        version: number;
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments6(facts: {
        id: string;
        now: number;
        now2: number;
        id2: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments7(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsStripePayments7(facts: {
        fact0: number;
        fact1: number;
    }): Promise<PmsProviderRows<{
        id: string;
    }>>;
    updatePmsStripePayments8(facts: {
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments9(facts: {
        fact0: string;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsStripePayments8(facts: Record<string, never>): Promise<PmsProviderRows<{
        id: string;
    }>>;
    updatePmsStripePayments10(facts: {
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments11(facts: {
        fact0: string;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsStripePayments9(facts: {
        paymentId: string;
        companyId: string;
    }): Promise<{
        property_id: string;
    } | null>;
    readPmsStripePayments10(facts: {
        paymentId: string;
        companyId: string;
    }): Promise<Payment | null>;
    updatePmsStripePayments12(facts: {
        amountMinor: number;
        fact1: string;
        userId: string;
        fact3: number;
        paymentId: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsStripePayments11(facts: {
        paymentId: string;
        companyId: string;
    }): Promise<{
        property_id: string;
    } | null>;
    readWriteContext2(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readPmsStripePayments12(facts: {
        paymentId: string;
        companyId: string;
    }): Promise<Payment | null>;
    updatePmsStripePayments13(facts: {
        userId: string;
        fact1: number;
        paymentId: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsStripePayments13(facts: {
        id: string;
    }): Promise<Payment | null>;
    insertPmsCounterRefundAttempts(facts: {
        id: string;
        refund_generation: number;
        refundId: string;
        refund_amount_minor: number;
        fact4: string;
        fact5: string;
        fact6: string;
        now: number;
        now2: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments14(facts: {
        refundId: string;
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsCounterRefundAttempts(facts: {
        now: number;
        id: string;
        refund_generation: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments15(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments16(facts: {
        state: "requires_action" | "processing";
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsCounterRefundAttempts2(facts: {
        state: "requires_action" | "processing";
        now: number;
        id: string;
        refund_generation: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readWriteContext3(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readPmsStripePayments14(facts: {
        id: string;
    }): Promise<Payment | null>;
    updatePmsProperties2(facts: {
        version: number;
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsCounterRefundAttempts3(facts: {
        now: number;
        id: string;
        refund_generation: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments17(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsStripePayments15(facts: {
        companyId: string;
    }): Promise<PmsProviderRows<{
        id: string;
        property_id: string;
        guest_account_id: string;
        amount_minor: number;
        refund_amount_minor: number | null;
        refund_state: string;
        last_error: string | null;
        refund_reason: string | null;
        payer_email: string | null;
    }>>;
    readPmsStripePayments16(facts: {
        id: string;
        companyId: string;
    }): Promise<Payment | null>;
    readPmsStripePayments17(facts: {
        id: string;
    }): Promise<Payment | null>;
    updatePmsStripePayments18(facts: {
        fact0: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsStripePayments19(facts: {
        fact0: string | null;
        fact1: string;
        fact2: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    bookingSnapshot(arg0: string, arg1?: undefined, arg2?: true): Promise<Snapshot>;
    snapshotMoney(arg0: WorkContext, arg1: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
        transactionDate: string;
        currency: string;
        amountUnits: string;
    }): Promise<{
        amountUnits: string;
        usdUnits: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        statement: PmsProviderWrite;
    }>;
    persistPmsRecords(arg0: Snapshot, arg1: Map<string, string>): Promise<void>;
    persistEffects(arg0: WorkContext, arg1: Snapshot, arg2: Effects): Promise<void>;
    persist(arg0: Snapshot, arg1: Map<string, string>): Promise<void>;
};
export type PmsCounterPaymentRepository = PmsCounterPaymentUnit & {
    transaction<Result>(work: (unit: PmsCounterPaymentUnit) => Promise<Result>): Promise<Result>;
};
export type PmsCounterPaymentPorts = {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    liveEnabled(): string | undefined;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    forActor(actor: WorkContext): Promise<PmsCounterPaymentUnit>;
    stripeCall: typeof import("@/server/modules/pos/public/pos-stripe").stripeCall;
    verifiedStripePaymentTime: typeof import("@/server/modules/pos/public/stripe-payment-time").verifiedStripePaymentTime;
};
export type Payment = {
    created_at: number;
    id: string;
    request_hash: string;
    property_id: string;
    guest_account_id: string;
    company_id: string;
    account_id: string;
    livemode: boolean;
    kind: 'receipt' | 'deposit';
    amount_minor: number;
    payer_email: string | null;
    provider_id: string | null;
    qr_data: string | null;
    state: 'requested' | 'pending' | 'succeeded' | 'cancelled' | 'refund_review';
    expires_at: number;
    entry_id: string | null;
    refund_state: string | null;
    refund_amount_minor: number | null;
    refund_reason: string | null;
    refund_requested_by: string | null;
    refund_id: string | null;
    clearing_account: string;
    provider_succeeded_at: number | null;
    refund_reserved: boolean;
    refund_liability_posted: boolean;
    refund_generation: number;
    refund_approved_by: string | null;
};
export function createPmsCounterPayment(ports: PmsCounterPaymentPorts) {
    const inputSchema = z.object({ propertyId: z.string().min(1), guestAccountId: z.string().min(1), kind: z.enum(['receipt', 'deposit']), amountMinor: z.number().int().positive(), payerEmail: z.union([z.email(), z.literal('')]).default('') });
    function visible(row: Payment) { return { id: row.id, state: row.state, kind: row.kind, amountMinor: Number(row.amount_minor), qrData: row.state === 'pending' ? row.qr_data : null, expiresAt: Number(row.expires_at), entryId: row.entry_id, refundState: row.refund_state, payerEmailProvided: Boolean(row.payer_email) }; }
    function scope(actor: WorkContext, s: Snapshot) { if (s.property.companyId !== actor.companyId || actor.branchId && s.property.branchId !== actor.branchId || actor.departmentId)
        throw ports.error(404, 'PMS_NOT_FOUND', 'Hotel not found in this workspace'); }
    async function createPmsStripePayment(db: PmsCounterPaymentRepository, actor: WorkContext, raw: unknown, key: string, now = ports.clock.now()) {
        if (!hasPermission(actor, 'pms.cashier'))
            throw ports.error(403, 'PMS_CASHIER_REQUIRED', 'Cashier access is required');
        z.string().uuid().parse(key);
        const input = inputSchema.parse(canonicalGuestAccount(raw)), digest = ports.hash(JSON.stringify({ actor: actor.userId, company: actor.companyId, input }));
        const id = await db.transaction(async (tx) => {
            const s = await tx.bookingSnapshot(input.propertyId, undefined, true);
            scope(actor, s);
            const old = await tx.readPmsStripePayments({ key: key });
            if (old) {
                if (old.request_hash !== digest && old.request_hash !== ports.hash(JSON.stringify({actor:actor.userId,company:actor.companyId,input:legacyGuestAccount(input)})))
                    throw ports.error(409, 'PMS_PAYMENT_KEY_REUSED', 'Use a new payment request key');
                return old.id;
            }
            if (!s.property.config.onlineAccounting?.approvedBy)
                throw ports.error(409, 'PMS_ONLINE_ACCOUNTING_REQUIRED', 'Finance must approve the online liability mappings');
            if (s.property.state !== 'open' || s.property.config.currency !== 'THB')
                throw ports.error(409, 'PMS_PAYMENT_NOT_READY', 'Hotel must be open and use THB');
            const guestAccount = record(s, 'guest_account', input.guestAccountId);
            if (guestAccount.data.state !== 'open' && guestAccount.data.state !== 'credit')
                throw ports.error(409, 'PMS_GUEST_ACCOUNT_CLOSED', 'Choose an open guest account');
            const connection = await tx.readPosConnections({ companyId: actor.companyId });
            if (!connection?.account_id || connection.livemode && ports.liveEnabled() !== 'true')
                throw ports.error(503, 'PMS_PAYMENT_UNAVAILABLE', 'Company Stripe is not ready for PromptPay');
            const settings = await tx.readPosSettings({ companyId: actor.companyId });
            const clearing = settings ? (JSON.parse(settings.accounts_json).stripe || s.property.config.accounts.edc) : s.property.config.accounts.edc;
            await tx.insertPmsStripePayments({ key: key, digest: digest, propertyId: input.propertyId, guestAccountId: input.guestAccountId, companyId: actor.companyId, account_id: connection.account_id, livemode: connection.livemode, kind: input.kind, amountMinor: input.amountMinor, fact9: input.payerEmail || null, fact10: now + 30 * 60 * 1000, userId: actor.userId, now: now, now2: now, clearing: clearing });
            const money = await tx.snapshotMoney(actor, { sourceType: 'pms_stripe_payment', sourceId: key, amountKind: 'requested', transactionDate: s.property.businessDate, currency: 'THB', amountUnits: String(input.amountMinor) });
            await money.statement.run();
            return key;
        });
        return ensurePmsStripeQr(db, id, now);
    }
    async function ensurePmsStripeQr(db: PmsCounterPaymentRepository, id: string, now = ports.clock.now()) {
        const row = await db.readPmsStripePayments2({ id: id });
        if (!row)
            throw ports.error(404, 'PMS_PAYMENT_NOT_FOUND', 'Payment request not found');
        if (row.state !== 'requested' || row.provider_id)
            return visible(row);
        if (Number(row.expires_at) <= now)
            throw ports.error(409, 'PMS_PAYMENT_EXPIRED', 'Create a new payment request');
        const pi = await ports.stripeCall(Boolean(row.livemode), 'payment_intents', { method: 'POST', account: row.account_id, key: `pms-counter-${id}`, body: { amount: String(row.amount_minor), currency: 'thb', 'payment_method_types[0]': 'promptpay', 'payment_method_data[type]': 'promptpay', confirm: 'true', 'metadata[pms_counter_payment_id]': id, ...(row.payer_email ? { receipt_email: row.payer_email } : {}) } });
        const qr = ((pi.next_action as Record<string, unknown> | undefined)?.promptpay_display_qr_code as Record<string, unknown> | undefined)?.data;
        if (pi.status !== 'requires_action' || typeof qr !== 'string' || !qr)
            throw ports.error(502, 'PMS_PROMPTPAY_QR_UNAVAILABLE', 'Stripe did not return a PromptPay QR');
        await db.updatePmsStripePayments({ id: pi.id, qr: qr, now: now, id2: id });
        return visible({ ...row, provider_id: pi.id, qr_data: qr, state: 'pending' });
    }
    async function pmsStripePaymentStatus(db: PmsCounterPaymentRepository, actor: WorkContext, id: string, now = ports.clock.now()) {
        if (!hasPermission(actor, 'pms.cashier'))
            throw ports.error(403, 'PMS_CASHIER_REQUIRED', 'Cashier access is required');
        const row = await db.readPmsStripePayments3({ id: id, companyId: actor.companyId });
        if (!row)
            throw ports.error(404, 'PMS_PAYMENT_NOT_FOUND', 'Payment request not found');
        const s = await db.bookingSnapshot(row.property_id);
        scope(actor, s);
        await reconcilePmsStripePayment(db, id, now);
        const latest = (await db.readPmsStripePayments2({ id: id }))!;
        return visible(latest);
    }
    async function reconcilePmsStripePayment(db: PmsCounterPaymentRepository, id: string, now = ports.clock.now()) {
        const row = await db.readPmsStripePayments2({ id: id });
        if (!row || !['pending', 'requested', 'cancelled'].includes(row.state))
            return;
        if (!row.provider_id) {
            if (row.state === 'requested' && Number(row.expires_at) <= now)
                await db.updatePmsStripePayments2({ now: now, id: id });
            return;
        }
        const pi = await ports.stripeCall(Boolean(row.livemode), `payment_intents/${row.provider_id}`, { account: row.account_id });
        if (pi.status === 'succeeded') {
            if (pi.amount_received !== Number(row.amount_minor) || pi.currency !== 'thb' || (pi.metadata as Record<string, string> | undefined)?.pms_counter_payment_id !== id)
                throw ports.error(409, 'PMS_PROVIDER_PAYMENT_MISMATCH', 'Provider payment needs review');
            await db.updatePmsStripePayments3({ now: now, id: id });
            const succeededAt = await ports.verifiedStripePaymentTime({ livemode: Boolean(row.livemode), accountId: row.account_id, paymentIntentId: String(pi.id), known: row.provider_succeeded_at, createdAt: Number(row.created_at), now });
            await db.updatePmsStripePayments4({ succeededAt: succeededAt, succeededAt2: succeededAt, id: id });
            await db.transaction(async (tx) => {
                const s = await tx.bookingSnapshot(row.property_id, undefined, true);
                await tx.readWriteContext({});
                const current = await tx.readPmsStripePayments6({ id: id });
                if (!current || current.state === 'succeeded' || current.state === 'refund_review')
                    return;
                const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)])), oldEdc = s.property.config.accounts.edc;
                s.property.config.accounts.edc = current.clearing_account;
                if (current.state === 'cancelled' || (current.provider_succeeded_at ?? now) >= Number(current.expires_at)) {
                    const outcome = executePms(s, 'payment.unallocated', { guestAccountId: current.guest_account_id, amount: Number(current.amount_minor), reference: String(current.provider_id) }, { userId: 'system:stripe', now, uuid: ports.uuid, independent: () => false });
                    record(s, 'entry', outcome.id).data.sourceKey = `counter-payment:${current.id}`;
                    await tx.persistPmsRecords(s, before);
                    await tx.persistEffects({ companyId: current.company_id, userId: 'system:stripe' } as WorkContext, s, outcome.effects);
                    s.property.config.accounts.edc = oldEdc;
                    await tx.persist(s, before);
                    await tx.updatePmsStripePayments5({ id: outcome.id, now: now, amount_minor: current.amount_minor, now2: now, id2: id });
                    return;
                }
                const outcome = executePms(s, current.kind === 'deposit' ? 'payment.stripe_deposit' : 'payment.stripe', { guestAccountId: current.guest_account_id, amount: Number(current.amount_minor), reference: String(current.provider_id) }, { userId: 'system:stripe', now, uuid: ports.uuid, independent: () => false });
                record(s, 'entry', outcome.id).data.sourceKey = `counter-payment:${current.id}`;
                await tx.persistPmsRecords(s, before);
                await tx.persistEffects({ companyId: current.company_id, userId: 'system:stripe' } as WorkContext, s, outcome.effects);
                s.property.config.accounts.edc = oldEdc;
                await tx.updatePmsProperties({ version: s.property.version, now: now, id: s.property.id });
                await tx.updatePmsStripePayments6({ id: outcome.id, now: now, now2: now, id2: id });
            });
            return;
        }
        if (pi.status === 'canceled') {
            await db.updatePmsStripePayments7({ now: now, id: id });
            return;
        }
        if (now >= Number(row.expires_at) || pi.status === 'requires_payment_method') {
            await ports.stripeCall(Boolean(row.livemode), `payment_intents/${row.provider_id}/cancel`, { method: 'POST', account: row.account_id, key: `pms-counter-cancel-${id}`, body: {} });
            await reconcilePmsStripePayment(db, id, now);
        }
    }
    async function drainPmsStripePayments(db: PmsCounterPaymentRepository, now = ports.clock.now()) {
        const due = (await db.readPmsStripePayments7({ fact0: now - 300000, fact1: now - 30 * 86400000 })).results;
        for (const row of due) {
            try {
                await reconcilePmsStripePayment(db, row.id, now);
                await db.updatePmsStripePayments8({ id: row.id });
            }
            catch (error) {
                await db.updatePmsStripePayments9({ fact0: error instanceof Error ? error.message.slice(0, 100) : 'PMS_PAYMENT_REVIEW', id: row.id });
            }
        }
        const refunds = (await db.readPmsStripePayments8({})).results;
        for (const row of refunds) {
            try {
                await processPmsStripeRefund(db, row.id, now);
                await db.updatePmsStripePayments8({ id: row.id });
            }
            catch (error) {
                await db.updatePmsStripePayments11({ fact0: error instanceof Error ? error.message.slice(0, 100) : 'PMS_REFUND_REVIEW', id: row.id });
            }
        }
    }
    function remainingReceipt(s: Snapshot, row: Payment) {
        if (!row.entry_id)
            throw ports.error(409, 'PMS_ORIGINAL_RECEIPT_MISSING', 'Original receipt requires recovery');
        return availableReceiptSources(s, row.guest_account_id, row.kind === 'deposit' ? 'deposit' : 'advance', false).find(p => p.entryId === row.entry_id)?.amount ?? 0;
    }
    async function requestPmsStripeRefund(db: PmsCounterPaymentRepository, actor: WorkContext, paymentId: string, amountMinor: number, reason: string) {
        if (!hasPermission(actor, 'pms.cashier'))
            throw ports.error(403, 'PMS_CASHIER_REQUIRED', 'Cashier access is required');
        if (!Number.isSafeInteger(amountMinor) || amountMinor <= 0 || reason.trim().length < 3)
            throw ports.error(422, 'PMS_REFUND_INPUT', 'Enter a valid amount and reason');
        const found = await db.readPmsStripePayments9({ paymentId: paymentId, companyId: actor.companyId });
        if (!found)
            throw ports.error(404, 'PMS_PAYMENT_NOT_FOUND', 'Payment not found');
        return db.transaction(async (tx) => {
            const s = await tx.bookingSnapshot(found.property_id, undefined, true);
            scope(actor, s);
            const row = await tx.readPmsStripePayments10({ paymentId: paymentId, companyId: actor.companyId });
            if (!row || row.state !== 'succeeded' || row.refund_state && row.refund_state !== 'succeeded')
                throw ports.error(409, 'PMS_REFUND_NOT_AVAILABLE', 'Choose an unsettled Stripe receipt');
            if (!row.payer_email)
                throw ports.error(409, 'PMS_OFFLINE_REFUND_REQUIRED', 'Use the independently approved cash or bank refund with payout evidence');
            if (amountMinor > Number(row.amount_minor) || amountMinor > remainingReceipt(s, row))
                throw ports.error(422, 'PMS_REFUND_EXCEEDS_PAYMENT', 'Refund exceeds the unused original receipt');
            const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)]));
            executePms(s, 'payment.refund_reserve', { guestAccountId: row.guest_account_id, amount: amountMinor, sourceEntryId: row.entry_id, pocket: row.kind === 'deposit' ? 'deposit' : 'advance' }, { userId: 'system:stripe', now: ports.clock.now(), uuid: ports.uuid, independent: () => false });
            await tx.persist(s, before);
            await tx.updatePmsStripePayments12({ amountMinor: amountMinor, fact1: reason.trim(), userId: actor.userId, fact3: ports.clock.now(), paymentId: paymentId });
            return { id: paymentId, state: 'requested' };
        });
    }
    async function approvePmsStripeRefund(db: PmsCounterPaymentRepository, actor: WorkContext, paymentId: string) {
        if (!hasPermission(actor, 'pms.approve') || actor.departmentId) throw ports.error(403,'PMS_APPROVAL_REQUIRED','PMS refund approval is required');
        const found = await db.readPmsStripePayments9({ paymentId: paymentId, companyId: actor.companyId });
        if (!found)
            throw ports.error(404, 'PMS_PAYMENT_NOT_FOUND', 'Payment not found');
        return db.transaction(async (tx) => {
            const s = await tx.bookingSnapshot(found.property_id, undefined, true);
            scope(actor, s);
            await tx.authorize(actor,'pms.approve');
            await tx.readWriteContext({});
            const row = await tx.readPmsStripePayments10({ paymentId: paymentId, companyId: actor.companyId });
            if (!row || row.refund_state !== 'requested' || samePerson(actor, row.refund_requested_by ?? ''))
                throw ports.error(409, 'PMS_INDEPENDENT_REFUND_APPROVAL', 'A different PMS manager must approve a pending refund');
            if (row.state === 'succeeded' && !row.refund_liability_posted) {
                const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)])), n = Number(row.refund_amount_minor), pocket = row.kind === 'deposit' ? 'deposit' : 'advance';
                if (n > remainingReceipt(s, row))
                    throw ports.error(409, 'PMS_REFUND_EXCEEDS_BALANCE', 'Original unused receipt changed');
                if (!row.refund_reserved)
                    executePms(s, 'payment.refund_reserve', { guestAccountId: row.guest_account_id, amount: n, pocket, sourceEntryId: row.entry_id }, { userId: 'system:stripe', now: ports.clock.now(), uuid: ports.uuid, independent: () => false });
                const outcome = executePms(s, 'payment.refund_liability', { guestAccountId: row.guest_account_id, amount: n, pocket, sourceEntryId: row.entry_id, reference: row.id }, { userId: 'system:stripe', now: ports.clock.now(), uuid: ports.uuid, independent: () => false });
                await tx.persist(s, before);
                await tx.persistEffects(actor, s, outcome.effects);
            }
            if (!s.property.config.onlineAccounting?.approvedBy || !row.entry_id)
                throw ports.error(409, 'PMS_REFUND_ACCOUNTING_REQUIRED', 'Recover the original receipt and approve online liability mappings');
            await tx.updatePmsStripePayments13({ userId: actor.userId, fact1: ports.clock.now(), paymentId: paymentId });
            return { id: paymentId, state: 'approved' };
        });
    }
    async function processPmsStripeRefund(db: PmsCounterPaymentRepository, id: string, now = ports.clock.now()) {
        const row = await db.readPmsStripePayments2({ id: id });
        if (!row || !['approved', 'processing', 'requires_action'].includes(row.refund_state ?? ''))
            return;
        if (!row.refund_liability_posted)
            throw ports.error(409, 'PMS_REFUND_ACCOUNTING_REQUIRED', 'Independent Finance liability review is required');
        if (!row.provider_id || !row.refund_amount_minor || !row.payer_email)
            throw ports.error(409, 'PMS_REFUND_PROVIDER_MISSING', 'Provider payment or refund email is missing');
        let refundId = row.refund_id;
        if (!refundId) {
            const pi = await ports.stripeCall(Boolean(row.livemode), `payment_intents/${row.provider_id}`, { account: row.account_id });
            if (pi.status !== 'succeeded' || pi.amount_received !== Number(row.amount_minor) || (pi.metadata as Record<string, string> | undefined)?.pms_counter_payment_id !== id)
                throw ports.error(409, 'PMS_REFUND_PAYMENT_MISMATCH', 'Provider payment does not match');
            const refund = await ports.stripeCall(Boolean(row.livemode), 'refunds', { method: 'POST', account: row.account_id, key: `pms-counter-refund-${id}${row.refund_generation ? '-' + row.refund_generation : ''}`, body: { payment_intent: pi.id, amount: String(row.refund_amount_minor), instructions_email: row.payer_email, 'metadata[pms_counter_payment_id]': id, 'metadata[pms_counter_refund_generation]': String(row.refund_generation) } });
            refundId = refund.id;
            await db.insertPmsCounterRefundAttempts({ id: id, refund_generation: row.refund_generation, refundId: refundId, refund_amount_minor: row.refund_amount_minor, fact4: row.refund_reason ?? 'Counter refund', fact5: row.refund_requested_by ?? 'system:stripe', fact6: row.refund_approved_by ?? 'legacy:review', now: now, now2: now });
            await db.updatePmsStripePayments14({ refundId: refundId, now: now, id: id });
        }
        const refund = await ports.stripeCall(Boolean(row.livemode), `refunds/${refundId}`, { account: row.account_id });
        if (refund.amount !== Number(row.refund_amount_minor) || (refund.metadata as Record<string, string> | undefined)?.pms_counter_payment_id !== id || (row.refund_generation > 0 && (refund.metadata as Record<string, string> | undefined)?.pms_counter_refund_generation !== String(row.refund_generation)))
            throw ports.error(409, 'PMS_REFUND_PROVIDER_MISMATCH', 'Provider refund does not match');
        if (refund.status === 'failed' || refund.status === 'canceled') {
            await db.updatePmsCounterRefundAttempts({ now: now, id: id, refund_generation: row.refund_generation });
            await db.updatePmsStripePayments15({ now: now, id: id });
            return;
        }
        if (refund.status !== 'succeeded') {
            const state = refund.status === 'requires_action' ? 'requires_action' : 'processing';
            await db.updatePmsStripePayments16({ state: state, now: now, id: id });
            await db.updatePmsCounterRefundAttempts2({ state: state, now: now, id: id, refund_generation: row.refund_generation });
            return;
        }
        await db.transaction(async (tx) => {
            const s = await tx.bookingSnapshot(row.property_id, undefined, true);
            await tx.readWriteContext({});
            const current = await tx.readPmsStripePayments6({ id: id });
            if (current?.refund_state === 'succeeded')
                return;
            if (!current || !['processing', 'requires_action'].includes(current.refund_state ?? ''))
                throw ports.error(409, 'PMS_REFUND_STATE', 'Refund state changed');
            if (['succeeded', 'refund_review'].includes(current.state)) {
                const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)]));
                const oldEdc = s.property.config.accounts.edc;
                s.property.config.accounts.edc = current.clearing_account;
                const outcome = executePms(s, 'payment.refund_settle', { guestAccountId: current.guest_account_id, amount: Number(current.refund_amount_minor), reference: String(refundId), sourceEntryId: current.entry_id, pocket: current.kind === 'deposit' ? 'deposit' : 'advance' }, { userId: 'system:stripe', now, uuid: ports.uuid, independent: () => false });
                await tx.persistPmsRecords(s, before);
                await tx.persistEffects({ companyId: current.company_id, userId: 'system:stripe' } as WorkContext, s, outcome.effects);
                s.property.config.accounts.edc = oldEdc;
                await tx.updatePmsProperties({ version: s.property.version, now: now, id: s.property.id });
            }
            await tx.updatePmsCounterRefundAttempts3({ now: now, id: id, refund_generation: current.refund_generation });
            await tx.updatePmsStripePayments17({ now: now, id: id });
        });
    }
    async function pmsStripeRefundsForFinance(actor: WorkContext) {
        if (!hasPermission(actor, 'finance.read') || actor.branchId || actor.departmentId) return [];
        const write = await ports.forActor(actor);
        return (await write.readPmsStripePayments15({ companyId: actor.companyId })).results;
    }
    async function recoverPmsStripeRefund(db: PmsCounterPaymentRepository, actor: WorkContext, id: string, reason: string, payerEmail?: string) {
        if (!hasPermission(actor, 'pms.approve') || actor.departmentId || reason.trim().length < 3)
            throw ports.error(403, 'PMS_FINANCE_REQUIRED', 'Use company Finance and enter a recovery reason');
        if (payerEmail !== undefined)
            z.email().max(254).parse(payerEmail);
        const found = await db.readPmsStripePayments3({ id: id, companyId: actor.companyId });
        if (!found || !['approved', 'processing', 'requires_action', 'failed'].includes(found.refund_state ?? '') || samePerson(actor, found.refund_requested_by ?? ''))
            throw ports.error(409, 'PMS_REFUND_RECOVERY_UNAVAILABLE', 'Choose an independently approved provider refund');
        const provider = found.refund_id ? await ports.stripeCall(Boolean(found.livemode), `refunds/${found.refund_id}`, { account: found.account_id }) : null;
        if (found.refund_state === 'failed' && !provider)
            throw ports.error(409, 'PMS_REFUND_PROVIDER_MISSING', 'Recover the original provider refund identity');
        if (found.refund_state === 'failed' && !['failed', 'canceled'].includes(String(provider?.status)))
            throw ports.error(409, 'PMS_REFUND_PROVIDER_PENDING', 'Recover the existing provider refund without creating another');
        return db.transaction(async (tx) => {
            const s=await tx.bookingSnapshot(found.property_id, undefined, true);
            scope(actor,s);await tx.authorize(actor,'pms.approve');
            const current = await tx.readPmsStripePayments6({ id: id });
            if (!current || current.refund_id !== found.refund_id || current.refund_state !== found.refund_state)
                throw ports.error(409, 'PMS_REFUND_CHANGED', 'Reload before recovering this refund');
            if (found.refund_state === 'failed')
                await tx.updatePmsStripePayments18({ fact0: ports.clock.now(), id: id });
            await tx.updatePmsStripePayments19({ fact0: payerEmail ?? null, fact1: `Recovery approved by ${actor.userId}: ${reason.trim()}`.slice(0, 500), fact2: ports.clock.now(), id: id });
            return { id, state: 'recovery_approved' };
        });
    }
    return { createPmsStripePayment, pmsStripePaymentStatus, reconcilePmsStripePayment, drainPmsStripePayments, requestPmsStripeRefund, approvePmsStripeRefund, processPmsStripeRefund, pmsStripeRefundsForFinance, recoverPmsStripeRefund };
}
