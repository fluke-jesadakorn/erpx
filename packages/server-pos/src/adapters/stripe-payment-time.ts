import "server-only";
import { stripeListCall } from "@erpx/server-pos/public/pos-stripe";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createVerifiedStripePaymentTime } from "@erpx/server-pos/application/stripe-payment-time";
// Read fixture mode per invocation, preserving process-local sandbox opt-in.
export function verifiedStripePaymentTime(input: {
    livemode: boolean;
    accountId: string;
    paymentIntentId: string;
    known: number | null;
    createdAt: number;
    now: number;
}) { return createVerifiedStripePaymentTime({ faultFixtures: process.env.NODE_ENV === 'test' && process.env.ERPX_TEST_STRIPE_SANDBOX !== '1', events: stripeListCall, error: (status, code, message) => new ApiError(status, code, message) }).verifiedStripePaymentTime(input); }
