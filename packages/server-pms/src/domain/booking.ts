import { z } from "zod";
import { guestInputSchema } from "@erpx/feature-pms/lib/pms-booking-domain";
export type BookingEnv = {
    secret: string;
    origin: string;
    holdingOrigins?: Readonly<Record<string,string>>;
    apiKey?: string;
    from?: string;
};
export function bookingPublicUrl(env:BookingEnv,slug:string,suffix:string){
    const origin=env.holdingOrigins?.[slug];
    return origin?`${origin}/book${suffix}`:`${env.origin}/book/${encodeURIComponent(slug)}${suffix}`;
}
export type BookingPaymentRow = {
    id: string;
    property_id: string;
    reservation_id: string;
    company_id: string;
    holding_slug: string;
    account_id: string;
    livemode: boolean;
    amount_minor: number;
    currency: string;
    checkout_id: string | null;
    checkout_url: string | null;
    created_at: number;
    status: 'payment_pending' | 'confirmed' | 'expired' | 'refund_review' | 'cancelled';
    expires_at: number;
    refund_state: string | null;
    allocation_json: string;
    methods_json: string;
    clearing_account: string;
    payment_intent_id: string | null;
    provider_succeeded_at: number | null;
    purpose: "reservation" | "change";
    proposal_id: string | null;
};
export type PaymentAllocation = {
    stayId: string;
    guestAccountId: string;
    amount: number;
};
export const publicReservationSchema = z.object({
    propertyId: z.string().min(1).max(128),
    quoteToken: z.string().min(1).max(100000),
    guest: guestInputSchema,
    notes: z.string().trim().max(2000).default(""),
});
