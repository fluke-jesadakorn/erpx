import {contextGeneration,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';

export type PortalPurchaseControls={settleExisting:boolean;history:boolean;revision:string;identityRevision:string};
export type PortalActionAttempt={body:string;key:string;context:string;uncertain:boolean};
export type PortalActionSource={context:string;image:string};
/** A lost public response retains its exact original body/key and verified
 * account context. No staff Work session is inferred for the customer. */
export function createPortalActionAttempt(nextKey:()=>string=()=>crypto.randomUUID(),options:{requireSource?:boolean}={}){
 let pending:PortalActionAttempt|null=null;
 let source:PortalActionSource|null=null,required=options.requireSource===true;
 const route=()=>typeof window!=='undefined'&&window.location?window.location.pathname+window.location.search:null;
 return {
  select(body:string,context:string){if(pending){if(pending.context!==context)throw Error('Return to the original verified customer account before retrying.');if(pending.body!==body)throw Error('Retry the submitted customer action first. Your newer message is retained.');return pending;}return {body,key:nextKey(),context,uncertain:false};},
  uncertain(attempt:PortalActionAttempt){attempt.uncertain=true;pending=attempt;},
  resolved(attempt:PortalActionAttempt){if(!pending||pending===attempt)pending=null;},
  retry(){return pending;},
  /** The document owner commits only its already-public identity/revisions.
   * Other original Portal callers retain their explicit legacy source guards. */
  attach(value:PortalActionSource){required=true;const committed=Object.freeze({...value});source=committed;return()=>{if(source===committed)source=null;};},
  retain(attempt:PortalActionAttempt){if(pending&&pending!==attempt)throw Error('Retry the submitted customer action first. Your newer message is retained.');pending=attempt;},
  capture(attempt:PortalActionAttempt){
   const captured=source,generation=contextGeneration(),href=route();
   const sameOwner=()=>required?source!==null&&source.context===attempt.context:true;
   const current=()=>sameOwner()&&!contextIsSwitching()&&contextGeneration()===generation&&(!required||captured!==null&&source?.image===captured.image&&href!==null&&route()===href);
   const assertCurrent=()=>{if(!current())throw Error('The customer document or verified account changed. Retry the original action in the current document.');};
   assertCurrent();return{current,sameOwner,assertCurrent};
  },
  /** Each code is emitted before record/evidence/notification effects by the
   * original document-action application, or by its request schema parser.
   * A prior uncertain send can never use this first-send rollback allowance. */
  firstRollback(attempt:PortalActionAttempt,status:number,code:unknown,action:'acknowledge_delivery'|'request_support'){
   if(attempt.uncertain)return false;
   if(status===422&&(code==='VALIDATION_FAILED'||code==='CUSTOMER_PORTAL_ATTACHMENT_ACTION')||status===409&&(code==='CUSTOMER_PORTAL_BINDING_CHANGED'||code==='CUSTOMER_PORTAL_SOURCE_UNAVAILABLE'||code==='CUSTOMER_PORTAL_ACTION_ALREADY_RECORDED'))return true;
   // Original prepareAttachmentBinding runs before repository.record and
   // before its returned guards/steps are committed by this owned command.
   return action==='request_support'&&typeof code==='string'&&(status===422&&['ATTACHMENT_SETS_REQUIRED','ATTACHMENT_SET_TARGET','ATTACHMENT_BINDING_CONTEXT','ATTACHMENT_SET_EMPTY','ATTACHMENT_REQUIRED','ATTACHMENT_SANITIZATION_ATTESTATION_REQUIRED'].includes(code)||status===409&&['ATTACHMENT_SET_LOCKED','ATTACHMENT_SET_STALE','ATTACHMENT_TARGET_CHANGED','ATTACHMENT_SCAN_PENDING'].includes(code));
  },
 };
}
export function validPortalActionReceipt(value:unknown,bindingId:string,action:string):value is {id:string;bindingId:string;orderId:string;action:string;status:'recorded';createdAt:number}{
 if(!value||typeof value!=='object'||Array.isArray(value))return false;const row=value as Record<string,unknown>;return typeof row.id==='string'&&row.id.length>0&&row.bindingId===bindingId&&typeof row.orderId==='string'&&row.orderId.length>0&&row.action===action&&row.status==='recorded'&&typeof row.createdAt==='number'&&Number.isSafeInteger(row.createdAt)&&row.createdAt>=0;
}
