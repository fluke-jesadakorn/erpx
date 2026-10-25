import { hasPermission } from "@/lib/permissions";
import { parseGuestAccountJson } from "@/lib/guest-account-compat";
import { record, records, type Snapshot } from "../domain/pms";
import { executePms } from '@/lib/pms-engine';
import { samePerson } from '@/lib/iam';
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { Effects } from "@/lib/pms-engine";
import type { Clock } from "../../shared/application/ports";
import type { BookingPaymentRow, BookingEnv } from "../public/pms-booking";
export type PmsProviderRows<Row> = {
    results: Row[];
};
export type PmsProviderWrite = {
    run(): Promise<PmsProviderRows<Record<string, unknown>>>;
};
export type PmsBookingRefundUnit = {
    authorize(actor: WorkContext, permission: string): Promise<void>;
    readPmsBookingPayments(facts: {
        paymentId: string;
        id: string;
    }): Promise<BookingPaymentRow | null>;
    readPmsBookingRefundRequests(facts: {
        requestId: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<Request | null>;
    readPmsBookingAllocations(facts: {
        paymentId: string;
    }): Promise<PmsProviderRows<{
        stay_id: string;
        guest_account_id: string;
        entry_id: string;
        amount_minor: number;
    }>>;
    readPmsBookingRefundRequests2(facts: {
        paymentId: string;
    }): Promise<PmsProviderRows<{
        amount_minor: number;
        allocations_json: string;
    }>>;
    insertPmsBookingRefundRequests(facts: {
        requestId: `${string}-${string}-${string}-${string}-${string}`;
        paymentId: string;
        amount: number;
        fact3: string;
        reason: string;
        requestedBy: string;
        now: number;
        now2: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingPayments(facts: {
        now: number;
        now2: number;
        paymentId: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsBookingRefundRequests3(facts: {
        id: string;
        id2: string;
        companyId: string;
    }): Promise<{
        id: string;
        property_id: string;
    } | null>;
    readWriteContext(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readPmsBookingRefundRequests4(facts: {
        id: string;
    }): Promise<Request | null>;
    updatePmsBookingRefundRequests(facts: {
        userId: string;
        fact1: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingPayments2(facts: {
        userId: string;
        fact1: number;
        payment_id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsRecords(facts: {
        companyId: string;
    }): Promise<PmsProviderRows<{
        id: string;
        payment_id: string;
        property_id: string;
        amount_minor: number;
        status: string;
        refund_state: string;
        last_error: string | null;
        reason: string;
        created_at: number;
        needs_receipt_recovery: boolean;
    }>>;
    readPmsBookingPayments2(facts: {
        requestId: string;
        companyId: string;
    }): Promise<BookingPaymentRow | null>;
    readWriteContext2(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readPmsBookingPayments3(facts: {
        id: string;
    }): Promise<{
        status: string;
    } | null>;
    updatePmsBookingPayments3(facts: {
        succeededAt: number;
        succeededAt2: number;
        id: string;
        now: number;
        id2: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsBookingRefundRequests5(facts: {
        id: string;
        id2: string;
    }): Promise<PmsProviderRows<Request>>;
    readPmsBookingPayments4(facts: {
        payment_id: string;
    }): Promise<BookingPaymentRow | null>;
    insertPmsBookingRefundAttempts(facts: {
        id: string;
        generation: number;
        refundId: string;
        now: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingRefundRequests2(facts: {
        refundId: string;
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingPayments4(facts: {
        refundId: string;
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingRefundRequests3(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingPayments5(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingRefundRequests4(facts: {
        state: "requires_action" | "processing";
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readWriteContext3(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readPmsBookingRefundRequests6(facts: {
        id: string;
    }): Promise<Request | null>;
    updatePmsBookingRefundRequests5(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingPayments6(facts: {
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsBookingRefundRequests7(facts: {
        id: string;
        companyId: string;
    }): Promise<(Request & {
        property_id: string;
        account_id: string;
        livemode: boolean;
    }) | null>;
    readPmsBookingRefundRequests8(facts: {
        id: string;
    }): Promise<Request | null>;
    insertPmsBookingRefundAttempts2(facts: {
        id: string;
        generation: number;
        provider_id: string | null;
        fact3: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingRefundRequests6(facts: {
        fact0: string;
        fact1: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingPayments7(facts: {
        fact0: number;
        payment_id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    persist(arg0: Snapshot, arg1: Map<string, string>): Promise<void>;
    bookingSnapshot(arg0: string, arg1?: undefined, arg2?: true): Promise<Snapshot>;
    effects(arg0: WorkContext, arg1: Snapshot, arg2: Effects): Promise<void>;
    queueMail(arg0: Snapshot, arg1: string, arg2: string, arg3: "refund", arg4: BookingEnv, arg5: number): Promise<string>;
};
export type PmsBookingRefundRepository = PmsBookingRefundUnit & {
    transaction<Result>(work: (unit: PmsBookingRefundUnit) => Promise<Result>): Promise<Result>;
};
export type PmsBookingRefundPorts = {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    liveEnabled(): string | undefined;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    forActor(actor: WorkContext): Promise<PmsBookingRefundUnit>;
    stripeCall: typeof import("@/server/modules/pos/public/pos-stripe").stripeCall;
    verifiedStripePaymentTime: typeof import("@/server/modules/pos/public/stripe-payment-time").verifiedStripePaymentTime;
    bookingEnv: typeof import("@/server/modules/pms/public/pms-booking").bookingEnv;
};
export type Part = {
    stayId: string;
    guestAccountId: string;
    entryId: string;
    amount: number;
};
export type Request = {
    id: string;
    payment_id: string;
    amount_minor: number;
    allocations_json: string;
    reason: string;
    requested_by: string;
    approved_by: string | null;
    provider_id: string | null;
    state: string;
    generation: number;
    legacy_review?: boolean;
};
export function createPmsBookingRefund(ports: PmsBookingRefundPorts) {
    const providerActor = (now: number) => ({ userId: 'system:stripe', now, uuid: ports.uuid, independent: () => false });
    /** Called while the Property lock is held. Reserve original receipts before another command can consume them. */
    async function requestBookingRefund(tx: PmsBookingRefundUnit, s: Snapshot, paymentId: string, wanted: Array<{
        stayId: string;
        amount: number;
    }>, reason: string, requestedBy: string, now = ports.clock.now(), requestId = ports.uuid()) {
        const payment = await tx.readPmsBookingPayments({ paymentId: paymentId, id: s.property.id });
        if (!payment || !['confirmed', 'cancelled'].includes(payment.status))
            throw ports.error(409, 'PMS_REFUND_NOT_AVAILABLE', 'Choose a confirmed original payment');
        const prior = await tx.readPmsBookingRefundRequests({ requestId: requestId });
        if (prior) {
            const old = parseGuestAccountJson(prior.allocations_json) as Part[];
            if (prior.payment_id !== paymentId || prior.reason !== reason || prior.requested_by !== requestedBy || JSON.stringify(old.map(p => ({ stayId: p.stayId, amount: p.amount }))) !== JSON.stringify(wanted))
                throw ports.error(409, 'PMS_REFUND_KEY_REUSED', 'Use a new refund request key');
            return prior;
        }
        const sources = (await tx.readPmsBookingAllocations({ paymentId: paymentId })).results;
        const previous = (await tx.readPmsBookingRefundRequests2({ paymentId: paymentId })).results;
        const used = previous.flatMap(r => parseGuestAccountJson(r.allocations_json) as Part[]);
        const parts: Part[] = [];
        for (const selection of wanted) {
            if (!Number.isSafeInteger(selection.amount) || selection.amount <= 0)
                throw ports.error(422, 'PMS_REFUND_INPUT', 'Enter a positive amount');
            const source = sources.find(p => p.stay_id === selection.stayId);
            const consumed = used.filter(p => p.stayId === selection.stayId).reduce((n, p) => n + p.amount, 0) + parts.filter(p => p.stayId === selection.stayId).reduce((n, p) => n + p.amount, 0);
            if (!source || selection.amount > Number(source.amount_minor) - consumed)
                throw ports.error(409, 'PMS_REFUND_EXCEEDS_PAYMENT', 'Refund exceeds the remaining original receipt');
            parts.push({ stayId: source.stay_id, guestAccountId: source.guest_account_id, entryId: source.entry_id, amount: selection.amount });
        }
        const amount = parts.reduce((n, p) => n + p.amount, 0);
        if (!amount || previous.reduce((n, p) => n + Number(p.amount_minor), 0) + amount > Number(payment.amount_minor))
            throw ports.error(409, 'PMS_REFUND_EXCEEDS_PAYMENT', 'Cumulative refunds exceed the payment');
        const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)]));
        for (const part of parts)
            executePms(s, 'payment.refund_reserve', { guestAccountId: part.guestAccountId, amount: part.amount, sourceEntryId: part.entryId }, providerActor(now));
        await tx.persist(s, before);
        await tx.insertPmsBookingRefundRequests({ requestId: requestId, paymentId: paymentId, amount: amount, fact3: JSON.stringify(parts), reason: reason, requestedBy: requestedBy, now: now, now2: now });
        await tx.updatePmsBookingPayments({ now: now, now2: now, paymentId: paymentId });
        return { id: requestId, state: 'requested' as const, amount_minor: amount };
    }
    async function approveBookingRefund(db: PmsBookingRefundRepository, actor: WorkContext, id: string) {
        ports.require(actor, 'pms.approve');
        if (actor.departmentId) throw ports.error(403,'PMS_BRANCH_REQUIRED','Choose a Company or Branch.');
        const found = await db.readPmsBookingRefundRequests3({ id: id, id2: id, companyId: actor.companyId });
        if (!found)
            throw ports.error(409, 'PMS_REFUND_NOT_REQUESTED', 'No refund request awaits approval');
        return db.transaction(async (tx) => {
            const s = await tx.bookingSnapshot(found.property_id, undefined, true);
            if(actor.branchId && s.property.branchId !== actor.branchId) throw ports.error(404,'PMS_NOT_FOUND','Refund not found in this workspace.');
            await tx.authorize(actor,'pms.approve');
            await tx.readWriteContext({});
            const request = await tx.readPmsBookingRefundRequests4({ id: found.id });
            if (!request || request.state !== 'requested' || samePerson(actor, request.requested_by))
                throw ports.error(409, 'PMS_INDEPENDENT_REFUND_APPROVAL', 'A different PMS manager must approve the refund');
            if (!s.property.config.onlineAccounting?.approvedBy)
                throw ports.error(409, 'PMS_ONLINE_ACCOUNTING_REQUIRED', 'Finance must approve the online liability accounts');
            if (!(parseGuestAccountJson(request.allocations_json) as Part[]).length && !records(s, 'entry').some(e => e.data.sourceKey === `online-payment:${request.payment_id}` && e.data.description === 'payment.unallocated'))
                throw ports.error(409, 'PMS_UNALLOCATED_RECEIPT_MISSING', 'Recover and account for the original provider receipt before approving its refund');
            const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)]));
            for (const part of parseGuestAccountJson(request.allocations_json) as Part[]) {
                if (request.legacy_review)
                    executePms(s, 'payment.refund_reserve', { guestAccountId: part.guestAccountId, amount: part.amount, sourceEntryId: part.entryId }, providerActor(ports.clock.now()));
                const outcome = executePms(s, 'payment.refund_liability', { guestAccountId: part.guestAccountId, amount: part.amount, reference: request.id, sourceEntryId: part.entryId }, providerActor(ports.clock.now()));
                await tx.persist(s, before);
                await tx.effects(actor, s, outcome.effects);
            }
            await tx.updatePmsBookingRefundRequests({ userId: actor.userId, fact1: ports.clock.now(), id: request.id });
            await tx.updatePmsBookingPayments2({ userId: actor.userId, fact1: ports.clock.now(), payment_id: request.payment_id });
            return { id: request.id, state: 'approved' };
        });
    }
    async function bookingRefundsForFinance(actor: WorkContext) {
        ports.require(actor, 'finance.read');
        if (actor.branchId || actor.departmentId)
            return [];
        const write = await ports.forActor(actor);
        return (await write.readPmsRecords({ companyId: actor.companyId })).results;
    }
    /** Older refund-review rows can lack their clearing liability. Recover the original verified
     * money once before Finance may approve a provider refund; never collect again. */
    async function recoverBookingReceipt(db: PmsBookingRefundRepository, actor: WorkContext, requestId: string, reason: string) {
        ports.require(actor, 'finance.approve');
        if (actor.branchId || actor.departmentId || reason.trim().length < 3)
            throw ports.error(422, 'PMS_RECEIPT_RECOVERY_INPUT', 'Use company Finance and enter a recovery reason');
        const found = await db.readPmsBookingPayments2({ requestId: requestId, companyId: actor.companyId });
        if (!found?.checkout_id)
            throw ports.error(409, 'PMS_RECEIPT_RECOVERY_UNAVAILABLE', 'Select an original unassigned payment awaiting Finance review');
        const session = await ports.stripeCall(Boolean(found.livemode), `checkout/sessions/${found.checkout_id}`, { account: found.account_id });
        const pi = await ports.stripeCall(Boolean(found.livemode), `payment_intents/${String(session.payment_intent)}`, { account: found.account_id });
        if (session.payment_status !== 'paid' || pi.status !== 'succeeded' || pi.amount_received !== Number(found.amount_minor) || pi.currency !== 'thb' || (pi.metadata as Record<string, string> | undefined)?.pms_booking_payment_id !== found.id)
            throw ports.error(409, 'PMS_PAYMENT_MISMATCH', 'The original provider receipt does not match');
        const now = ports.clock.now(), succeededAt = await ports.verifiedStripePaymentTime({ livemode: Boolean(found.livemode), accountId: found.account_id, paymentIntentId: String(pi.id), known: found.provider_succeeded_at ? Number(found.provider_succeeded_at) : null, createdAt: Number(found.created_at), now });
        return db.transaction(async (tx) => {
            const s = await tx.bookingSnapshot(found.property_id, undefined, true);
            await tx.readWriteContext({});
            const current = await tx.readPmsBookingPayments3({ id: found.id });
            if (current?.status !== 'refund_review')
                throw ports.error(409, 'PMS_PAYMENT_CHANGED', 'Reload the original payment before recovery');
            const existing = records(s, 'entry').find(e => e.data.sourceKey === `online-payment:${found.id}`);
            if (existing) {
                if (existing.data.description !== 'payment.unallocated')
                    throw ports.error(409, 'PMS_RECEIPT_RECOVERY_UNAVAILABLE', 'Recover the original allocated receipt through its owner');
                return { id: requestId, state: 'receipt_recovered' };
            }
            const stay = records(s, 'stay').find(st => st.data.reservationId === found.reservation_id);
            if (!stay)
                throw ports.error(409, 'PMS_RECEIPT_TARGET_MISSING', 'The original booking needs owner review');
            const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)])), clearing = s.property.config.accounts.edc;
            s.property.config.accounts.edc = found.clearing_account;
            const result = executePms(s, 'payment.unallocated', { guestAccountId: stay.data.guestAccountId, amount: Number(found.amount_minor), reference: String(pi.id) }, providerActor(now));
            record(s, 'entry', result.id).data.sourceKey = `online-payment:${found.id}`;
            await tx.persist(s, before);
            await tx.effects(actor, s, result.effects);
            s.property.config.accounts.edc = clearing;
            const reservation = record(s, 'reservation', found.reservation_id);
            reservation.data.history = [...(reservation.data.history ?? []), { at: now, actor: actor.userId, action: `Recovered original unallocated receipt: ${reason.trim()}` }];
            reservation.version++;
            reservation.updatedAt = now;
            await tx.persist(s, before);
            await tx.updatePmsBookingPayments3({ succeededAt: succeededAt, succeededAt2: succeededAt, id: pi.id, now: now, id2: found.id });
            return { id: requestId, state: 'receipt_recovered' };
        });
    }
    async function processBookingRefund(db: PmsBookingRefundRepository, id: string, now = ports.clock.now()) {
        const requests = (await db.readPmsBookingRefundRequests5({ id: id, id2: id })).results;
        for (const request of requests) {
            const payment = await db.readPmsBookingPayments4({ payment_id: request.payment_id });
            if (!payment?.checkout_id)
                throw ports.error(409, 'PMS_REFUND_PROVIDER_MISSING', 'Original Checkout session is missing');
            let refundId = request.provider_id;
            if (!refundId) {
                const session = await ports.stripeCall(Boolean(payment.livemode), `checkout/sessions/${payment.checkout_id}`, { account: payment.account_id });
                const pi = await ports.stripeCall(Boolean(payment.livemode), `payment_intents/${String(session.payment_intent)}`, { account: payment.account_id });
                if (pi.status !== 'succeeded' || pi.amount_received !== Number(payment.amount_minor) || (pi.metadata as Record<string, string> | undefined)?.pms_booking_payment_id !== payment.id)
                    throw ports.error(409, 'PMS_REFUND_PAYMENT_MISMATCH', 'Original provider payment needs review');
                const guest = record(await db.bookingSnapshot(payment.property_id), 'reservation', payment.reservation_id).data.guestId;
                const email = record(await db.bookingSnapshot(payment.property_id), 'guest', guest).data.email;
                const charge = typeof pi.latest_charge === 'string' ? await ports.stripeCall(Boolean(payment.livemode), `charges/${pi.latest_charge}`, { account: payment.account_id }) : null;
                const promptpay = (charge?.payment_method_details as {
                    type?: string;
                } | undefined)?.type === 'promptpay' || parseGuestAccountJson(payment.methods_json).join(',') === 'promptpay';
                const refund = await ports.stripeCall(Boolean(payment.livemode), 'refunds', { method: 'POST', account: payment.account_id, key: `pms-refund-${request.id}-${request.generation}`, body: { payment_intent: pi.id, amount: String(request.amount_minor), ...(promptpay ? { instructions_email: email } : {}), 'metadata[pms_booking_payment_id]': payment.id, 'metadata[pms_booking_refund_id]': request.id } });
                refundId = String(refund.id);
                await db.insertPmsBookingRefundAttempts({ id: request.id, generation: request.generation, refundId: refundId, now: now });
                await db.updatePmsBookingRefundRequests2({ refundId: refundId, now: now, id: request.id });
                await db.updatePmsBookingPayments4({ refundId: refundId, now: now, id: payment.id });
            }
            const refund = await ports.stripeCall(Boolean(payment.livemode), `refunds/${refundId}`, { account: payment.account_id });
            if (refund.amount !== Number(request.amount_minor) || refund.currency !== 'thb' && refund.currency !== undefined || (refund.metadata as Record<string, string> | undefined)?.pms_booking_payment_id !== payment.id || ((refund.metadata as Record<string, string> | undefined)?.pms_booking_refund_id && ((refund.metadata as Record<string, string>).pms_booking_refund_id !== request.id)))
                throw ports.error(409, 'PMS_REFUND_PROVIDER_MISMATCH', 'Refund amount or payment reference does not match');
            if (refund.status === 'failed' || refund.status === 'canceled') {
                await db.updatePmsBookingRefundRequests3({ now: now, id: request.id });
                await db.updatePmsBookingPayments5({ now: now, id: payment.id });
                continue;
            }
            if (refund.status !== 'succeeded') {
                const state = refund.status === 'requires_action' ? 'requires_action' : 'processing';
                await db.updatePmsBookingRefundRequests4({ state: state, now: now, id: request.id });
                continue;
            }
            await db.transaction(async (tx) => {
                const s = await tx.bookingSnapshot(payment.property_id, undefined, true);
                await tx.readWriteContext({});
                const current = await tx.readPmsBookingRefundRequests6({ id: request.id });
                if (current?.state === 'succeeded')
                    return;
                if (!current || !['processing', 'requires_action'].includes(current.state))
                    throw ports.error(409, 'PMS_REFUND_STATE', 'Refund state changed');
                const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)])), oldEdc = s.property.config.accounts.edc;
                s.property.config.accounts.edc = payment.clearing_account;
                const parts = parseGuestAccountJson(current.allocations_json) as Part[];
                if (!parts.length) {
                    const st = records(s, 'stay').find(st => st.data.reservationId === payment.reservation_id)!;
                    const receipt = records(s, 'entry').find(e => e.data.sourceKey === `online-payment:${payment.id}` && e.data.description === 'payment.unallocated');
                    if (!receipt)
                        throw ports.error(409, 'PMS_UNALLOCATED_RECEIPT_MISSING', 'Unallocated receipt requires recovery');
                    parts.push({ stayId: st.id, guestAccountId: st.data.guestAccountId, entryId: receipt.id, amount: Number(current.amount_minor) });
                }
                for (const part of parts) {
                    const outcome = executePms(s, 'payment.refund_settle', { guestAccountId: part.guestAccountId, amount: part.amount, reference: refundId, sourceEntryId: part.entryId }, providerActor(now));
                    await tx.persist(s, before);
                    await tx.effects({ companyId: payment.company_id, userId: 'system:stripe' } as WorkContext, s, outcome.effects);
                }
                s.property.config.accounts.edc = oldEdc;
                await tx.persist(s, before);
                await tx.updatePmsBookingRefundRequests5({ now: now, id: current.id });
                await tx.updatePmsBookingPayments6({ now: now, id: payment.id });
                await tx.queueMail(s, payment.reservation_id, payment.holding_slug, 'refund', await ports.bookingEnv(), now);
            });
        }
    }
    /** Retry only after Stripe confirms that the previous refund failed or was cancelled. */
    async function retryBookingRefund(db: PmsBookingRefundRepository, actor: WorkContext, id: string, reason: string) {
        ports.require(actor, 'pms.approve');
        if (actor.departmentId || reason.trim().length < 3) throw ports.error(422,'PMS_REFUND_RETRY_INPUT','Use PMS and enter a recovery reason');
        const found = await db.readPmsBookingRefundRequests7({ id: id, companyId: actor.companyId });
        if (!found || found.state !== 'failed' || !found.provider_id || samePerson(actor, found.requested_by))
            throw ports.error(409, 'PMS_REFUND_RETRY_UNAVAILABLE', 'An independent PMS reviewer must select a failed provider refund');
        const provider = await ports.stripeCall(Boolean(found.livemode), `refunds/${found.provider_id}`, { account: found.account_id });
        if (!['failed', 'canceled'].includes(String(provider.status)))
            throw ports.error(409, 'PMS_REFUND_PROVIDER_PENDING', 'Recover the current provider refund without creating another');
        return db.transaction(async (tx) => {
            const s=await tx.bookingSnapshot(found.property_id, undefined, true);
            if(actor.branchId && s.property.branchId !== actor.branchId)throw ports.error(404,'PMS_NOT_FOUND','Refund not found in this workspace.');
            await tx.authorize(actor,'pms.approve');
            const current = await tx.readPmsBookingRefundRequests8({ id: id });
            if (current?.state !== 'failed' || current.provider_id !== found.provider_id)
                throw ports.error(409, 'PMS_REFUND_CHANGED', 'Refund changed; refresh before retrying');
            await tx.insertPmsBookingRefundAttempts2({ id: id, generation: current.generation, provider_id: current.provider_id, fact3: ports.clock.now() });
            await tx.updatePmsBookingRefundRequests6({ fact0: `Recovery approved by ${actor.userId}: ${reason.trim()}`.slice(0, 500), fact1: ports.clock.now(), id: id });
            await tx.updatePmsBookingPayments7({ fact0: ports.clock.now(), payment_id: current.payment_id });
            return { id, state: 'approved' };
        });
    }
    return { requestBookingRefund, approveBookingRefund, bookingRefundsForFinance, recoverBookingReceipt, processBookingRefund, retryBookingRefund };
}
