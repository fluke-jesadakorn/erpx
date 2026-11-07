import {contextGeneration,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {createFieldWithdrawal} from '../application/field-withdrawal';
import {withdrawAttachmentLink} from './attachment-client';

const boundary=()=>typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
const validBoundary=(n:HTMLElement)=>[n.dataset.workUser,n.dataset.workRevision,n.dataset.workCompany,n.dataset.workAssignment,n.dataset.workAssignmentVersion].every(value=>typeof value==='string'&&value.length>0);
const identity=(n:HTMLElement)=>JSON.stringify([n.dataset.workUser,n.dataset.workRevision,n.dataset.workCompany,n.dataset.workAssignment,n.dataset.workAssignmentVersion]);
const withdrawal=createFieldWithdrawal({
 context:()=>{const node=boundary();return node?{identity:identity(node),generation:contextGeneration(),valid:validBoundary(node)&&node.isConnected&&node.dataset.workStale!=='true'&&!contextIsSwitching()}:null;},
 key:()=>crypto.randomUUID(),
 withdraw:attempt=>withdrawAttachmentLink(attempt.base,attempt.linkId,attempt.reason,attempt.key,'caller'),
 acknowledge:()=>{window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form:null,formEditVersion:null}}));announceWorkChange();},
});
export const registerFieldWithdrawalOwner=withdrawal.register;
export function fieldWithdrawalScope(base:string,targetType:string,targetId:string,componentKind:string,componentRef:string|null,purposeKey:string){const node=boundary();return JSON.stringify([base,targetType,targetId,componentKind,componentRef,purposeKey,node?identity(node):null]);}
export const pendingFieldWithdrawal=withdrawal.pending;
export const subscribeFieldWithdrawal=withdrawal.subscribe;
export const submitFieldWithdrawal=withdrawal.submit;
export const retryFieldWithdrawal=withdrawal.retry;
