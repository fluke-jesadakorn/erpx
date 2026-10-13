import { canonicalGuestAccount, legacyGuestAccount, legacyGuestAccountOperation } from "@/lib/identity_access/guest-account-compat";
import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';
import {financialConstraintError,type CommandActor} from '../domain/commands';
export interface CommandRepository<Effect> {
  receipt(id:string):Promise<{request_hash:string;result_json:string;operation?:string;actor_id?:string;record_id?:string;authority_json?:string|null}|null>;
  clearAssertions(id:string):Effect;
  insertReceipt(actor:CommandActor,id:string,operation:string,digest:string,recordId:string,result:unknown,now:number):Effect;
  captureAssignmentAuthority(actor:CommandActor,id:string):Effect;
  audit(actor:CommandActor,id:string,operation:string,recordId:string,version:number,now:number,eventId:string):Effect;
  outbox(actor:CommandActor,id:string,operation:string,recordId:string,version:number,state:unknown,scope:Pick<CommandActor,'companyId'|'branchId'|'departmentId'>,now:number,eventId:string):Effect;
}
export interface CommandPorts<Effect> {
  repository:CommandRepository<Effect>;
  clock:Clock; identifiers:IdentifierSource;
  digest(value:unknown):Promise<string>;
  workContextGuards(actor:CommandActor,id:string):Effect[];
  iamMutationGuards(actor:CommandActor,id:string):Effect[];
  assignmentGuard(actor:CommandActor,id:string):Effect;
  prepareOwnerStatements(actor:CommandActor,id:string,operation:string,input:unknown,recordId:string,version:number,effects:Effect[]):Promise<Effect[]>;
  batch(effects:Effect[]):Promise<unknown>;
  activeContext(actor:CommandActor):Promise<{revision:number;assignment_id:string;assignment_version:number}|null>;
  activeGrant(actor:CommandActor,now:number):Promise<unknown>;
}
export function createCommandApplication<Effect>(ports:CommandPorts<Effect>) {
  const repository=ports.repository;
  function fingerprint(actor:CommandActor,operation:string,recordId:string,input:unknown){return ports.digest({actor:actor.userId,assignment:actor.assignmentId,role:actor.role,authority:actor.authority,company:actor.companyId,branch:actor.branchId,department:actor.departmentId,operation,recordId,input});}
  async function validateReplayAuthority(actor:CommandActor){
    if(!actor.authority&&actor.iam?.queuedAssignmentVersion===undefined)return;
    const key=ports.identifiers.next();
    try{await ports.batch([...ports.workContextGuards(actor,key),...ports.iamMutationGuards(actor,key),repository.clearAssertions(key)]);}
    catch(error){if((error as {code?:string}).code==='23514')throw new BusinessRuleError(409,'WORK_CONTEXT_CHANGED','Your access changed. Reload and review before retrying.');throw error;}
  }
  async function replayCommand<Result=Record<string,unknown>>(actor:CommandActor,id:string,operation:string,recordId:string,input:unknown):Promise<Result|null>{
    const row=await repository.receipt(id);if(!row)return null;
    const historicalOperation=legacyGuestAccountOperation(operation),historicalInput=legacyGuestAccount(input);
    let matches=row.request_hash===await fingerprint(actor,operation,recordId,input) || row.request_hash===await fingerprint(actor,historicalOperation,recordId,historicalInput);
    // A terminology migration invalidates active context revisions. Reconstruct only
    // the saved fingerprint of a legacy receipt; current authority is checked below.
    if(!matches && historicalOperation!==operation && row.operation===historicalOperation && row.actor_id===actor.userId && row.record_id===recordId && row.authority_json && actor.authority) {
      const saved=JSON.parse(row.authority_json) as Record<string,unknown>;
      if(saved.role===actor.role && saved.assignmentId===actor.assignmentId && saved.companyId===actor.companyId && saved.branchId===actor.branchId && saved.departmentId===actor.departmentId) {
        const keys=Object.keys(actor.authority) as Array<keyof NonNullable<CommandActor['authority']>>;
        if(keys.every(key=>typeof saved[key]===typeof actor.authority![key])) {
          const authority=Object.fromEntries(keys.map(key=>[key,saved[key]])) as NonNullable<CommandActor['authority']>;
          matches=row.request_hash===await fingerprint({...actor,authority},historicalOperation,recordId,historicalInput);
        }
      }
    }
    if(!matches)throw new BusinessRuleError(409,'IDEMPOTENCY_CONFLICT','This command key belongs to a different request.');
    await validateReplayAuthority(actor);return canonicalGuestAccount(JSON.parse(row.result_json)) as Result;
  }
  async function executeCommand<Result>(actor:CommandActor,id:string,operation:string,input:unknown,recordId:string,version:number,result:Result,effects:Effect[],eventScope:Pick<CommandActor,'companyId'|'branchId'|'departmentId'>=actor):Promise<Result>{
    if(!/^[a-zA-Z0-9:_.-]{8,180}$/.test(id))throw new BusinessRuleError(422,'COMMAND_KEY_REQUIRED','Use a valid Idempotency-Key for this action.');
    const digest=await fingerprint(actor,operation,recordId,input),prior=()=>replayCommand<Result>(actor,id,operation,recordId,input);
    const existing=await prior();if(existing)return existing;
    effects=await ports.prepareOwnerStatements(actor,id,operation,input,recordId,version,effects);
    const now=ports.clock.now(),state=typeof result==='object'&&result!==null?((result as Record<string,unknown>).state??(result as Record<string,unknown>).status):undefined;
    try{
      await ports.batch([...ports.workContextGuards(actor,id),...ports.iamMutationGuards(actor,id),repository.insertReceipt(actor,id,operation,digest,recordId,result,now),ports.assignmentGuard(actor,id),repository.captureAssignmentAuthority(actor,id),...effects,repository.audit(actor,id,operation,recordId,version,now,ports.identifiers.next()),repository.outbox(actor,id,operation,recordId,version,state,eventScope,now,ports.identifiers.next()),repository.clearAssertions(id)]);
      return result;
    }catch(error){
      // The owning transaction retries the whole handoff for these failures.
      if(['40001','40P01'].includes((error as {code?:string}).code??''))throw error;
      try{
        if(actor.authority){
          const active=await ports.activeContext(actor);
          if(!active||active.revision!==actor.authority.revision||active.assignment_id!==actor.assignmentId||active.assignment_version!==actor.authority.assignmentVersion)throw new BusinessRuleError(409,'WORK_CONTEXT_CHANGED','Your work role changed. Reload and review before submitting.');
          if(!await ports.activeGrant(actor,ports.clock.now()))throw new BusinessRuleError(409,'WORK_ROLE_UNAVAILABLE','Your selected assignment is no longer valid. Choose a work role again.');
        }
        const committed=await prior();if(committed)return committed;
      }catch(inspectionError){if((inspectionError as {code?:string}).code!=='25P02')throw inspectionError;}
      const financeFailure=financialConstraintError(error);if(financeFailure)throw financeFailure;
      if(/constraint|UNIQUE/i.test(String(error))){const conflict=new BusinessRuleError(409,'CONFLICT','The record changed, a reference is unavailable, or this value already exists. Refresh and review again.');conflict.cause=error;throw conflict;}
      throw error;
    }
  }
  return {executeCommand,replayCommand};
}
