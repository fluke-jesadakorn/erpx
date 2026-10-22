import {z} from 'zod';
import type {BillingInvoice,BillingView} from "@erpx/feature-billing/lib/billing";
export type {BillingAccount,Enrollment as BillingEnrollment,BillingPeriod,BillingCost,BillingPlan,BillingInvoice,BillingView} from "@erpx/feature-billing/lib/billing";
export type BillingActor = {
  userId: string;
  platform: boolean;
  holdingId?: string;
  email: string;
};
const amount = z.number().int().min(0).max(100000000);
export const billingInput = z
  .object({
    action: z.enum([
      "enroll",
      "change-plan",
      "cancel-hosted",
      "withdraw",
      "payment-mode",
      "portal",
      "setup",
      "cost",
      "delete-cost",
      "complete-costs",
      "approve",
      "void",
      "revise-plan",
      "retry",
      "resolve-legacy",
      "assign-target",
      "verify-provider-cost",
      "verify-statement",
      "approve-target",
      "rate-target",
      "record-operating-cost",
      "review-target-capacity",
      "approve-target-rollout",
      "approve-target-extra",
    ]),
    version: z.number().int().nonnegative(),
    reason: z.string().trim().min(3).max(1000),
    planId: z.enum(["infrastructure", "standard", "priority"]).optional(),
    supportSatang: amount.optional(),
    planRevision: z.number().int().positive().optional(),
    paymentMode: z.enum(["manual", "auto"]).optional(),
    consent: z.boolean().optional(),
    periodId: z.string().max(100).optional(),
    costId: z.string().max(100).optional(),
    invoiceId: z.string().max(100).optional(),
    jobId: z.string().max(100).optional(),
    category: z
      .enum(["hosting", "storage", "provider_ai", "adjustment"])
      .optional(),
    amountSatang: z.number().int().min(-100000000).max(100000000).optional(),
    serviceStart: z.number().int().optional(),
    serviceEnd: z.number().int().optional(),
    note: z.string().trim().max(2000).optional(),
    simulated: z.boolean().optional(),
    evidenceId: z.string().max(100).nullable().optional(),
    originalInvoiceId: z.string().max(100).optional(),
    complete: z.boolean().optional(),
    targetMonth: z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/).optional(),
    targetMode: z.enum(["shadow", "sandbox_invoice"]).optional(),
    priceId: z.string().max(100).optional(),
    sourceKey: z.string().regex(/^[a-f0-9]{64}$/).optional(),
    provider: z.string().trim().min(1).max(100).optional(),
    statementId: z.string().trim().min(1).max(200).optional(),
    originalCurrency: z.string().regex(/^[A-Z]{3}$/).optional(),
    originalMinor: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    providerTotalMinor: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    fxNumerator: z.number().int().positive().max(Number.MAX_SAFE_INTEGER).optional(),
    fxDenominator: z.number().int().positive().max(Number.MAX_SAFE_INTEGER).optional(),
    providerCostSatang: z.number().int().min(0).max(100000000).optional(),
    operatingCategory: z.enum(["infrastructure", "backup", "stripe", "support"]).optional(),
    operatingCostSatang: amount.optional(),
    projectedOriginalBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    primaryAvailableBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    backupAvailableBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    reservedBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    companyLimitBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    minimumMarginBps: z.number().int().min(0).max(10000).optional(),
    extraKind: z.enum(["setup", "support"]).optional(),
  })
  .strict();
export type BillingInput = z.infer<typeof billingInput>;

export type BillingInvoiceRow=BillingInvoice & {holding_id:string;payment_mode:'manual'|'auto';rating_model:string;updated_at:number};
export type BillingReceipt={id:string;digest:string;result_json:string;created_at:number};
export type BillingJobRow={id:string;holding_id:string;invoice_id:string;kind:string;state:string;progress_json:string;attempts:number;next_at:number;created_at:number;last_error:string|null};
export type BillingMeterRow={provider:string;cost_reconciled_at:number|null;funding_source:string;request_status:string;gateway_status:string;cost_status:string;verified_cost_satang:string|number|null};
export type BillingPriceRow={id:string;effective_month:string;rating_enabled:boolean;seat_terms_status:string};
export type BillingViewFacts={account:BillingView['account'];enrollment:BillingView['enrollment'];plans:BillingView['plans'];periods:BillingView['periods'];costs:BillingView['costs'];invoices:BillingView['invoices'];audit:BillingView['audit'];jobs:BillingView['jobs'];legacy:BillingView['legacy']};
