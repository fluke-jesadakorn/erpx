/** Compatibility entry point; legacy implementation belongs to billing. */
export {applyLegacyStripeEvent,billingLink,stripeRequest,stripeWebhook,verifyStripeSignature} from "@/server/modules/billing/adapters/beta-billing";
