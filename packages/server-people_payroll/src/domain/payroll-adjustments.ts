import {z} from 'zod';export type Row=Record<string,unknown>;
export const payrollAdjustmentValuesSchema=z.object({
  taxableArrearsMinor:z.number().int().nonnegative().safe(), nonTaxableArrearsMinor:z.number().int().nonnegative().safe().default(0),
  socialSecurityWageMinor:z.number().int().nonnegative().safe().default(0),
  timeEntryIds:z.array(z.string().min(1)).max(100).default([]),
  reason:z.string().trim().min(10).max(2000), evidenceRef:z.string().trim().min(8).max(2000),
}).strict().refine(input=>input.socialSecurityWageMinor<=input.taxableArrearsMinor,{message:"Contribution wages cannot exceed taxable arrears."});
export const payrollAdjustmentCreateSchema=z.object({expectedVersion:z.number().int().positive(),originalItemId:z.string().min(1),countryPackId:z.string().min(1),payDate:z.string().date(),sourceRef:z.string().trim().min(8).max(200),values:payrollAdjustmentValuesSchema}).strict();
export const payrollAdjustmentEditSchema=z.object({expectedVersion:z.number().int().positive(),values:payrollAdjustmentValuesSchema}).strict();
export type Values=z.infer<typeof payrollAdjustmentValuesSchema>;
export type Origin={run:Row;item:Row;slips:Row[]};
export const normalize=(value:unknown):unknown=>Array.isArray(value)?value.map(normalize):value&&typeof value==="object"?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([key,entry])=>[key,normalize(entry)])):value;
export const equal=(a:unknown,b:unknown)=>JSON.stringify(normalize(a))===JSON.stringify(normalize(b));
