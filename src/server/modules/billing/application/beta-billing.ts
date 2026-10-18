import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {legacyStripeObjectSchema,type LegacyStripeEvent,type LegacyStripeObject,type LegacyStripeRequest} from '../domain/beta-billing';
export interface BetaBillingPorts{error(status:number,code:string,message:string):Error;now():number;origin:string;secretKey():string|undefined;webhookSecret():string|undefined;transport(path:string,fields?:Record<string,string>,key?:string):Promise<{ok:boolean;json():Promise<unknown>}>;verifyHmac(secret:string,payload:string,bytes:Uint8Array):Promise<boolean>;customer(holdingId:string):Promise<{customer_id:string|null}|null>;retainCustomer(holdingId:string,customerId:string,now:number):Promise<unknown>;enqueueEvent(event:LegacyStripeEvent,raw:string):Promise<unknown>;inboxState(id:string):Promise<{state:string}|null>;legacyCustomer(customerId:string):Promise<{holding_id:string}|null>;ignoreEvent(id:string,now:number):Promise<unknown>;applyEvent(facts:{event:LegacyStripeEvent;current:NonNullable<LegacyStripeObject['data']>[number]|null;holdingId:string;updatedAt:number;appliedAt:number}):Promise<unknown>;}
export function createBetaBilling(ports:BetaBillingPorts){
async function stripeRequest(
  path: string,
  fields?: Record<string, string>,
  key?: string,
) {
  if (!ports.secretKey()?.startsWith("sk_test_"))
    throw ports.error(
      503,
      "BILLING_NOT_CONFIGURED",
      "Test billing is not configured. Beta remains free.",
    );
  const response=await ports.transport(path,fields,key);
  if (!response.ok)
    throw ports.error(
      503,
      "BILLING_UNAVAILABLE",
      "Stripe test billing is temporarily unavailable.",
    );
  return legacyStripeObjectSchema.parse(await response.json());
}

async function billingLink(holdingId:string,slug:string,email:string,action:"checkout"|"portal") {
  if (action === "checkout") throw ports.error(409,"LEGACY_CHECKOUT_DISABLED","Choose a Support plan in Usage & billing instead.");
  let row=await ports.customer(holdingId);
  if(!row)throw ports.error(404,"BILLING_UNAVAILABLE","Beta company not found.");
  if(!row.customer_id){
    const customer=await stripeRequest("customers",{email,"metadata[holding_id]":holdingId},`erpx-beta-customer-${holdingId}`);
    if(typeof customer.id!=="string"||customer.livemode)throw new Error("INVALID_STRIPE_CUSTOMER");
    await ports.retainCustomer(holdingId,customer.id,ports.now());
    row=await ports.customer(holdingId);
  }
  const returnUrl=new URL(canonicalUiHref(`/holdings/${slug}/settings?section=usage`),ports.origin).toString();
  const result=await stripeRequest("billing_portal/sessions",{customer:row!.customer_id!,return_url:returnUrl});
  if(typeof result.url!=="string"||!["checkout.stripe.com","billing.stripe.com"].includes(new URL(result.url).hostname))throw new Error("INVALID_STRIPE_URL");
  return {url:result.url,testMode:true};
}

async function verifyStripeSignature(
  raw: string,
  header: string | null,
  secret: string,
  now = ports.now(),
) {
  const parts = (header ?? "").split(","),
    time = parts.find((p) => p.startsWith("t="))?.slice(2),
    signatures = parts
      .filter((p) => p.startsWith("v1="))
      .map((p) => p.slice(3));
  if (!time || !/^\d+$/.test(time) || Math.abs(now / 1000 - Number(time)) > 300)
    return false;
  for (const signature of signatures) {
    if (!/^[a-f0-9]{64}$/.test(signature)) continue;
    const bytes = Uint8Array.from(signature.match(/../g)!, (x) =>
      parseInt(x, 16),
    );
    if (
      await ports.verifyHmac(secret,`${time}.${raw}`,bytes)
    )
      return true;
  }
  return false;
}

async function stripeWebhook(
  raw: string,
  signature: string | null,
  retrieve?: LegacyStripeRequest,
) {
  if (
    !ports.webhookSecret() ||
    !(await verifyStripeSignature(raw, signature, ports.webhookSecret()!))
  )
    throw ports.error(400, "INVALID_SIGNATURE", "Invalid Stripe signature.");
  const event = JSON.parse(raw) as LegacyStripeEvent;
  if (
    event.livemode !== false ||
    event.data?.object?.livemode === true ||
    (ports.secretKey() !== undefined && !ports.secretKey()!.startsWith("sk_test_")) ||
    typeof event.id !== "string" ||
    typeof event.type !== "string" ||
    !Number.isSafeInteger(event.created)
  )
    throw ports.error(
      400,
      "INVALID_EVENT",
      "Only Stripe test events are accepted.",
    );
    await ports.enqueueEvent(event,raw);
  if (!retrieve) return { received: true };
  return applyLegacyStripeEvent(event,retrieve);
}

async function applyLegacyStripeEvent(event: LegacyStripeEvent, retrieve = stripeRequest) {
  const inbox=await ports.inboxState(event.id);
  if (inbox?.state !== "pending") return { received: true };
  const relevant =
      /^(customer\.subscription\.|invoice\.(paid|payment_failed|payment_action_required)|checkout\.session\.completed)/.test(
        event.type,
      ),
    customer = event.data?.object?.customer;
  const row =
    typeof customer === "string"
      ? await ports.legacyCustomer(customer)
      : null;
  if (!relevant || !row) {
    await ports.ignoreEvent(event.id,ports.now());
    return { received: true };
  }
  // Query current Stripe state; an older delivery must never restore an obsolete subscription snapshot.
  const subscriptions = await retrieve(
    `subscriptions?customer=${encodeURIComponent(String(customer))}&status=all&limit=10`,
  );
  const list = Array.isArray(subscriptions.data) ? subscriptions.data : [];
  if (list.some((s) => s.livemode !== false))
    throw new Error("LIVE_SUBSCRIPTION_REJECTED");
  const current = list.sort((a, b) => b.created - a.created)[0];
  await ports.applyEvent({event,current:current??null,holdingId:row.holding_id,updatedAt:ports.now(),appliedAt:ports.now()});
  return { received: true };
}
return {stripeRequest,billingLink,verifyStripeSignature,stripeWebhook,applyLegacyStripeEvent};}
