import 'server-only';
import{PosStripeApiError,type StripeOptions}from '@erpx/server-pos/domain/stripe';
import{ApiError}from '@erpx/server-runtime/public/http';
import{createStripeCalls}from '@erpx/server-pos/application/stripe-calls';
function fail(code:string,message:string,status=409):never{throw new ApiError(status,code,message);}
export function secret(live: boolean) {
    const key = process.env[live ? 'POS_STRIPE_LIVE_SECRET_KEY' : 'POS_STRIPE_TEST_SECRET_KEY'];
    if (!key?.startsWith(live ? 'sk_live_' : 'sk_test_'))
        fail('POS_STRIPE_NOT_CONFIGURED', 'Platform has not configured Stripe POS');
    return key;
}
export async function stripeRequest(live: boolean, path: string, options: StripeOptions = {}): Promise<unknown> {
    if (!/^[a-z0-9_/?=&.%+-]+$/i.test(path))
        throw new Error('STRIPE_PATH_INVALID');
    const response = await fetch(`https://api.stripe.com/v1/${path}`, { method: options.method ?? 'GET', headers: { Authorization: `Bearer ${secret(live)}`, 'Stripe-Version': '2025-02-24.acacia', ...(options.account ? { 'Stripe-Account': options.account } : {}), ...(options.key ? { 'Idempotency-Key': options.key } : {}), ...(options.body ? { 'Content-Type': 'application/x-www-form-urlencoded' } : {}) }, body: options.body ? new URLSearchParams(options.body) : undefined, signal: AbortSignal.timeout(20000), cache: 'no-store' });
    if (!response.ok) {
        const details = await response.json().catch(() => ({})) as {
            error?: {
                code?: string;
                param?: string;
                message?: string;
                type?: string;
            };
        };
        throw new PosStripeApiError(response.status, details.error?.code ?? null, details.error?.param ?? null, response.headers.get('request-id'), details.error?.message ?? `STRIPE_HTTP_${response.status}`, path.split('?')[0], details.error?.type ?? null);
    }
    return response.json();
}
export const{stripeCall,stripeListCall}=createStripeCalls({request:stripeRequest});
