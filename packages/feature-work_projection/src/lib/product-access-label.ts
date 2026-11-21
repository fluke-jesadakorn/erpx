import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
/** Entry purpose controls admission; source work still has its own action policy. */
export function productAccessLabel(access:{purchasePurpose:CapabilityPurpose;sourceRecord?:boolean},language:'th'|'en'):string|undefined{
 if(access.sourceRecord)return language==='th'?'เฉพาะงานที่ได้รับสิทธิ์':'Authorized work';
 if(access.purchasePurpose==='new-work')return undefined;
 return access.purchasePurpose==='history'?(language==='th'?'ประวัติเท่านั้น':'History only'):(language==='th'?'งานเดิมและประวัติ':'Existing work and history');
}
