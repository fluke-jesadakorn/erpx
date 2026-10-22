import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {privateFileDenialAuthoritySourceLocks} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {attachmentPrivateReadSourceLocks} from '@erpx/server-files/public/private-attachment-reads';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';

export type FinanceSignatureCommand={key:string;operation:'finance.tax-identity.approve'|'finance.payment.confirm'|'finance.thai-document.change.approve'|'finance.tax-signature.read';signatureRecordId?:string;signatureSource?:'supplier-correction'};
type Unit={companyId:string;userId:string;actorWitness:string;command:FinanceSignatureCommand;preparing:boolean;checks:(()=>Promise<()=>Promise<void>>)[]};
const units=new WeakMap<SqlDatabase,Unit>();
const actorWitness=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.email,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.assignmentScope,actor.scope,actor.modules,actor.authority??null,actor.iam??null]);
export function isFinanceSignatureOperation(operation:string):operation is FinanceSignatureCommand['operation']{
 return ['finance.tax-identity.approve','finance.payment.confirm','finance.thai-document.change.approve','finance.tax-signature.read'].includes(operation);
}
export function financeSignatureParent(database:SqlDatabase,actor:WorkContext){
 const unit=units.get(database);return unit&&!unit.preparing&&unit.companyId===actor.companyId&&unit.userId===actor.userId&&unit.actorWitness===actorWitness(actor)?unit.command:null;
}
export function requireFinanceSignatureParent(database:SqlDatabase,actor:WorkContext,command:Pick<FinanceSignatureCommand,'key'|'operation'>){
 const parent=financeSignatureParent(database,actor);if(!parent||parent.key!==command.key||parent.operation!==command.operation)throw new ApiError(503,'FINANCE_SIGNATURE_PARENT_REQUIRED','Signature bytes require their original complete Finance command.');
}
export function deferFinanceSignatureRead(database:SqlDatabase,actor:WorkContext,prepare:()=>Promise<()=>Promise<void>>){
 if(!financeSignatureParent(database,actor))throw new ApiError(503,'FINANCE_SIGNATURE_PARENT_REQUIRED','Signature checks require their original complete Finance command.');
 units.get(database)!.checks.push(prepare);
}
/** Only actual closed Finance constructors call this before their first data
 * statement. No arbitrary supplied transaction can mint or inherit the unit.
 * Existing source-specific Service/Supplier constructors still own all of
 * their original money/source guards; this adds no purchased capability. */
export async function inFinanceSignatureParent<Result>(owner:SqlDatabase,actor:WorkContext,command:FinanceSignatureCommand,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'FINANCE_SIGNATURE_FRESH_PARENT_REQUIRED','Signature bytes require a fresh complete Finance transaction.');
 return owner.transaction(async database=>{
  const unit:Unit={companyId:actor.companyId,userId:actor.userId,actorWitness:actorWitness(actor),command,preparing:true,checks:[]};units.set(database,unit);
  try{
   // File binding writes metadata before its Tax identity version. Keep that
   // order, using writer-compatible leases across separate Companies.
   const sourceLeases=command.operation==='finance.payment.confirm'?(await import('@erpx/server-finance/adapters/document-purchases')).financeSourceDocumentReadLeases(database,actor.companyId,true,command.signatureSource):[companyPurchaseReadLocks(database,actor.companyId)[0]];
   await database.batch([...sourceLeases,attachmentPrivateReadSourceLocks(database),database.prepare('LOCK TABLE finance_tax_identities IN SHARE ROW EXCLUSIVE MODE'),...privateFileDenialAuthoritySourceLocks(database)]);
   await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);unit.preparing=false;
   const result=await run(database);
   if(unit.checks.length){
    if(unit.actorWitness!==actorWitness(actor))throw new ApiError(409,'FINANCE_SIGNATURE_CONTEXT_CHANGED','The selected Finance context changed during its command.');
    const finalChecks:(()=>Promise<void>)[]=[];for(const prepare of unit.checks)finalChecks.push(await prepare());
    // Read children never flush the parent halfway through an approval, bank
    // settlement, certificate or amendment. Materialize whole-owner effects.
    await flushFinanceDeferredEffects(database);
    if(command.operation!=='finance.tax-signature.read')await requireQueuedCommandAuthority(database,actor,command.operation==='finance.payment.confirm'?'finance.pay':'finance.approve');
    for(const check of finalChecks)await check();
   }
   return result;
  }finally{units.delete(database);}
 });
}
