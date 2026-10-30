// Test-only retained schema oracle; never imported by product/runtime code.
// Exact original source: packages/server-finance/src/domain/service-parts-prerequisites.ts
// Original SHA-256: 704ee8619e6bd82c4ae5dd040e98f1cfa4f25edc1bfcab96fee7cb91dade3daa
import {z} from 'zod';
import {createChartSchema,createAccountSchema,createPeriodSchema} from '@erpx/server-finance/domain/finance';
import {manualRateSchema} from '@erpx/server-finance/domain/company-money';
import type {SupplyPostingPrerequisitesView} from '@erpx/server-finance/domain/supply-posting-prerequisites';
const initial={expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/)};
export const servicePartsChartSchema=z.object({...initial,input:createChartSchema.strict()}).strict();
export const servicePartsAccountSchema=z.object({...initial,input:createAccountSchema.strict()}).strict();
export const servicePartsPeriodSchema=z.object({...initial,input:createPeriodSchema.strict()}).strict();
export const servicePartsRateSchema=z.object({...initial,input:manualRateSchema.strict()}).strict();
export type ServicePartsChartInput=z.infer<typeof servicePartsChartSchema>;
export type ServicePartsAccountInput=z.infer<typeof servicePartsAccountSchema>;
export type ServicePartsPeriodInput=z.infer<typeof servicePartsPeriodSchema>;
export type ServicePartsRateInput=z.infer<typeof servicePartsRateSchema>;
export type ServicePartsPrerequisitesView=SupplyPostingPrerequisitesView;
