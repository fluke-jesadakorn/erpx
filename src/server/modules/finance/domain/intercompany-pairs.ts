import {z} from 'zod';
import {decimalToUnits} from "@/lib/finance/finance-money";
const side = z.object({
  companyId: z.string().min(1),
  journalId: z.string().min(1),
  accountId: z.string().min(1),
});
export const intercompanyPairSchema = z
  .object({
    reference: z.string().trim().min(2).max(200),
    source: side,
    target: side,
    currency: z
      .string()
      .length(3)
      .transform((v) => v.toUpperCase()),
    amount: z
      .string()
      .regex(/^\d+(\.\d{1,2})?$/)
      .refine((v) => decimalToUnits(v) > BigInt(0)),
    reason: z.string().trim().min(3).max(2000),
  })
  .refine((v) => v.source.companyId !== v.target.companyId, {
    message: "Select two different Companies.",
  });
export const intercompanyPairDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});

export type PairFactRow=Record<string,string|number|null>;
export type PairEvidence={entity:PairFactRow;journal:PairFactRow;lines:PairFactRow[];net:string};
export type PairSource={evidence:PairEvidence[];hash:string};
