/** Pure native request validation only; no authority, owner or runtime imports. */
import type {z} from 'zod';
import {hireConsumptionSchema} from '@erpx/people-payroll-server/domain/people-handoffs';
export {hireConsumptionSchema};
export type HireConsumptionInput=z.infer<typeof hireConsumptionSchema>;
