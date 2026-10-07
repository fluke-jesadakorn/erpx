import {z} from "zod";
const date=z.string().date();
export const createAgreementSchema = z.object({
  branchId: z.string().min(1),
  departmentId: z.string().nullable().optional(),
  customerId: z.string().min(1),
  assetId: z.string().nullable().optional(),
  title: z.string().trim().min(3).max(160),
  startDate: date,
  endDate: date,
  currency: z
    .string()
    .length(3)
    .transform((x) => x.toUpperCase()),
  visitRateMinor: z.number().int().nonnegative(),
  warrantyDiscountBps: z.number().int().min(0).max(10000).default(0),
});

export type AgreementInput=z.infer<typeof createAgreementSchema>;
