import { APP_CURRENCY, APP_LOCALE } from "./localization";

/** Calendar boundaries are Bangkok midnight, independent of the machine's timezone. */
export const BILLING_ZONE = "Asia/Bangkok";
export function billingMonth(now: number) {
  return new Date(now + 7 * 3600000).toISOString().slice(0, 7);
}
export function monthBounds(month: string) {
  if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(month))
    throw new Error("INVALID_BILLING_MONTH");
  const [year, m] = month.split("-").map(Number);
  return {
    start: Date.UTC(year, m - 1, 1) - 7 * 3600000,
    end: Date.UTC(year, m, 1) - 7 * 3600000,
  };
}
export function proratedSupport(amount: number, start: number, month: string) {
  const b = monthBounds(month),
    day = 86400000;
  const firstDay = Math.max(
    b.start,
    Math.floor((start + 7 * 3600000) / day) * day - 7 * 3600000,
  );
  return Math.round(
    (amount * Math.max(0, (b.end - firstDay) / day)) /
      ((b.end - b.start) / day),
  );
}
export const baht = (satang: number) =>
  new Intl.NumberFormat(APP_LOCALE, {
    style: "currency",
    currency: APP_CURRENCY,
    currencyDisplay: "code",
  }).format(satang / 100);
export type BillingPlan = {
  id: string;
  revision: number;
  name: string;
  support_satang: number;
  effective_month: string;
};
export type BillingAccount = {
  holding_id: string;
  version: number;
  customer_id: string | null;
  payment_mode: "manual" | "auto";
  consent_at: number | null;
  setup_state?: string | null;
  card_json: string | null;
};
export type Enrollment = {
  id: string;
  plan_id: string;
  started_at: number;
  ended_at: number | null;
  pending_plan: string | null;
  pending_end: boolean;
  effective_at: number | null;
};
export type BillingPeriod = {
  id: string;
  month: string;
  start_at: number;
  end_at: number;
  plan_id: string;
  plan_revision: number;
  support_satang: number;
  costs_complete: boolean;
  closed_at: number | null;
};
export type BillingCost = {
  id: string;
  period_id: string;
  category: string;
  amount_satang: number;
  service_start: number;
  service_end: number;
  note: string;
  simulated: boolean;
  evidence_id: string | null;
  invoice_id: string | null;
  original_invoice_id: string | null;
};
export type InvoiceLine = {
  description: string;
  amount: number;
  start: number;
  end: number;
  costId?: string;
  simulated?: boolean;
};
export type BillingInvoice = {
  id: string;
  period_id: string;
  kind: string;
  status: string;
  total_satang: number;
  lines_json: string;
  stripe_id: string | null;
  hosted_url: string | null;
  pdf_url: string | null;
  due_at: number | null;
  payment_state: string | null;
  created_at: number;
  original_invoice_id: string | null;
};
export type BillingView = {
  sandbox: true;
  now: number;
  account: BillingAccount | null;
  enrollment: Enrollment | null;
  plans: BillingPlan[];
  periods: BillingPeriod[];
  costs: BillingCost[];
  invoices: BillingInvoice[];
  audit: {
    id: string;
    actor: string;
    action: string;
    reason: string;
    created_at: number;
  }[];
  jobs: { id: string; state: string; last_error: string | null }[];
  legacy: { status: string; subscription_id: string | null } | null;
};
