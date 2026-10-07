import { billingHandler } from "@/server/modules/billing/public/billing-api";
export const GET = billingHandler(false);
export const POST = billingHandler(false);
