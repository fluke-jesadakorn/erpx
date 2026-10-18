import {z} from 'zod';
import {payGroupSchema} from './people-payroll';
export const hireHandoffPayGroupSchema=z.object({expectedOfferVersion:z.number().int().positive(),expectedCandidateVersion:z.number().int().positive(),input:payGroupSchema.strict()}).strict();
export type HireHandoffPayGroupInput=z.infer<typeof hireHandoffPayGroupSchema>;
export type HireHandoffPayGroupResult={id:string;handoffId:string;status:'active';currency:string};
