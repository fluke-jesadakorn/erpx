import "server-only";
import { stripeListCall } from "../public/pos-stripe";
import { ApiError } from "../../runtime/public/http";
import { createVerifiedStripePaymentTime } from "../application/stripe-payment-time";
// Read fixture mode per invocation, preserving process-local sandbox opt-in.
export function verifiedStripePaymentTime(input: {
    livemode: boolean;
    accountId: string;
    paymentIntentId: string;
    known: number | null;
    createdAt: number;
    now: number;
}) { return createVerifiedStripePaymentTime({ faultFixtures: process.env.NODE_ENV === 'test' && process.env.ERPX_TEST_STRIPE_SANDBOX !== '1', events: stripeListCall, error: (status, code, message) => new ApiError(status, code, message) }).verifiedStripePaymentTime(input); }
