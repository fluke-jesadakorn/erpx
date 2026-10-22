import {z} from 'zod';
export const stripeObjectSchema = z
  .object({ id: z.string(), livemode: z.boolean().optional() })
  .passthrough();
export type StripeObject = z.infer<typeof stripeObjectSchema>;
export type BillingStripe = (
  path: string,
  fields?: Record<string, string>,
  key?: string,
) => Promise<StripeObject>;
export function stripeUrl(value: unknown, hosts: string[]) {
  if (typeof value !== "string") return null;
  const u = new URL(value);
  if (u.protocol !== "https:" || !hosts.includes(u.hostname))
    throw new Error("INVALID_STRIPE_URL");
  return value;
}
export function stripeList(value: StripeObject): StripeObject[] {
  return z.array(stripeObjectSchema).parse(value.data ?? []);
}
export function testObject(value: StripeObject) {
  if (value.livemode !== false) throw new Error("TEST_STRIPE_OBJECT_REQUIRED");
  return value;
}
