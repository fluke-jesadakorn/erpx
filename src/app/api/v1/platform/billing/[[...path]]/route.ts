import { billingHandler } from "@/server/billing-api";
export const GET = billingHandler(true);
export const POST = billingHandler(true);
