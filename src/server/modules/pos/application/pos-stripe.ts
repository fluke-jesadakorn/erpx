import { z } from "zod";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { Clock } from "../../shared/application/ports";
import { businessDate as evaluateBusinessDate, type PosAccounts } from "../domain/pos";
import { hasPermission } from "@/lib/identity_access/permissions";
import { num, str, json, pair, type PosRow } from "../domain/pos-model";
import { PosStripeApiError, type StripeOptions } from "../domain/stripe";
import type { PosProviderPurchase } from "../domain/package-commands";
export type PosRows<Row> = {
    results: Row[];
};
export type PosStripeUnit = {
    writePosStripeEvents1(facts: {
        id: string;
        account: string;
        livemode: boolean;
        company_id: string | number | boolean | null;
        type: string;
        fact5: string;
        fact6: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosStripeEvents2(facts: {
        fact0: number;
        id: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosConnectionHistory1(facts: {
        companyId: string;
        accountId: string;
        live: boolean;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosConnections1(facts: {
        companyId: string;
        accountId: string;
        live: boolean;
        fact3: boolean;
        fact4: boolean;
        fact5: boolean;
        fact6: boolean;
        fact7: string;
        fact8: "ready" | "onboarding";
        fact9: string | null;
        thbBankReady: boolean;
        fact11: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosOauthStates1(facts: {
        stateId: string;
        companyId: string;
        userId: string;
        fact3: number;
    }): Promise<PosRow | null>;
    writePosOauthStates1(facts: {
        fact0: number;
        stateId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosPayments1(facts: {
        fact0: number;
        id: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments1(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosPayments2(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosPayments3(facts: {
        fact0: number;
        id: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments3(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments4(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosPayments4(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosRefundPayments1(facts: {
        id: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosStripeEvents1(facts: {
        record_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosConnections1(facts: {
        company_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosConnections2(facts: {
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPmsStripePayments1(facts: {
        counterId: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePmsStripePayments1(facts: {
        provider_created_at: string | number | true;
        counterId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPmsBookingPayments1(facts: {
        bookingId: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePmsBookingPayments1(facts: {
        provider_created_at: string | number | true;
        bookingId: string;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments5(facts: {
        pid: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPmsStripePayments2(facts: {
        counterId: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosStripeEvents3(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPmsBookingPayments2(facts: {
        bookingId: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosStripeEvents4(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosRefundPayments1(facts: {
        rpid: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments6(facts: {
        payment_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosStripeEvents5(facts: {
        fact0: number;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSettlements1(facts: {
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
        btId: string;
    }): Promise<PosRow | null>;
    readPosPayments7(facts: {
        paymentId: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPmsBookingPayments3(facts: {
        bookingId: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPmsStripePayments3(facts: {
        counterId: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosRefundPayments2(facts: {
        source: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPmsBookingRefundRequests1(facts: {
        source: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPmsStripePayments4(facts: {
        source: string;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosSettlements1(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
        btId: string;
        amount: number;
        fee: number;
        fact7: string;
        fact8: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosSettlements2(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
        btId: string;
        amount: number;
        fee: number;
        fact7: string;
        journal: string | null;
        fact9: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSettings1(facts: {
        companyId: string;
    }): Promise<PosRow | null>;
    readPmsProperties1(facts: {
        companyId: string;
    }): Promise<PosRow | null>;
    readPmsProperties2(facts: {
        companyId: string;
    }): Promise<PosRow | null>;
    writePosSettlements3(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
        id: string;
        amount: number;
        fact6: string;
        fact7: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSettlements2(facts: {
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
        fact2: string;
    }): Promise<PosRow | null>;
    readPosPostings1(facts: {
        fact0: string;
    }): Promise<PosRow | null>;
    readPosSettlements3(facts: {
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
        id: string;
    }): Promise<PosRow | null>;
    readPosSettlements4(facts: {
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
        id: string;
    }): Promise<PosRow | null>;
    writePosSettlements4(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        company_id: string | number | boolean | null;
        account_id: string | number | boolean | null;
        livemode: string | number | boolean | null;
        fact4: "payout" | "dispute";
        id: string;
        amount: number;
        fact7: string;
        fact8: string;
        journal: string | null;
        fact10: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    readAssignments1(facts: {
        assignmentId: string | undefined;
        userId: string | undefined;
        company_id: string | number | boolean | null;
        fact3: number;
        fact4: number;
    }): Promise<PosRow | null>;
    readPosConnections2(facts: {
        company_id: string | number | boolean | null;
        accountId: string | undefined;
        fact2: boolean;
    }): Promise<PosRow | null>;
    readAssignments2(facts: {
        assignmentId: string | undefined;
        userId: string | undefined;
        company_id: string | number | boolean | null;
        fact3: number;
        fact4: number;
    }): Promise<PosRow | null>;
    readPosConnections3(facts: {
        company_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosConnections4(facts: {
        company_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosJobs1(facts: {
        fact0: string;
        id: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments8(facts: {
        record_id: string | number | boolean | null;
        company_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosSales1(facts: {
        sale_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosPayments5(facts: {
        id: string;
        qr: string;
        fact2: number;
        fact3: number;
        id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosSales2(facts: {
        sale_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosPayments6(facts: {
        id: string;
        url: unknown;
        fact2: number;
        id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosPayments9(facts: {
        record_id: string | number | boolean | null;
        company_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosSales3(facts: {
        record_id: string | number | boolean | null;
        company_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments10(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosRefundPayments3(facts: {
        record_id: string | number | boolean | null;
        company_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosPayments11(facts: {
        payment_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    writePosRefundPayments2(facts: {
        id: string;
        id2: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    readPosJobs1(facts: {
        token: `${string}-${string}-${string}-${string}-${string}`;
        fact1: number;
        now: number;
        now2: number;
        companyId: string | null;
        companyId2: string | null;
    }): Promise<PosRow | null>;
    writePosJobs2(facts: {
        fact0: number;
        id: string | number | boolean | null;
        token: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosJobs3(facts: {
        fact0: "completed" | "pending";
        fact1: number;
        id: string | number | boolean | null;
        token: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PosRows<Record<string, unknown>>>;
    writePosJobs4(facts: {
        fact0: "pending" | "exception";
        fact1: number;
        code: string;
        id: string | number | boolean | null;
        token: `${string}-${string}-${string}-${string}-${string}`;
    }): Promise<PosRows<Record<string, unknown>>>;
    readContext1(facts: Record<string, never>): Promise<PosRow | null>;
    enqueue(arg0: string, arg1: "event" | "complete_sale" | "check_payment", arg2: string): Promise<void>;
    audit(arg0: string, arg1: null, arg2: string | "system:stripe", arg3: "connection.connected" | "job.exception", arg4: string, arg5: {
        livemode: boolean;
    } | {
        jobId: string | number | boolean | null;
        code: string;
    }): Promise<void>;
    receivePayment(arg0: PosRow, arg1: "system:stripe", arg2: string | undefined): Promise<void>;
    releaseUnpaidReservations(arg0: string): Promise<void>;
    completeRefundPayment(arg0: string, arg1: "system:stripe", arg2: string | undefined): Promise<void>;
    posting(arg0: string, arg1: null, arg2: PosAccounts, arg3: string, arg4: string | "THB", arg5: string, arg6: string, arg7: "system:stripe", arg8: {
        account: string;
        debit: number;
        credit: number;
    }[]): Promise<string | null>;
    loadIamAuthority(arg0: string, arg1: string, arg2: string): Promise<{
        queuedAssignmentVersion?: number;
        roleRevision: number;
        identityVersion: number;
        permissions: string[];
        grantablePermissions: string[];
        principalId: string;
        personId: string | null;
        personUserIds?: string[];
        hrAppointmentVersion?: number;
        emergencyUntil?: number | null;
        freshUntil: number | null;
    }>;
    completeSale(arg0: PosRow, arg1: "system:stripe", arg2: string | undefined): Promise<{
        id: string | number | boolean | null;
        state: string;
        invoiceId: string | number | boolean | null;
    }>;
    controlOne1(facts: {
        arg0: string;
        arg1: boolean;
    }): Promise<PosRow | null>;
    controlOne2(facts: {
        arg0: string;
        arg1: boolean;
    }): Promise<PosRow | null>;
    controlRun1(facts: {
        arg0: string;
        arg1: boolean;
        arg2: string;
        arg3: string;
        arg4: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    controlOne3(facts: {
        arg0: string;
        arg1: boolean;
    }): Promise<PosRow | null>;
    controlOne4(facts: {
        arg0: string;
    }): Promise<PosRow | null>;
    reconcileCounter(paymentId: string): Promise<unknown>;
    reconcileBooking(paymentId: string): Promise<unknown>;
    refundCounter(paymentId: string): Promise<unknown>;
    refundBooking(paymentId: string): Promise<unknown>;
};
export type PosStripeRepository = PosStripeUnit & {
    transaction<Result>(work: (unit: PosStripeRepository) => Promise<Result>, purchase?: PosProviderPurchase): Promise<Result>;
    requirePurchase(input: PosProviderPurchase): Promise<void>;
};
export function createPosStripe(ports: {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    environment(key: string): string | undefined;
    error(status: number, code: string, message: string): Error;
    resources(): Promise<{
        control: PosStripeRepository;
        holdings: Array<{
            id: string;
        }>;
    }>;
    forHolding(holdingId: string): Promise<PosStripeRepository>;
    provider: {
        request(live: boolean, path: string, options?: StripeOptions): Promise<unknown>;
        secret(live: boolean): string;
        origin(): string;
        settingsReturn?(holdingSlug: string, companyId: string): string;
        fetch(url: string, init: RequestInit & {
            timeoutMs: number;
        }): Promise<{
            ok: boolean;
            json(): Promise<unknown>;
        }>;
    };
    signing: {
        equal(secret: string, payload: string, signature: string): boolean;
    };
    leases: {
        repeat(callback: () => void, ms: number): unknown;
        cancel(token: unknown): void;
    };
    report(value: string): void;
}) {
    function fail(code: string, message: string, status = 409): never { throw ports.error(status, code, message); }
    const businessDate = (timezone = "Asia/Bangkok", date = new Date(ports.clock.now())) => evaluateBusinessDate(timezone, date);
    function verifySignature(raw: string, signature: string, signingSecrets: string[], now = ports.clock.now()) {
        const parts = signature.split(','), timestamp = parts.find(p => p.startsWith('t='))?.slice(2), values = parts.filter(p => p.startsWith('v1=')).map(p => p.slice(3));
        if (!timestamp || !/^[0-9]+$/.test(timestamp) || Math.abs(now / 1000 - Number(timestamp)) > 300)
            return false;
        return signingSecrets.some(secret => values.some(value => /^[a-f0-9]{64}$/.test(value) && ports.signing.equal(secret, `${timestamp}.${raw}`, value)));
    }
    const objectSchema = z.object({ id: z.string() }).passthrough();
    type StripeObject = z.infer<typeof objectSchema>;
    const hasCompanyStripePermission = (actor: WorkContext) => hasPermission(actor, 'pos.payments.configure') || hasPermission(actor, 'finance.approve');
    async function stripeCall(live: boolean, path: string, options: StripeOptions = {}): Promise<StripeObject> { return objectSchema.parse(await ports.provider.request(live, path, options)); }
    async function stripeListCall(live: boolean, path: string, options: StripeOptions = {}) { return z.object({ object: z.literal('list'), data: z.array(z.record(z.string(), z.unknown())), has_more: z.boolean() }).parse(await ports.provider.request(live, path, options)); }
    async function promptPayIntent(p: PosRow, saleNumber: string) {
        const base = { amount: String(p.amount_minor), currency: 'thb', 'payment_method_types[0]': 'promptpay', 'payment_method_data[type]': 'promptpay', confirm: 'true', description: saleNumber, 'metadata[pos_payment_id]': str(p, 'id') };
        const body = { ...base, 'payment_method_data[billing_details][email]': String(p.payer_email), receipt_email: String(p.payer_email) };
        const options = { method: 'POST' as const, account: str(p, 'stripe_account'), key: `pos-promptpay-${p.id}` };
        try {
            return await stripeCall(Boolean(p.livemode), 'payment_intents', { ...options, body });
        }
        catch (error) {
            if (!(error instanceof PosStripeApiError) || error.providerType !== 'idempotency_error')
                throw error;
            // The legacy adapter omitted mandatory billing email. Recover only when the
            // original key definitively returns that validation failure, never after an
            // unknown provider result. Both legacy receipt-email shapes use the same key.
            let rejectedBeforeCreation = false;
            for (const legacyBody of [{ ...base, receipt_email: String(p.payer_email) }, base]) {
                try {
                    await stripeCall(Boolean(p.livemode), 'payment_intents', { ...options, body: legacyBody });
                    throw new Error('POS_PROVIDER_LOOKUP_REQUIRED');
                }
                catch (legacyError) {
                    if (legacyError instanceof PosStripeApiError && legacyError.status === 400 && legacyError.providerCode === 'parameter_missing' && legacyError.parameter === 'billing_details[email]') {
                        rejectedBeforeCreation = true;
                        break;
                    }
                    if (!(legacyError instanceof PosStripeApiError) || legacyError.providerType !== 'idempotency_error')
                        throw new Error('POS_PROVIDER_LOOKUP_REQUIRED');
                }
            }
            if (!rejectedBeforeCreation)
                throw new Error('POS_PROVIDER_LOOKUP_REQUIRED');
            return stripeCall(Boolean(p.livemode), 'payment_intents', { ...options, key: `${options.key}-billing-email-v2`, body });
        }
    }
    async function posStripeWebhook(raw: string, signature: string) {
        const secrets = [ports.environment("POS_STRIPE_TEST_WEBHOOK_SECRET"), ports.environment("POS_STRIPE_LIVE_WEBHOOK_SECRET")].filter((s): s is string => Boolean(s));
        if (!verifySignature(raw, signature, secrets))
            fail('POS_WEBHOOK_SIGNATURE', 'Invalid signature', 400);
        const event = z.object({ id: z.string(), type: z.string(), account: z.string(), livemode: z.boolean(), created: z.number().int().positive().optional(), data: z.object({ object: z.record(z.string(), z.unknown()) }) }).parse(JSON.parse(raw));
        const modeSecret = ports.environment(event.livemode ? 'POS_STRIPE_LIVE_WEBHOOK_SECRET' : 'POS_STRIPE_TEST_WEBHOOK_SECRET');
        if (!modeSecret || !verifySignature(raw, signature, [modeSecret]))
            fail('POS_WEBHOOK_MODE', 'Webhook signing mode mismatch', 400);
        const env = await ports.resources(), route = await env.control.controlOne1({ arg0: event.account, arg1: event.livemode });
        if (!route)
            return { received: true, ignored: true };
        const db = await ports.forHolding(str(route, 'holding_id'));
        await db.transaction(async (tx) => {
            await tx.writePosStripeEvents1({ id: event.id, account: event.account, livemode: event.livemode, company_id: route.company_id, type: event.type, fact5: JSON.stringify(event.data.object), fact6: ports.clock.now() });
            if (event.created)
                await tx.writePosStripeEvents2({ fact0: event.created * 1000, id: event.id });
            await tx.enqueue(str(route, 'company_id'), 'event', event.id);
        });
        return { received: true };
    }
    function oauthUrl(state: string, holdingSlug: string, live: boolean) {
        if (ports.environment("NODE_ENV") === 'production' && ports.environment("POS_STRIPE_LIVE_ENABLED") !== 'true')
            fail('POS_PROVIDER_COMING_SOON', 'Enable the Stripe production gate before connecting live accounts', 403);
        ports.provider.secret(live);
        const client = ports.environment(live ? 'POS_STRIPE_LIVE_CLIENT_ID' : 'POS_STRIPE_TEST_CLIENT_ID');
        if (!client)
            fail('POS_STRIPE_OAUTH_NOT_CONFIGURED', 'Platform has not configured Connect OAuth');
        const params = new URLSearchParams({ response_type: 'code', client_id: client, scope: 'read_write', state, redirect_uri: `${ports.provider.origin()}/api/v1/holdings/${holdingSlug}/pos/connect/callback`, 'stripe_user[country]': 'TH', 'stripe_user[currency]': 'thb' });
        return `https://connect.stripe.com/oauth/authorize?${params}`;
    }
    async function bindAccount(db: PosStripeRepository, holdingId: string, companyId: string, accountId: string, live: boolean) {
        const account = await stripeCall(live, `accounts/${accountId}`), env = await ports.resources();
        if (account.country !== 'TH')
            fail('POS_STRIPE_COUNTRY', 'This version supports Stripe accounts in Thailand');
        const banks = z.object({ data: z.array(z.record(z.string(), z.unknown())) }).parse(await ports.provider.request(live, `accounts/${accountId}/external_accounts?object=bank_account&limit=20`));
        const thbBankReady = Array.isArray(banks.data) && banks.data.some((bank: unknown) => { const b = bank as Record<string, unknown>; return b.currency === 'thb' && b.status !== 'errored' && b.status !== 'verification_failed'; });
        const payoutInterval = (((account.settings as Record<string, unknown> | undefined)?.payouts as Record<string, unknown> | undefined)?.schedule as Record<string, unknown> | undefined)?.interval;
        const existing = await env.control.controlOne2({ arg0: accountId, arg1: live });
        if (existing && (existing.holding_id !== holdingId || existing.company_id !== companyId))
            fail('POS_STRIPE_ACCOUNT_BOUND', 'This Stripe account is already connected to another company');
        await env.control.controlRun1({ arg0: accountId, arg1: live, arg2: holdingId, arg3: companyId, arg4: ports.clock.now() });
        const route = await env.control.controlOne2({ arg0: accountId, arg1: live });
        if (route?.holding_id !== holdingId || route.company_id !== companyId)
            fail('POS_STRIPE_ACCOUNT_BOUND', 'This Stripe account is already connected to another company');
        await db.transaction(async (tx) => {
            await tx.writePosConnectionHistory1({ companyId: companyId, accountId: accountId, live: live });
            const capabilities = (account.capabilities ?? {}) as Record<string, unknown>, requirements = (account.requirements ?? {}) as Record<string, unknown>;
            await tx.writePosConnections1({ companyId: companyId, accountId: accountId, live: live, fact3: account.charges_enabled === true, fact4: account.payouts_enabled === true, fact5: capabilities.card_payments === 'active', fact6: capabilities.promptpay_payments === 'active', fact7: JSON.stringify({ currently_due: requirements.currently_due ?? [], pending_verification: requirements.pending_verification ?? [] }), fact8: account.charges_enabled && account.payouts_enabled && thbBankReady && payoutInterval === 'daily' ? 'ready' : 'onboarding', fact9: typeof payoutInterval === 'string' ? payoutInterval : null, thbBankReady: thbBankReady, fact11: ports.clock.now() });
        });
    }
    async function finishPosOAuth(actor: WorkContext, stateId: string, code: string) {
        if (ports.environment("NODE_ENV") === 'production' && ports.environment("POS_STRIPE_LIVE_ENABLED") !== 'true')
            fail('POS_PROVIDER_COMING_SOON', 'Enable the Stripe production gate before connecting live accounts', 403);
        if (!hasCompanyStripePermission(actor))
            fail('POS_CONNECTION_FORBIDDEN', 'Company Finance access is required', 403);
        if (actor.branchId || actor.departmentId)
            fail('POS_COMPANY_SETTINGS', 'Select a company area', 403);
        const db = await ports.forHolding(actor.holdingId);
        // Claim once before the non-idempotent OAuth exchange. A failed exchange requires fresh authorization.
        const live = await db.transaction(async (tx) => {
            const state = await tx.readPosOauthStates1({ stateId: stateId, companyId: actor.companyId, userId: actor.userId, fact3: ports.clock.now() });
            if (!state)
                fail('POS_OAUTH_STATE', 'The confirmation link expired. Connect again.', 400);
            await tx.writePosOauthStates1({ fact0: ports.clock.now(), stateId: stateId });
            return Boolean(state.livemode);
        });
        const response = await ports.provider.fetch('https://connect.stripe.com/oauth/token', { method: 'POST', headers: { Authorization: `Bearer ${ports.provider.secret(live)}`, 'Content-Type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({ grant_type: 'authorization_code', code }), timeoutMs: 20000 });
        if (!response.ok)
            fail('POS_OAUTH_FAILED', 'Connection failed. Start again.');
        const value = z.object({ stripe_user_id: z.string().regex(/^acct_/), livemode: z.boolean() }).parse(await response.json());
        if (value.livemode !== live)
            fail('POS_STRIPE_MODE', 'Account mode does not match');
        await bindAccount(db, actor.holdingId, actor.companyId, value.stripe_user_id, live);
        await db.audit(actor.companyId, null, actor.userId, 'connection.connected', value.stripe_user_id, { livemode: live });
    }
    async function checkPayment(db: PosStripeRepository, p: PosRow, date?: string) {
        const purchase: PosProviderPurchase = { companyId: str(p, 'company_id'), operation: 'payment.settle', recordId: str(p, 'id') };
        await db.requirePurchase(purchase);
        if (!p.provider_id)
            throw new Error('POS_CHECKOUT_NOT_CREATED');
        if (p.method === 'stripe_promptpay' && String(p.provider_id).startsWith('pi_')) {
            const pi = await stripeCall(Boolean(p.livemode), `payment_intents/${p.provider_id}`, { account: str(p, 'stripe_account') });
            if (pi.status === 'succeeded') {
                if (pi.amount_received !== num(p, 'amount_minor') || pi.currency !== 'thb' || (pi.metadata as Record<string, string>)?.pos_payment_id !== p.id)
                    throw new Error('POS_PROVIDER_PAYMENT_MISMATCH');
                await db.transaction(async (tx) => { const current = (await tx.readPosPayments1({ id: p.id }))!; await tx.writePosPayments1({ fact0: ports.clock.now(), id: str(p, 'id') }); await tx.receivePayment(current, 'system:stripe', date); await tx.enqueue(str(p, 'company_id'), 'complete_sale', str(p, 'sale_id')); }, purchase);
                return false;
            }
            if (pi.status === 'canceled') {
                await db.transaction(async (tx) => {
                    const current = (await tx.readPosPayments1({ id: p.id }))!;
                    if (current.state === 'succeeded')
                        return;
                    await tx.writePosPayments2({ fact0: ports.clock.now(), id: p.id });
                    await tx.releaseUnpaidReservations(str(p, 'sale_id'));
                }, purchase);
                return false;
            }
            if ((p.expires_at && ports.clock.now() >= num(p, 'expires_at')) || pi.status === 'requires_payment_method') {
                await stripeCall(Boolean(p.livemode), `payment_intents/${p.provider_id}/cancel`, { method: 'POST', account: str(p, 'stripe_account'), key: `pos-cancel-${p.id}`, body: {} });
                return checkPayment(db, p, date);
            }
            return true;
        }
        const session = await stripeCall(Boolean(p.livemode), `checkout/sessions/${p.provider_id}`, { account: str(p, 'stripe_account') });
        if (session.payment_status === 'paid') {
            const pi = await stripeCall(Boolean(p.livemode), `payment_intents/${String(session.payment_intent)}`, { account: str(p, 'stripe_account') });
            if (pi.status !== 'succeeded' || pi.amount_received !== num(p, 'amount_minor') || pi.currency !== 'thb' || (pi.metadata as Record<string, string>)?.pos_payment_id !== p.id)
                throw new Error('POS_PROVIDER_PAYMENT_MISMATCH');
            await db.transaction(async (tx) => { const current = (await tx.readPosPayments1({ id: p.id }))!; await tx.writePosPayments1({ fact0: ports.clock.now(), id: str(p, 'id') }); await tx.receivePayment(current, 'system:stripe', date); await tx.enqueue(str(p, 'company_id'), 'complete_sale', str(p, 'sale_id')); }, purchase);
            return false;
        }
        if (session.status === 'expired') {
            await db.transaction(async (tx) => {
                const current = (await tx.readPosPayments1({ id: p.id }))!;
                if (current.state === 'succeeded')
                    return;
                await tx.writePosPayments2({ fact0: ports.clock.now(), id: p.id });
                await tx.releaseUnpaidReservations(str(p, 'sale_id'));
            }, purchase);
            return false;
        }
        return true;
    }
    async function checkRefund(db: PosStripeRepository, rp: PosRow, p: PosRow, date?: string) {
        const purchase: PosProviderPurchase = { companyId: str(p, 'company_id'), operation: 'refund.settle', recordId: str(rp, 'refund_id') };
        await db.requirePurchase(purchase);
        const refund = await stripeCall(Boolean(p.livemode), `refunds/${rp.provider_id}`, { account: str(p, 'stripe_account') });
        if (refund.amount !== num(rp, 'amount_minor') || (refund.metadata as Record<string, string>)?.pos_refund_payment_id !== rp.id)
            throw new Error('POS_PROVIDER_REFUND_MISMATCH');
        if (refund.status === 'succeeded') {
            await db.transaction(tx => tx.completeRefundPayment(str(rp, 'id'), 'system:stripe', date), purchase);
            return false;
        }
        if (refund.status === 'failed' || refund.status === 'canceled') {
            await db.writePosRefundPayments1({ id: rp.id });
            throw new Error('POS_PROVIDER_REFUND_FAILED');
        }
        return true;
    }
    async function stripeEvent(db: PosStripeRepository, job: PosRow, holdingId: string) {
        const event = (await db.readPosStripeEvents1({ record_id: job.record_id }))!;
        if (event.applied_at)
            return false;
        const obj = json<Record<string, unknown>>(event.payload_json), type = str(event, 'event_type');
        if (type === 'account.updated') {
            const current = await db.readPosConnections1({ company_id: job.company_id });
            if (current?.account_id === event.account_id && current.livemode === event.livemode)
                await bindAccount(db, holdingId, str(job, 'company_id'), str(event, 'account_id'), Boolean(event.livemode));
        }
        else if (type === 'account.application.deauthorized')
            await db.writePosConnections2({ company_id: job.company_id, account_id: event.account_id, livemode: event.livemode });
        else if (type.startsWith('account.external_account.'))
            await bindAccount(db, holdingId, str(job, 'company_id'), str(event, 'account_id'), Boolean(event.livemode));
        else if (type.startsWith('checkout.session.') || type.startsWith('payment_intent.')) {
            const counterId = (obj.metadata as Record<string, string> | undefined)?.pms_counter_payment_id;
            if (counterId) {
                const payment = await db.readPmsStripePayments1({ counterId: counterId, company_id: job.company_id, account_id: event.account_id, livemode: event.livemode });
                if (!payment)
                    throw new Error('PMS_PROVIDER_PAYMENT_MISMATCH');
                if (type === 'payment_intent.succeeded' && event.provider_created_at)
                    await db.writePmsStripePayments1({ provider_created_at: event.provider_created_at, counterId: counterId });
                await db.reconcileCounter(counterId);
            }
            const bookingId = (obj.metadata as Record<string, string> | undefined)?.pms_booking_payment_id;
            if (bookingId) {
                const payment = await db.readPmsBookingPayments1({ bookingId: bookingId, company_id: job.company_id, account_id: event.account_id, livemode: event.livemode });
                if (!payment)
                    throw new Error('PMS_PROVIDER_BOOKING_MISMATCH');
                if (type === 'payment_intent.succeeded' && event.provider_created_at)
                    await db.writePmsBookingPayments1({ provider_created_at: event.provider_created_at, bookingId: bookingId });
                await db.reconcileBooking(bookingId);
            }
            const pid = (obj.metadata as Record<string, string> | undefined)?.pos_payment_id;
            const p = pid ? await db.readPosPayments5({ pid: pid, company_id: job.company_id, account_id: event.account_id, livemode: event.livemode }) : null;
            if (p?.provider_id)
                await checkPayment(db, p, json<{
                    accountingDate?: string;
                }>(job.payload_json).accountingDate);
        }
        else if (type.startsWith('refund.')) {
            const counterId = (obj.metadata as Record<string, string> | undefined)?.pms_counter_payment_id;
            if (counterId) {
                const payment = await db.readPmsStripePayments1({ counterId: counterId, company_id: job.company_id, account_id: event.account_id, livemode: event.livemode });
                if (!payment)
                    throw new Error('PMS_PROVIDER_REFUND_MISMATCH');
                await db.refundCounter(counterId);
                await db.writePosStripeEvents3({ fact0: ports.clock.now(), id: event.id });
                return false;
            }
            const bookingId = (obj.metadata as Record<string, string> | undefined)?.pms_booking_payment_id;
            if (bookingId) {
                const payment = await db.readPmsBookingPayments1({ bookingId: bookingId, company_id: job.company_id, account_id: event.account_id, livemode: event.livemode });
                if (!payment)
                    throw new Error('PMS_PROVIDER_REFUND_MISMATCH');
                await db.refundBooking(bookingId);
                await db.writePosStripeEvents3({ fact0: ports.clock.now(), id: event.id });
                return false;
            }
            const rpid = (obj.metadata as Record<string, string> | undefined)?.pos_refund_payment_id;
            const rp = rpid ? await db.readPosRefundPayments1({ rpid: rpid, company_id: job.company_id, account_id: event.account_id, livemode: event.livemode }) : null;
            if (!rp)
                throw new Error('POS_EXTERNAL_REFUND_REQUIRES_REVIEW');
            if (rp.provider_id)
                await checkRefund(db, rp, (await db.readPosPayments6({ payment_id: rp.payment_id }))!, json<{
                    accountingDate?: string;
                }>(job.payload_json).accountingDate);
        }
        else if (type.startsWith('payout.') || type.startsWith('charge.dispute.')) {
            await settlement(db, job, event, obj);
        }
        else if (type === 'charge.succeeded') {
            const charge = await stripeCall(Boolean(event.livemode), `charges/${String(obj.id)}`, { account: str(event, 'account_id') });
            if (!charge.balance_transaction)
                return true;
            await balanceTransaction(db, event, String(charge.balance_transaction), json<{
                accountingDate?: string;
            }>(job.payload_json).accountingDate);
        }
        await db.writePosStripeEvents3({ fact0: ports.clock.now(), id: event.id });
        return false;
    }
    async function balanceTransaction(db: PosStripeRepository, event: PosRow, btId: string, date?: string) {
        const existing = await db.readPosSettlements1({ company_id: event.company_id, account_id: event.account_id, livemode: event.livemode, btId: btId });
        if (existing && existing.state !== 'unmapped')
            return;
        const bt = await stripeCall(Boolean(event.livemode), `balance_transactions/${btId}`, { account: str(event, 'account_id') });
        if (bt.currency !== 'thb')
            throw new Error('POS_SETTLEMENT_CURRENCY');
        const amount = z.number().int().parse(bt.amount), fee = z.number().int().parse(bt.fee);
        const source = typeof bt.source === 'string' ? bt.source : '';
        let owned = bt.type === 'stripe_fee';
        if (source.startsWith('ch_') || source.startsWith('py_')) {
            const charge = await stripeCall(Boolean(event.livemode), `charges/${source}`, { account: str(event, 'account_id') });
            const paymentId = (charge.metadata as Record<string, string> | undefined)?.pos_payment_id;
            const bookingId = (charge.metadata as Record<string, string> | undefined)?.pms_booking_payment_id;
            const counterId = (charge.metadata as Record<string, string> | undefined)?.pms_counter_payment_id;
            owned = Boolean(paymentId && await db.readPosPayments7({ paymentId: paymentId, company_id: event.company_id, account_id: event.account_id, livemode: event.livemode })) || Boolean(bookingId && await db.readPmsBookingPayments3({ bookingId: bookingId, company_id: event.company_id, account_id: event.account_id, livemode: event.livemode })) || Boolean(counterId && await db.readPmsStripePayments3({ counterId: counterId, company_id: event.company_id, account_id: event.account_id, livemode: event.livemode }));
        }
        else if (source.startsWith('re_') || source.startsWith('pyr_'))
            owned = Boolean(await db.readPosRefundPayments2({ source: source, company_id: event.company_id, account_id: event.account_id, livemode: event.livemode })) || Boolean(await db.readPmsBookingRefundRequests1({ source: source, company_id: event.company_id, account_id: event.account_id, livemode: event.livemode })) || Boolean(await db.readPmsStripePayments4({ source: source, company_id: event.company_id, account_id: event.account_id, livemode: event.livemode }));
        else if (source.startsWith('dp_'))
            owned = true;
        if (!owned) {
            await db.writePosSettlements1({ fact0: ports.uuid(), company_id: event.company_id, account_id: event.account_id, livemode: event.livemode, btId: btId, amount: amount, fee: fee, fact7: JSON.stringify({ source, net: bt.net, type: bt.type }), fact8: ports.clock.now() });
            return;
        }
        await db.transaction(async (tx) => {
            const settings = await settlementSettings(tx, str(event, 'company_id')), a = json<PosAccounts>(settings.accounts_json);
            const journal = await tx.posting(str(event, 'company_id'), null, a, date ?? businessDate(), str(settings, 'currency'), `stripe-fee:${event.livemode}:${event.account_id}:${btId}`, btId, 'system:stripe', pair(fee >= 0 ? a.fees : a.stripe, fee >= 0 ? a.stripe : a.fees, Math.abs(fee)));
            await tx.writePosSettlements2({ fact0: ports.uuid(), company_id: event.company_id, account_id: event.account_id, livemode: event.livemode, btId: btId, amount: amount, fee: fee, fact7: JSON.stringify({ source: bt.source, net: bt.net, type: bt.type, created: bt.created }), journal: journal, fact9: ports.clock.now() });
        });
    }
    async function settlementSettings(db: PosStripeUnit, companyId: string) {
        const pos = await db.readPosSettings1({ companyId: companyId });
        if (pos)
            return pos;
        const hotel = await db.readPmsProperties1({ companyId: companyId }) ?? await db.readPmsProperties2({ companyId: companyId });
        if (!hotel)
            throw new Error('STRIPE_COMPANY_ACCOUNTS_REQUIRED');
        const c = json<{
            currency: string;
            chartId: string;
            accounts: {
                edc: string;
                bank: string;
                commission: string;
            };
        }>(hotel.config_json);
        return { currency: c.currency, accounts_json: JSON.stringify({ chartId: c.chartId, stripe: c.accounts.edc, bank: c.accounts.bank, fees: c.accounts.commission, disputes: c.accounts.commission }) };
    }
    async function settlement(db: PosStripeRepository, job: PosRow, event: PosRow, obj: Record<string, unknown>) {
        const payout = str(event, 'event_type').startsWith('payout.'), resource = await stripeCall(Boolean(event.livemode), `${payout ? 'payouts' : 'disputes'}/${String(obj.id)}`, { account: str(event, 'account_id') });
        if (resource.currency !== 'thb')
            throw new Error('POS_SETTLEMENT_CURRENCY');
        const amount = z.number().int().positive().parse(resource.amount), date = json<{
            accountingDate?: string;
        }>(job.payload_json).accountingDate ?? businessDate();
        const pendingTransactions = (resource.balance_transactions ?? []) as Array<{
            id: string;
        }>;
        for (const bt of pendingTransactions)
            await balanceTransaction(db, event, bt.id, date);
        let details: unknown = { status: resource.status };
        if (payout && resource.status === 'paid' && resource.automatic === false) {
            // Stripe cannot attribute constituent transactions to a manual payout. Retain
            // the verified balance movement for Finance to match to an approved journal.
            const bt = await stripeCall(Boolean(event.livemode), `balance_transactions/${String(resource.balance_transaction)}`, { account: str(event, 'account_id') });
            if (bt.type !== 'payout' || bt.source !== resource.id || bt.currency !== 'thb' || bt.amount !== -amount || bt.net !== -amount || bt.fee !== 0)
                throw new Error('POS_MANUAL_PAYOUT_BALANCE_MISMATCH');
            await db.writePosSettlements3({ fact0: ports.uuid(), company_id: event.company_id, account_id: event.account_id, livemode: event.livemode, id: resource.id, amount: amount, fact6: JSON.stringify({ status: 'paid', manualPayout: true, automatic: false, net: -amount, balanceTransaction: bt.id, reconciliationMode: 'balance', items: null }), fact7: ports.clock.now() });
            return;
        }
        if (payout && resource.status === 'paid') {
            const items: unknown[] = [];
            let after = '';
            for (let page = 0; page < 100; page++) {
                const response = await ports.provider.fetch(`https://api.stripe.com/v1/balance_transactions?${new URLSearchParams({ payout: resource.id, limit: '100', ...(after ? { starting_after: after } : {}) })}`, { headers: { Authorization: `Bearer ${ports.provider.secret(Boolean(event.livemode))}`, 'Stripe-Account': str(event, 'account_id') }, timeoutMs: 20000 });
                if (!response.ok)
                    throw new Error('POS_PAYOUT_ITEMS_UNAVAILABLE');
                const list = z.object({ data: z.array(objectSchema), has_more: z.boolean() }).parse(await response.json());
                for (const item of list.data) {
                    await balanceTransaction(db, event, item.id, date);
                    items.push({ id: item.id, amount: item.amount, fee: item.fee, net: item.net, source: item.source });
                }
                if (!list.has_more)
                    break;
                if (!list.data.length || page === 99)
                    throw new Error('POS_PAYOUT_TOO_LARGE');
                after = list.data.at(-1)!.id;
            }
            details = { items, status: resource.status };
            if (items.reduce<number>((sum, item) => sum + Number((item as {
                net: number;
            }).net), 0) !== amount)
                throw new Error('POS_PAYOUT_NET_MISMATCH');
            for (const item of items) {
                const entry = await db.readPosSettlements2({ account_id: event.account_id, livemode: event.livemode, fact2: (item as {
                        id: string;
                    }).id });
                if (entry?.state === 'unmapped')
                    throw new Error('POS_EXTERNAL_BALANCE_REVIEW');
            }
        }
        await db.transaction(async (tx) => {
            const settings = await settlementSettings(tx, str(event, 'company_id')), a = json<PosAccounts>(settings.accounts_json);
            let journal: string | null = null;
            if (payout && resource.status === 'failed' && (await tx.readPosPostings1({ fact0: `payout:${event.livemode}:${event.account_id}:${resource.id}` }) || await tx.readPosSettlements3({ company_id: event.company_id, account_id: event.account_id, livemode: event.livemode, id: resource.id })))
                await tx.posting(str(event, 'company_id'), null, a, date, 'THB', `payout-return:${event.livemode}:${event.account_id}:${resource.id}`, resource.id, 'system:stripe', pair(a.stripe, a.bank, amount));
            if (payout && resource.status === 'paid')
                journal = await tx.posting(str(event, 'company_id'), null, a, date, 'THB', `payout:${event.livemode}:${event.account_id}:${resource.id}`, resource.id, 'system:stripe', pair(a.bank, a.stripe, amount));
            // Disputes are tracked separately from sales/refunds. Post actual balance movements only.
            if (!payout) {
                for (const bt of pendingTransactions) {
                    const row = await tx.readPosSettlements4({ account_id: event.account_id, livemode: event.livemode, id: bt.id });
                    if (row) {
                        const value = num(row, 'amount_minor');
                        journal = await tx.posting(str(event, 'company_id'), null, a, date, 'THB', `dispute-move:${event.account_id}:${bt.id}`, resource.id, 'system:stripe', pair(value < 0 ? a.disputes : a.stripe, value < 0 ? a.stripe : a.disputes, Math.abs(value)));
                    }
                }
                if (resource.status === 'lost')
                    journal = await tx.posting(str(event, 'company_id'), null, a, date, 'THB', `dispute-loss:${event.account_id}:${resource.id}`, resource.id, 'system:stripe', pair(a.fees, a.disputes, amount));
            }
            await tx.writePosSettlements4({ fact0: ports.uuid(), company_id: event.company_id, account_id: event.account_id, livemode: event.livemode, fact4: payout ? 'payout' : 'dispute', id: resource.id, amount: amount, fact7: String(resource.status), fact8: JSON.stringify(details), journal: journal, fact10: ports.clock.now() });
        });
    }
    async function processJob(db: PosStripeRepository, holdingId: string, job: PosRow): Promise<boolean> {
        const payload = json<{
            livemode?: boolean;
            accountId?: string;
            holdingSlug?: string;
            userId?: string;
            assignmentId?: string;
            accountingDate?: string;
            providerAttemptStartedAt?: number;
            refundInstructionsAttempts?: Array<{
                email: string;
            }>;
        }>(job.payload_json);
        if (job.kind === 'balance_sync') {
            const assignment = await db.readAssignments1({ assignmentId: payload.assignmentId, userId: payload.userId, company_id: job.company_id, fact3: ports.clock.now(), fact4: ports.clock.now() });
            if (!assignment || (await db.loadIamAuthority(str(job, 'company_id'), String(payload.userId), str(assignment, 'role'))).permissions.includes('finance.approve') !== true)
                throw new Error('POS_CONNECTION_AUTHORITY_CHANGED');
            const connection = await db.readPosConnections2({ company_id: job.company_id, accountId: payload.accountId, fact2: Boolean(payload.livemode) });
            if (!connection)
                throw new Error('POS_CONNECTION_AUTHORITY_CHANGED');
            const event = { company_id: job.company_id, account_id: payload.accountId, livemode: Boolean(payload.livemode) } as PosRow;
            let after = '';
            for (let page = 0; page < 100; page++) {
                const list = z.object({ data: z.array(objectSchema), has_more: z.boolean() }).parse(await ports.provider.request(Boolean(payload.livemode), `balance_transactions?${new URLSearchParams({ limit: '100', ...(after ? { starting_after: after } : {}) })}`, { account: payload.accountId }));
                for (const item of list.data)
                    if (!String(item.source ?? '').startsWith('po_'))
                        await balanceTransaction(db, event, item.id, payload.accountingDate);
                if (!list.has_more)
                    return false;
                if (!list.data.length || page === 99)
                    throw new Error('POS_PAYOUT_TOO_LARGE');
                after = list.data.at(-1)!.id;
            }
        }
        if (job.kind === 'connect' || job.kind === 'connect_refresh') {
            if (!payload.holdingSlug || !/^[A-Za-z0-9_-]+$/.test(payload.holdingSlug))
                throw new Error('POS_CONNECTION_HOLDING_REQUIRED');
            const assignment = await db.readAssignments1({ assignmentId: payload.assignmentId, userId: payload.userId, company_id: job.company_id, fact3: ports.clock.now(), fact4: ports.clock.now() });
            if (!assignment)
                throw new Error('POS_CONNECTION_AUTHORITY_CHANGED');
            const authority = await db.loadIamAuthority(str(job, 'company_id'), String(payload.userId), str(assignment, 'role'));
            if (!authority.permissions.includes('pos.payments.configure') && !authority.permissions.includes('finance.approve'))
                throw new Error('POS_CONNECTION_AUTHORITY_CHANGED');
            let connection = await db.readPosConnections1({ company_id: job.company_id }), accountId = connection?.account_id;
            if (job.kind === 'connect' && (!accountId || connection?.livemode !== Boolean(payload.livemode))) {
                if (ports.clock.now() - num(job, 'created_at') > 23 * 60 * 60 * 1000)
                    throw new Error('POS_PROVIDER_LOOKUP_REQUIRED');
                const account = await stripeCall(Boolean(payload.livemode), 'accounts', { method: 'POST', key: `pos-connect-${job.id}-v2`, body: { country: 'TH', 'controller[fees][payer]': 'account', 'controller[losses][payments]': 'stripe', 'controller[requirement_collection]': 'stripe', 'controller[stripe_dashboard][type]': 'full', 'capabilities[card_payments][requested]': 'true', 'capabilities[transfers][requested]': 'true', 'capabilities[promptpay_payments][requested]': 'true', 'metadata[erpx_company_id]': str(job, 'company_id') } });
                accountId = account.id;
            }
            if (!accountId)
                throw new Error('POS_CONNECTION_MISSING');
            await bindAccount(db, holdingId, str(job, 'company_id'), String(accountId), Boolean(payload.livemode));
            connection = await db.readPosConnections1({ company_id: job.company_id });
            const url = ports.provider.settingsReturn?.(payload.holdingSlug, String(job.company_id))
                ?? `${ports.provider.origin()}/holdings/${payload.holdingSlug}/settings/stripe`;
            const link = z.object({ url: z.string().url(), expires_at: z.number().int().positive() }).parse(await ports.provider.request(Boolean(connection!.livemode), 'account_links', { method: 'POST', key: `pos-link-${job.id}`, body: { account: String(accountId), type: 'account_onboarding', return_url: url, refresh_url: url } }));
            if (typeof link.url !== 'string' || new URL(link.url).hostname !== 'connect.stripe.com')
                throw new Error('POS_CONNECT_URL_INVALID');
            await db.writePosJobs1({ fact0: JSON.stringify({ ...payload, accountId: String(accountId), livemode: Boolean(connection!.livemode), url: link.url, urlExpiresAt: link.expires_at * 1000 }), id: job.id });
            return false;
        }
        if (job.kind === 'checkout') {
            const p = (await db.readPosPayments8({ record_id: job.record_id, company_id: job.company_id }))!;
            if (!p)
                throw new Error('POS_PAYMENT_MISSING');
            if (p.state === 'succeeded' || p.state === 'cancelled')
                return false;
            const purchase: PosProviderPurchase = { companyId: str(p, 'company_id'), operation: 'payment.settle', recordId: str(p, 'id') };
            await db.requirePurchase(purchase);
            if (!p.provider_id) {
                if (p.method === 'stripe_promptpay') {
                    if (!p.payer_email)
                        throw new Error('POS_PROMPTPAY_EMAIL_REQUIRED');
                    const sale = (await db.readPosSales1({ sale_id: p.sale_id }))!;
                    const pi = await promptPayIntent(p, String(sale.number));
                    const qr = ((pi.next_action as Record<string, unknown> | undefined)?.promptpay_display_qr_code as Record<string, unknown> | undefined)?.data;
                    if (pi.status !== 'requires_action' || typeof qr !== 'string' || !qr)
                        throw new Error('POS_PROMPTPAY_QR_UNAVAILABLE');
                    await db.transaction(async (tx) => { await tx.writePosPayments5({ id: pi.id, qr: qr, fact2: ports.clock.now() + 30 * 60 * 1000, fact3: ports.clock.now(), id2: p.id }); await tx.enqueue(str(p, 'company_id'), 'check_payment', str(p, 'id')); }, purchase);
                    return false;
                }
                const sale = (await db.readPosSales2({ sale_id: p.sale_id }))!, env = await ports.resources(), binding = await env.control.controlOne4({ arg0: holdingId });
                const returnUrl = `${ports.provider.origin()}/holdings/${binding?.slug}/pos?sale=${sale.id}`;
                const session = await stripeCall(Boolean(p.livemode), 'checkout/sessions', { method: 'POST', account: str(p, 'stripe_account'), key: `pos-checkout-${p.id}`, body: { mode: 'payment', 'payment_method_types[0]': p.method === 'stripe_promptpay' ? 'promptpay' : 'card', 'line_items[0][price_data][currency]': 'thb', 'line_items[0][price_data][unit_amount]': String(p.amount_minor), 'line_items[0][price_data][product_data][name]': String(sale.number), 'line_items[0][quantity]': '1', 'metadata[pos_payment_id]': str(p, 'id'), 'payment_intent_data[metadata][pos_payment_id]': str(p, 'id'), success_url: returnUrl, cancel_url: returnUrl, expires_at: String(Math.floor(Number(p.created_at) / 1000) + 1800) } });
                if (typeof session.url !== 'string' || new URL(session.url).hostname !== 'checkout.stripe.com')
                    throw new Error('POS_CHECKOUT_URL_INVALID');
                await db.transaction(async (tx) => { await tx.writePosPayments6({ id: session.id, url: session.url, fact2: ports.clock.now(), id2: p.id }); await tx.enqueue(str(p, 'company_id'), 'check_payment', str(p, 'id')); }, purchase);
            }
            return false;
        }
        if (job.kind === 'check_payment' || job.kind === 'cancel_checkout') {
            const p = (await db.readPosPayments8({ record_id: job.record_id, company_id: job.company_id }))!;
            if (!p)
                throw new Error('POS_PAYMENT_MISSING');
            if (['succeeded', 'cancelled'].includes(str(p, 'state')))
                return false;
            if (job.kind === 'cancel_checkout') {
                if (!p.provider_id)
                    return true;
                await db.requirePurchase({ companyId: str(p, 'company_id'), operation: 'payment.settle', recordId: str(p, 'id') });
                if (p.method === 'stripe_promptpay' && String(p.provider_id).startsWith('pi_')) {
                    const pi = await stripeCall(Boolean(p.livemode), `payment_intents/${p.provider_id}`, { account: str(p, 'stripe_account') });
                    if (pi.status !== 'succeeded' && pi.status !== 'canceled')
                        await stripeCall(Boolean(p.livemode), `payment_intents/${p.provider_id}/cancel`, { method: 'POST', account: str(p, 'stripe_account'), key: `pos-cancel-${p.id}`, body: {} });
                    return checkPayment(db, p, payload.accountingDate);
                }
                const session = await stripeCall(Boolean(p.livemode), `checkout/sessions/${p.provider_id}`, { account: str(p, 'stripe_account') });
                if (session.status === 'open')
                    await stripeCall(Boolean(p.livemode), `checkout/sessions/${p.provider_id}/expire`, { method: 'POST', account: str(p, 'stripe_account'), key: `pos-expire-${p.id}`, body: {} });
            }
            return checkPayment(db, p, payload.accountingDate);
        }
        if (job.kind === 'complete_sale') {
            return db.transaction(async (tx) => {
                const sale = await tx.readPosSales3({ record_id: job.record_id, company_id: job.company_id });
                if (!sale || sale.state === 'completed')
                    return false;
                const paid = await tx.readPosPayments10({ id: sale.id });
                if (num(paid!, 'n') !== num(sale, 'total_minor'))
                    return true;
                await tx.completeSale(sale, 'system:stripe', payload.accountingDate);
                return false;
            }, { companyId: str(job, 'company_id'), operation: 'sale.complete', recordId: str(job, 'record_id') });
        }
        if (job.kind === 'refund') {
            const rp = (await db.readPosRefundPayments3({ record_id: job.record_id, company_id: job.company_id }))!;
            if (!rp)
                throw new Error('POS_REFUND_MISSING');
            if (rp.state === 'succeeded')
                return false;
            const p = (await db.readPosPayments6({ payment_id: rp.payment_id }))!;
            await db.requirePurchase({ companyId: str(p, 'company_id'), operation: 'refund.settle', recordId: str(rp, 'refund_id') });
            if (!rp.provider_id) {
                if (ports.clock.now() - (payload.providerAttemptStartedAt ?? num(job, 'created_at')) > 23 * 60 * 60 * 1000)
                    throw new Error('POS_PROVIDER_LOOKUP_REQUIRED');
                const paymentIntent = String(p.provider_id).startsWith('pi_') ? String(p.provider_id) : String((await stripeCall(Boolean(p.livemode), `checkout/sessions/${p.provider_id}`, { account: str(p, 'stripe_account') })).payment_intent);
                const baseKey = `pos-refund-${rp.id}-${rp.provider_attempt}`, body = { payment_intent: paymentIntent, amount: String(rp.amount_minor), 'metadata[pos_refund_payment_id]': str(rp, 'id'), ...(p.method === 'stripe_promptpay' && p.payer_email ? { instructions_email: String(p.payer_email) } : {}) };
                const replacements = z.array(z.object({ email: z.email().max(254) })).max(10).parse(payload.refundInstructionsAttempts ?? []);
                if (replacements.length && p.method !== 'stripe_promptpay')
                    throw new Error('POS_REFUND_EMAIL_LOCKED');
                let refund: StripeObject | undefined;
                // Replay every previous body/key first. A successful original result is
                // adopted; a timeout, unknown result or mismatched body never permits a new
                // refund. Only Stripe's explicit email rejection permits the next body.
                for (let attempt = 0; attempt <= replacements.length; attempt++) {
                    try {
                        refund = await stripeCall(Boolean(p.livemode), 'refunds', { method: 'POST', account: str(p, 'stripe_account'), key: attempt ? `${baseKey}-instructions-${attempt}` : baseKey, body: attempt ? { ...body, instructions_email: replacements[attempt - 1].email } : body });
                        break;
                    }
                    catch (error) {
                        if (attempt === replacements.length || !(error instanceof PosStripeApiError) || error.code !== 'POS_PROVIDER_REFUND_EMAIL_RESTRICTED')
                            throw error;
                    }
                }
                if (!refund)
                    throw new Error('POS_PROVIDER_LOOKUP_REQUIRED');
                await db.writePosRefundPayments2({ id: refund.id, id2: rp.id });
                rp.provider_id = refund.id;
            }
            const again = await checkRefund(db, rp, p, payload.accountingDate);
            if (!again)
                await db.transaction(tx => tx.releaseUnpaidReservations(str(p, 'sale_id')));
            return again;
        }
        if (job.kind === 'event')
            return stripeEvent(db, job, holdingId);
        throw new Error('POS_JOB_UNKNOWN');
    }
    /** Durable leases allow the jobs service to recover work after a process restart. */
    async function drainPos(db: PosStripeRepository, holdingId: string, limit = 8, companyId: string | null = null) {
        for (let i = 0; i < limit; i++) {
            const token = ports.uuid(), now = ports.clock.now();
            const job = await db.readPosJobs1({ token: token, fact1: now + 120000, now: now, now2: now, companyId: companyId, companyId2: companyId });
            if (!job)
                break;
            const renew = ports.leases.repeat(() => { void db.writePosJobs2({ fact0: ports.clock.now() + 120000, id: job.id, token: token }).catch(() => { }); }, 30000);
            try {
                const again = await processJob(db, holdingId, job);
                await db.writePosJobs3({ fact0: again ? 'pending' : 'completed', fact1: ports.clock.now() + 15000, id: job.id, token: token });
            }
            catch (error) {
                if (error instanceof PosStripeApiError)
                    ports.report(JSON.stringify({ event: 'pos_provider_request_failed', jobId: job.id, kind: job.kind, resource: error.resource, status: error.status, providerCode: error.providerCode, parameter: error.parameter, requestId: error.requestId }));
                const raw = (error as {
                    code?: string;
                    message?: string;
                }).code ?? (error as Error).message;
                const code = /^[A-Z0-9_]+$/.test(raw) ? raw : 'POS_JOB_FAILED', exception = num(job, 'attempts') >= 6 || ['POS_PERIOD_CLOSED', 'POS_CONNECTION_AUTHORITY_CHANGED', 'POS_PROMPTPAY_EMAIL_REQUIRED', 'POS_PROVIDER_REFUND_EMAIL_RESTRICTED', 'POS_REFUND_EMAIL_LOCKED', 'POS_PROVIDER_PAYMENT_MISMATCH', 'POS_PROVIDER_REFUND_FAILED', 'POS_STRIPE_NOT_CONFIGURED', 'POS_PROVIDER_LOOKUP_REQUIRED', 'POS_MANUAL_PAYOUT_BALANCE_MISMATCH'].includes(code);
                await db.writePosJobs4({ fact0: exception ? 'exception' : 'pending', fact1: ports.clock.now() + Math.min(60000, 1000 * 2 ** num(job, 'attempts')), code: code, id: job.id, token: token });
                if (exception)
                    await db.audit(str(job, 'company_id'), null, 'system:stripe', 'job.exception', str(job, 'record_id'), { jobId: job.id, code });
            }
            finally {
                ports.leases.cancel(renew);
            }
        }
    }
    async function drainAllPos() {
        const env = await ports.resources();
        for (const slot of env.holdings) {
            const db = await ports.forHolding(slot.id);
            if ((await db.readContext1({}))?.name)
                await drainPos(db, slot.id);
        }
    }
    return { stripeCall, stripeListCall, posStripeWebhook, oauthUrl, finishPosOAuth, drainPos, drainAllPos, verifyPosStripeSignature: verifySignature };
}
