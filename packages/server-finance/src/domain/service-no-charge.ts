import {z} from 'zod';
export const serviceNoChargeSchema=z.object({expectedVersion:z.number().int().positive(),contextKey:z.string().regex(/^[a-f0-9]{64}$/),reason:z.string().trim().min(10).max(3000)});
export type ServiceNoChargeInput=z.infer<typeof serviceNoChargeSchema>;
export type ServiceNoChargeResult={id:string;status:'no_charge_confirmed';version:number;confirmationId:string};
export type ServiceNoChargeControls={invoiceId:string;version:number;contextKey:string;canConfirm:boolean;confirmed:boolean};
export function serviceNoChargeAcknowledged(value:unknown,invoiceId:string,version:number,key:string):value is ServiceNoChargeResult{
 if(!value||typeof value!=='object')return false;const row=value as Record<string,unknown>;
 return row.id===invoiceId&&row.status==='no_charge_confirmed'&&row.version===version+1&&row.confirmationId===`service-no-charge-${key}`;
}

export type ServiceNoChargeAttempt={readonly invoiceId:string;readonly version:number;readonly key:string;readonly scope:string;readonly body:string};
export function captureServiceNoChargeAttempt(controls:Pick<ServiceNoChargeControls,'invoiceId'|'version'|'contextKey'>,reason:string,key:string,scope:string):ServiceNoChargeAttempt{
 return Object.freeze({invoiceId:controls.invoiceId,version:controls.version,key,scope,body:JSON.stringify({expectedVersion:controls.version,contextKey:controls.contextKey,reason:reason.trim()})});
}

/** A confirmed receipt can coexist with edits made after submission. Keep that
 * draft reviewable until its owner explicitly discards it. */
export function serviceNoChargeView(controls:Pick<ServiceNoChargeControls,'canConfirm'|'confirmed'>,draft:{reason:string;notice:string;uncertain:boolean}){
 return {showReview:!controls.confirmed||draft.uncertain||draft.reason.length>0||draft.notice.length>0,canSubmit:draft.uncertain||(!controls.confirmed&&controls.canConfirm)};
}
