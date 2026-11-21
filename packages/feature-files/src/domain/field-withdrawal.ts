export type WithdrawalAttempt = {scope:string;base:string;linkId:string;setId:string;reason:string;draftReason:string;key:string;generation:number;identity:string;busy:boolean};
export type WithdrawalStatus = {
 linkId:string;setId:string;sourceId:string|null;sourceState:"legacy_unscanned"|"quarantined"|"scanning"|"available"|"scan_error"|"malware_rejected"|"destroyed"|null;linkState:"draft"|"bound"|"withdrawn"|"expired";
 scan:{attempts:number;lastResult:string|null;lastError:string|null;signature:string|null;definitionUpdatedAt:number|null;scannedAt:number|null};
 previewAvailable:boolean;bindable:boolean;expiresAt:number;boundAt:number|null;withdrawnAt:number|null;
};
export function confirmsWithdrawal(value:unknown,attempt:WithdrawalAttempt):value is WithdrawalStatus {
 return fullStatus(value)&&value.linkId===attempt.linkId&&value.setId===attempt.setId&&value.linkState==='withdrawn'&&!value.bindable&&typeof value.withdrawnAt==='number'&&Number.isSafeInteger(value.withdrawnAt);
}
export function fullStatus(value:unknown):value is WithdrawalStatus {
 const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v),exact=(v:Record<string,unknown>,keys:string[])=>Object.keys(v).length===keys.length&&keys.every(k=>Object.hasOwn(v,k)),nullableString=(v:unknown)=>v===null||typeof v==='string',nullableInt=(v:unknown)=>v===null||typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
 if(!object(value)||!exact(value,['linkId','setId','sourceId','sourceState','linkState','scan','previewAvailable','bindable','expiresAt','boundAt','withdrawnAt'])||!object(value.scan)||!exact(value.scan,['attempts','lastResult','lastError','signature','definitionUpdatedAt','scannedAt']))return false;
 const scan=value.scan;return typeof value.linkId==='string'&&typeof value.setId==='string'&&nullableString(value.sourceId)&& (value.sourceState===null||['legacy_unscanned','quarantined','scanning','available','scan_error','malware_rejected','destroyed'].includes(String(value.sourceState)))&&['draft','bound','withdrawn','expired'].includes(String(value.linkState))&&typeof scan.attempts==='number'&&Number.isSafeInteger(scan.attempts)&&scan.attempts>=0&&nullableString(scan.lastResult)&&nullableString(scan.lastError)&&nullableString(scan.signature)&&nullableInt(scan.definitionUpdatedAt)&&nullableInt(scan.scannedAt)&&typeof value.previewAvailable==='boolean'&&typeof value.bindable==='boolean'&&typeof value.expiresAt==='number'&&Number.isSafeInteger(value.expiresAt)&&value.expiresAt>0&&nullableInt(value.boundAt)&&nullableInt(value.withdrawnAt);
}
