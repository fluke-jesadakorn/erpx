export class PosStripeApiError extends Error {
    readonly code: string;
    constructor(readonly status: number, readonly providerCode: string | null, readonly parameter: string | null, readonly requestId: string | null, message: string, readonly resource: string = '', readonly providerType: string | null = null) { super(message); this.code = status === 400 && resource === 'refunds' && message.startsWith('Cannot send email.') ? 'POS_PROVIDER_REFUND_EMAIL_RESTRICTED' : `STRIPE_HTTP_${status}`; }
}
export type StripeOptions = {
    account?: string;
    method?: "POST" | "GET";
    body?: Record<string, string>;
    key?: string;
};
