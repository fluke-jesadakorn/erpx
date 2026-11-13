import {BusinessRuleError} from "@erpx/server-shared/index";
import {assertCompanyTreasury,supplierBatchWorkItems,type BatchDecision,type BatchExport,type BatchInput,type InstructionConfirmation,type InstructionResult,type PreparedBatchItem,type SupplierPaymentRow as Row} from "@erpx/server-procurement/domain/supplier-payments";
type Command={key:string;operation:string;id:string;now:number};
/** Retained compatibility contract for the native supplier-batch command. */
export type SupplierPaymentMutation<Evidence,Journal>=Command&(
 | {kind:'create';input:BatchInput;bank:Row;items:PreparedBatchItem[]}
 | {kind:'decision';input:BatchDecision;batch:Row;decision:'approve'|'cancel'}
 | {kind:'export';input:BatchExport;batch:Row;sha256:string}
 | {kind:'result';input:InstructionResult;item:Row;batch:Row}
 | {kind:'confirmation';input:InstructionConfirmation;item:Row;batch:Row;invoice:Row;bank:Row;amount:number;evidence:Evidence|null;journal:Journal;journalId:string;paymentId:string}
);
export interface SupplierPaymentReadPorts{
 userId:string;branchId:string|null;departmentId:string|null;canRead:boolean;canWrite:boolean;canApprove:boolean;canPay:boolean;samePerson(id:unknown):boolean;
 authorizeRead():Promise<void>;
 batch(id:string):Promise<Row|null>;item(id:string):Promise<Row|null>;list(forWork:boolean):Promise<Row[]>;
 workspace(id:string|undefined,batch:Row|null):Promise<{items:Row[];invoices:Row[];accounts:Row[];periods:Row[];bank:Row[]}>;
}
export function createSupplierPaymentApplication(ports:SupplierPaymentReadPorts){
 const company=async()=>{await ports.authorizeRead();assertCompanyTreasury(ports.branchId,ports.departmentId);};
 const get=async(id:string)=>{await company();const batch=await ports.batch(id);if(!batch)throw new BusinessRuleError(404,'BATCH_NOT_FOUND','Supplier batch not found.');return batch;};
 const list=async(forWork=false)=>{await company();return ports.list(forWork);};
 return {
 get,list,
 async workspace(id?:string){await company();const batch=id?await get(id):null;return {batch,...await ports.workspace(id,batch),userId:ports.userId,canCreate:ports.canWrite,canApprove:ports.canApprove,canPay:ports.canPay};},
 async work(){if(!ports.canRead||ports.branchId||ports.departmentId)return [];return supplierBatchWorkItems(await list(true),ports);},
 };
}
