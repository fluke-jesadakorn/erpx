import { billingHandler } from "@/server/modules/billing/public/billing-api";
export const GET = billingHandler(true);
export const POST = billingHandler(true);
