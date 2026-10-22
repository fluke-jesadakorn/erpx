import {z} from 'zod';export const country = z.string().length(2).transform((value) => value.toUpperCase());
export const currency = z.string().length(3).transform((value) => value.toUpperCase());
export const date = z.string().date();
export function payrollCsvCell(value:unknown){
  const raw=String(value);
  const safe=/^[\s\u0000-\u001f]*[=+\-@]/u.test(raw)?`'${raw}`:raw;
  return `"${safe.replaceAll('"','""')}"`;
}
export const createPersonSchema = z.object({
  legalName: z.string().trim().min(1).max(200),
  preferredName: z.string().trim().max(160).optional(),
  email: z.string().email().optional(),
  phone: z.string().trim().max(60).optional(),
  countryCode: country,
  userId: z.never().optional(),
});
export const createEngagementSchema = z.object({
  personId: z.string().min(1),
  branchId: z.string().nullable().optional(),
  departmentId: z.string().nullable().optional(),
  workerType: z.enum(["employee", "contractor"]),
  employmentType: z.enum(["salaried", "daily", "hourly", "shift", "contractor"]),
  startDate: date,
  endDate: date.nullable().optional(),
  managerPersonId: z.string().nullable().optional(),
  jobTitle: z.string().trim().min(1).max(160),
  currency,
  payGroupId: z.string().nullable().optional(),
});
export const compensationSchema = z.object({
  engagementId: z.string().min(1),
  effectiveFrom: date,
  effectiveTo: date.nullable().optional(),
  payFrequency: z.enum(["weekly", "fortnightly", "semimonthly", "monthly", "off_cycle"]),
  baseAmount: z.string().regex(/^\d+(\.\d{1,6})?$/).default("0"),
  hourlyRate: z.string().regex(/^\d+(\.\d{1,6})?$/).default("0"),
  dailyRate:z.string().regex(/^\d+(\.\d{1,6})?$/).default("0"),
  standardDailyHours:z.number().positive().max(24).default(8),
  expectedHours: z.number().min(0).max(744).default(0),
  currency,
  components: z.array(z.object({
    code: z.string().min(1).max(80),
    label: z.string().min(1).max(160),
    kind: z.enum(["earning", "deduction", "employer"]),
    amount: z.string().regex(/^\d+(\.\d{1,6})?$/),
    taxable: z.boolean().default(false),
    incomeTiming: z.enum(["regular", "irregular", "non_taxable"]).optional(),
    socialSecurityWage: z.boolean().optional(),
  })).max(50).default([]),
});
export const compensationTransitionSchema = z.object({
  status: z.enum(["approved", "superseded"]),
  expectedVersion: z.number().int().positive(),
});
export const engagementTransitionSchema = z.object({
  status: z.enum(["active", "suspended", "ended"]),
  expectedVersion: z.number().int().positive(),
  endDate: date.nullable().optional(),
});
export const payrollCostVerificationSchema=z.object({expectedVersion:z.number().int().positive(),evidenceRef:z.string().trim().min(8).max(500)}).strict();
export const payGroupSchema = z.object({
  name: z.string().trim().min(1).max(120),
  countryCode: country,
  currency,
  frequency: z.enum(["weekly", "fortnightly", "semimonthly", "monthly"]),
  timezone: z.string().min(1).max(80).default("UTC"),
  overtimeMultiplierBps:z.number().int().min(10000).max(50000).default(15000),
  prorationMethod:z.enum(["calendar_days","none"]).default("calendar_days"),
});
export const countryPackSchema = z.object({
  countryCode: country,
  version: z.string().trim().min(1).max(40),
  rules: z.record(z.string(), z.unknown()),
  exportFormats: z.array(z.string().max(80)).max(20).default([]),
});
export const countryPackTransitionSchema = z.object({
  expectedVersion: z.number().int().positive().default(1),
  status: z.enum(["validated", "active", "superseded"]),
  proof: z.string().trim().min(1).max(2000),
});
export const payrollRunSchema = z.object({
  payGroupId: z.string().min(1),
  countryPackId: z.string().nullable().optional(),
  periodStart: date,
  periodEnd: date,
  payDate: date,
  currency,
  mode: z.enum(["preview", "final_pay", "production"]).default("preview"),
});
export const paymentBatchSchema = z.object({
  sourceType: z.enum(["payroll", "expense", "contractor"]),
  sourceId: z.string().min(1),
  formatCode: z.string().trim().min(1).max(80).default("generic-csv"),
  bankAccountId: z.string().min(1).optional(),
});
export const timeEntrySchema = z.object({
  engagementId: z.string().min(1),
  workDate: date,
  regularHours: z.number().min(0).max(24),
  overtimeHours: z.number().min(0).max(24).default(0),
  note: z.string().max(500).default(""),
});
export const paymentConfirmationSchema = z.object({
  rows: z.array(z.object({
    reference: z.string().min(1),
    status: z.enum(["confirmed", "failed"]),
    confirmationRef: z.string().max(160).optional(),
    bankTransactionId: z.string().min(1).optional(),
    bankVersion: z.number().int().positive().optional(),
  })).min(1),
});
export const payrollPostSchema = z.object({
  chartId: z.string().min(1).optional(),
  periodId: z.string().min(1).optional(),
  entryDate: date.optional(),
  accountMappingId: z.string().min(1).optional(),
  expenseAccountId: z.string().min(1).optional(),
  netPayableAccountId: z.string().min(1).optional(),
  taxPayableAccountId: z.string().min(1).optional(),
  employerContributionAccountId: z.string().min(1).optional(),
  employerExpenseAccountId: z.string().min(1).optional(),
  socialSecurityPayableAccountId: z.string().min(1).optional(),
  advanceReceivableAccountId: z.string().min(1).optional(),
  benefitPayableAccountId: z.string().min(1).optional(),
  otherDeductionPayableAccountId: z.string().min(1).optional(),
  serviceChargeExpenseAccountId: z.string().min(1).optional(),
  overtimeExpenseAccountId: z.string().min(1).optional(),
  bonusExpenseAccountId: z.string().min(1).optional(),
  expectedVersion: z.number().int().positive(),
});
export const payrollAccountMappingSchema = payrollPostSchema.omit({periodId:true,entryDate:true,expectedVersion:true,accountMappingId:true}).required({chartId:true,expenseAccountId:true,netPayableAccountId:true});
export const payrollCorrectionRequestSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
  attachmentSetIds: z.array(z.string().min(1)).max(20).default([]),
});
export const payrollCorrectionDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  action: z.enum(["approve", "reject", "cancel"]),
  reason: z.string().trim().min(3).max(2000),
});
export const finalPayPreparationSchema = z.object({
  taskId: z.string().min(1),
  countryPackId: z.string().min(1).optional(),
  expectedTaskVersion: z.number().int().positive(),
  payGroupId: z.string().min(1),
  periodStart: date,
  periodEnd: date,
  payDate: date,
  currency,
  reason: z.string().trim().min(3).max(2000),
});
export const payrollAdvanceRequestSchema = z.object({
  engagementId: z.string().min(1),
  amount: z.string().regex(/^\d+(\.\d{1,2})?$/),
  installmentCount: z.number().int().min(1).max(24),
  firstDeductionPeriod: date,
  currency,
  reason: z.string().trim().min(3).max(2000),
});
export const payrollAdvanceDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  action: z.enum(["approve", "reject", "cancel"]),
  reason: z.string().trim().min(3).max(2000),
});
export const assignPayGroupSchema=z.object({payGroupId:z.string().nullable(),expectedVersion:z.number().int().positive()});
export const timeEntryUpdateSchema = timeEntrySchema.omit({engagementId:true}).extend({
  expectedVersion:z.number().int().positive(),
  attachmentSetIds:z.array(z.string().min(1)).max(20).default([]),
});
export const timeEntryReviewSchema = z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(500)});
export function advancePeriodStart(first:string,months:number){const value=new Date(`${first}T00:00:00Z`);value.setUTCMonth(value.getUTCMonth()+months);return value.toISOString().slice(0,10);}
export function advancePeriodEnd(first:string,months:number){const value=new Date(`${first}T00:00:00Z`);value.setUTCMonth(value.getUTCMonth()+months+1);value.setUTCDate(0);return value.toISOString().slice(0,10);}
export type ResolvedPayrollPost = z.infer<typeof payrollPostSchema> & {chartId:string;periodId:string;entryDate:string;expenseAccountId:string;netPayableAccountId:string;resolvedMappingId:string|null;resolvedMappingVersion:number|null};
