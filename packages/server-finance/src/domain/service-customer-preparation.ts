import {z} from 'zod';
import {partySchema} from '@erpx/server-finance/domain/finance';

/** This is the original customer input, never a general party selector. */
export const serviceCustomerSchema=z.object({expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/),input:partySchema.safeExtend({kind:z.literal('customer')}).strict()}).strict();
export type ServiceCustomerInput=z.infer<typeof serviceCustomerSchema>;
export type ServiceCustomerResult=ServiceCustomerInput['input']&{id:string;creditLimitMinor:number};
export type ServiceCustomerPreparation={companyId:string;available:boolean;contextKey:string|null;canCreate:boolean;currencies:{code:string;name:string}[];customers:{id:string;code:string;name:string;currency:string}[];unavailableReason:'authority-required'|'purchase-required'|null};
/** Current status is distinct from the immutable original creation receipt. */
export type ServiceCustomerConfirmation={commandId:string;companyId:string;id:string;kind:'customer';status:'active'|'blocked';code:string;name:string;currency:string};
