import type {BenefitAttempt,BenefitKind} from './command';
export type BenefitPurchase={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
type Row=Record<string,string|number|null>;
export type BenefitPresentation={actorId:string;canWrite:boolean;canReview:boolean;purchase?:BenefitPurchase;plans?:Row[];windows?:Row[];dependents?:Row[];election?:Row;isOwner?:boolean};
const transitions:Record<BenefitKind,Record<string,string>>={plan:{publish:'draft',reject:'draft',retire:'published'},window:{open:'draft',close:'open',cancel:'draft'},dependent:{verify:'pending',reject:'pending',remove:'verified'},election:{submit:'elected',acknowledge:'submitted',release:'acknowledged',cancel:'elected'}};
export function benefitOriginalRole(view:BenefitPresentation,input:Readonly<Record<string,unknown>>){return view.canWrite&&(input.entity==='dependent'&&typeof input.id!=='string'||input.entity==='election'&&(typeof input.id!=='string'||input.action==='submit')||view.canReview);}
export function benefitActionAllowed(view:BenefitPresentation,input:Readonly<Record<string,unknown>>){
 if(!benefitOriginalRole(view,input))return false;const kind=input.entity;if(kind!=='plan'&&kind!=='window'&&kind!=='dependent'&&kind!=='election')return false;
 const newWork=typeof input.id!=='string'||input.action==='publish'||input.action==='open';if(view.purchase&&(newWork?!view.purchase.newWork:!view.purchase.settleExisting))return false;
 if(typeof input.id!=='string')return true;
 const row=kind==='election'?view.election:[...(view.plans??[]),...(view.windows??[]),...(view.dependents??[])].find(value=>value.id===input.id);
 if(!row||row.id!==input.id||row.status!==transitions[kind][String(input.action)]||Number(row.version)!==Number(input.expectedVersion))return false;
 if(['publish','reject','open','verify'].includes(String(input.action))&&row.created_by===view.actorId)return false;
 if(kind==='election'&&input.action==='submit'&&!view.isOwner)return false;
 return input.action!=='release'||row.acknowledged_by!==view.actorId;
}
export function benefitRetryAllowed(view:BenefitPresentation,attempt:BenefitAttempt){return (!view.purchase||view.purchase.history)&&benefitOriginalRole(view,attempt.input);}
