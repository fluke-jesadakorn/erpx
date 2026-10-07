import {stripeObjectSchema,type BillingStripe} from '../domain/stripe';
export interface StripeTransportResponse {status:number;ok:boolean;json():Promise<unknown>;}
export interface SandboxStripePorts {secretKey():string|undefined;error(status:number,code:string,message:string):Error;request(path:string,fields?:Record<string,string>,key?:string):Promise<StripeTransportResponse>;}
export function createSandboxStripe(ports:SandboxStripePorts){function retryRequired() {
  return ports.error(
    503,
    "STRIPE_RETRY_REQUIRED",
    "Stripe could not complete the Sandbox operation. Retry the same command.",
  );
}function stripe(): BillingStripe {
  return async (path, fields, key) => {
    if (!ports.secretKey()?.startsWith("sk_test_"))
      throw ports.error(
        503,
        "SANDBOX_REQUIRED",
        "Only Stripe Sandbox test keys are accepted.",
      );
    let r: StripeTransportResponse;
    try {
      r = await ports.request(path, fields, key);
    } catch {
      // A lost response is ambiguous. Preserve the caller's original key.
      throw retryRequired();
    }
    // https://docs.stripe.com/api/errors: 401 and 403 need credential repair.
    // Never expose provider bodies or credentials in a receipt or operator UI.
    if (r.status === 401)
      throw ports.error(
        503,
        "STRIPE_AUTHENTICATION_REQUIRED",
        "Stripe Sandbox rejected its configured key. Repair the Sandbox credential before retrying this command.",
      );
    if (r.status === 403)
      throw ports.error(
        503,
        "STRIPE_PERMISSION_REQUIRED",
        "The configured Stripe Sandbox key cannot perform this operation. Repair its permissions before retrying this command.",
      );
    if (!r.ok) throw retryRequired();
    let raw;
    try {
      raw = await r.json() as {livemode?:unknown;data?:{livemode?:boolean}[];id?:unknown};
    } catch {
      throw retryRequired();
    }
    // List/portal objects do not carry livemode. Every monetary object must be explicitly test.
    if (
      raw.livemode === true ||
      (Array.isArray(raw.data) &&
        raw.data.some((v: { livemode?: boolean }) => v.livemode === true))
    )
      throw new Error("LIVE_STRIPE_OBJECT_REJECTED");
    return stripeObjectSchema.parse({ ...raw, id: raw.id ?? "list" });
  };
}
return stripe();}
