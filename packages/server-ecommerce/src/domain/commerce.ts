import { z } from "zod";
const reason = z.string().trim().min(3).max(2000),
  version = z.number().int().positive(),
  qty = z.string().regex(/^\d+(\.\d{1,3})?$/);
export const listingSchema = z.object({
  channelId: z.string().min(1),
  productId: z.string().min(1),
  title: z.string().trim().min(2).max(200),
  currency: z
    .string()
    .length(3)
    .transform((x) => x.toUpperCase()),
  price: z.string().regex(/^\d+(\.\d{1,2})?$/),
});
export const cartSchema = z.object({
  channelId: z.string().min(1),
  partyId: z.string().min(1),
  warehouseId: z.string().min(1),
  currency: z
    .string()
    .length(3)
    .transform((x) => x.toUpperCase()),
});
export const lineSchema = z.object({
  cartId: z.string().min(1),
  expectedVersion: version,
  listingId: z.string().min(1),
  listingVersion: version,
  quantity: qty,
});
export const checkoutSchema = z.object({
  cartId: z.string().min(1),
  expectedVersion: version,
  orderDate: z.string().date(),
  dueDate: z.string().date(),
});
export const actionSchema = z.object({
  action: z.enum([
    "activate",
    "reject",
    "retire",
    "publish",
    "approve-order",
    "sync-fulfillment",
    "request-return",
    "abandon",
  ]),
  expectedVersion: version,
  reason,
  expiresAt: z.number().int().positive().optional(),
  fulfillmentId: z.string().min(1).optional(),
  fulfillmentVersion: version.optional(),
  date: z.string().date().optional(),
  lines: z
    .array(z.object({ lineId: z.string().min(1), quantity: qty, condition:z.enum(['unopened','damaged','unknown']).default('unknown'), conditionNote:z.string().trim().max(500).default(''), qualityDueDate:z.union([z.string().date(),z.literal('')]).default(''),allocations:z.array(z.object({lotId:z.string().min(1),quantity:qty})).max(200).optional() }))
    .optional(),
});

export type ListingInput = z.infer<typeof listingSchema>;
export type CartInput = z.infer<typeof cartSchema>;
export type CartLineInput = z.infer<typeof lineSchema>;
export type CheckoutInput = z.infer<typeof checkoutSchema>;
export type EcommerceAction = z.infer<typeof actionSchema>;
export type EcommerceRecordKind = "channel" | "listing" | "checkout" | "cart";

export function cartLineCurrent(cart: Record<string, unknown>, listing: Record<string, unknown>, input: CartLineInput) {
 return cart.status === "open" && Number(cart.version) === input.expectedVersion && listing.status === "published" && listing.channel_status === "active" && Number(listing.version) === input.listingVersion && cart.channel_id === listing.channel_id && cart.currency === listing.currency;
}
export function reviewTransition(kind: "channel" | "listing", action: EcommerceAction["action"]) {
 const allowed = kind === "channel" ? ["activate", "reject", "retire"] : ["publish", "reject", "retire"];
 if (!allowed.includes(action)) return null;
 const active = kind === "channel" ? "active" : "published", expected = action === "retire" ? active : "draft";
 return { expected, status: action === "activate" || action === "publish" ? active : action === "reject" ? "rejected" : "retired" };
}
