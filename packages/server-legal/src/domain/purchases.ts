import type {LegalKind} from '@erpx/server-legal/domain/model';
export type LegalPurchaseCapability='legal.contracts'|'legal.documents'|'legal.matters'|'legal.obligations'|'legal.access';
export type LegalPurchaseControls=Partial<Record<LegalPurchaseCapability,{newWork:boolean;settleExisting:boolean;history:boolean;revision:string}>>;
export function legalKindCapability(kind:LegalKind):LegalPurchaseCapability{return ['contract','template'].includes(kind)?'legal.contracts':['request','matter','counsel','ip'].includes(kind)?'legal.matters':'legal.obligations';}
export function legalCorePermission(role:string,action:string,state?:unknown){return role==='legal_requester'&&action==='transition'&&state==='review'?'legal.edit':action==='hold.acknowledge'?'legal.read':action==='transition'?state==='approved'?'legal.approve':['review','approval','rejected'].includes(String(state))?'legal.review':'legal.edit':action.startsWith('hold.')?action==='hold.release'?'legal.hold.release':'legal.hold':action==='member'||action==='share'||action==='share.revoke'?'legal.share':action==='approve'?'legal.approve':'legal.edit';}

export function legalActionPurchasePurpose(action:string,enabled?:unknown){if(action.startsWith('hold.')||action.startsWith('retention.')&&action!=='retention.policy')return 'history';return action==='settings'&&enabled===true||['create','update','document','share','link','assign','source.attach','template.apply','request.convert','revision.create','finance.prepare','ai.apply','import.preview','import.commit','ai.queue'].includes(action)?'newWork':'settleExisting';}
export function legalActionPurchased(purchases:LegalPurchaseControls|undefined,kind:LegalKind,action:string,enabled?:unknown){
 if(!purchases)return true;
 const purpose=legalActionPurchasePurpose(action,enabled),extra:LegalPurchaseCapability[]=['document','approve'].includes(action)||['source.attach','template.apply','ai.apply','ai.queue'].includes(action)||action.startsWith('approval.')?['legal.documents']:['member','share','share.revoke'].includes(action)?['legal.access']:action.startsWith('hold.')?['legal.obligations']:[];
 return [...(action==='settings'?[]:[legalKindCapability(kind)]),...extra,...(action==='settings'?['legal.access' as const]:[])].every(capability=>purchases[capability]?.[purpose]===true);
}
