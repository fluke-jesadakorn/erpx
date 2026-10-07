import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import {createFinancePolicy} from '../domain/finance-policy';
import type {FinanceError} from '../domain/invoice-line-revision';
export function createFinanceResourcePolicy(ports:{error:FinanceError;hasPermission(actor:WorkContext,permission:string):boolean}) {
 const {requireFinanceRole}=createFinancePolicy(ports.error);
 return (actor:WorkContext,readOnly=false)=>{requireFinanceRole(actor);if(!ports.hasPermission(actor,'finance.write')&&!readOnly)throw ports.error(403,'READ_ONLY_ROLE','Auditors have read-only access to finance.');};
}
export function createFinancePrior<Result=Record<string,unknown>>(ports:{error:FinanceError;receiptRecordId(key:string):Promise<string|null>;legacyCommandExists(key:string):Promise<boolean>;replay(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>}){
 return async(actor:WorkContext,key:string,operation:string,input:unknown)=>{const recordId=await ports.receiptRecordId(key);if(recordId!==null)return ports.replay(actor,key,operation,recordId,input);if(await ports.legacyCommandExists(key))throw ports.error(409,'IDEMPOTENCY_CONFLICT','This legacy command has no recorded work context. Review its result before starting a new command.');return null;};
}


