export function createVerifiedStripePaymentTime(ports: {
    faultFixtures: boolean;
    events(live: boolean, path: string, options: {
        account: string;
    }): Promise<{
        data: Array<Record<string, unknown>>;
        has_more: boolean;
    }>;
    error(status: number, code: string, message: string): Error;
}) {
    /** Signed webhook time or an authenticated Stripe event, never redirect/local receipt time. */
    async function verifiedStripePaymentTime(input: {
        livemode: boolean;
        accountId: string;
        paymentIntentId: string;
        known: number | null;
        createdAt: number;
        now: number;
    }) {
        if (input.known && Number.isSafeInteger(input.known) && input.known > 0)
            return input.known;
        // Explicit fault-injection fixtures have no provider events. Real sandbox rehearsals opt out.
        if (ports.faultFixtures)
            return input.now;
        let after: string | undefined;
        for (let page = 0; page < 5; page++) {
            const query = new URLSearchParams({ type: 'payment_intent.succeeded', limit: '100', 'created[gte]': String(Math.floor((input.createdAt - 3600000) / 1000)), ...(after ? { starting_after: after } : {}) });
            const result = await ports.events(input.livemode, `events?${query}`, { account: input.accountId });
            const event = result.data.find(event => (event.data as {
                object?: {
                    id?: string;
                };
            } | undefined)?.object?.id === input.paymentIntentId);
            if (event && Number.isSafeInteger(event.created) && Number(event.created) > 0)
                return Number(event.created) * 1000;
            if (!result.has_more || !result.data.length)
                break;
            after = String(result.data[result.data.length - 1].id);
        }
        throw ports.error(409, 'PMS_SUCCESS_TIME_PENDING', 'Waiting for verified provider completion time');
    }
    return { verifiedStripePaymentTime };
}
