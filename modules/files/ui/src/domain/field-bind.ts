export type BindAttempt={scope:string;base:string;setId:string;expectedVersion:number;key:string;identity:string;generation:number;busy:boolean};
export type BindResult={setId:string;version:number;manifest:{linkId:string;sourceId:string;purposeKey:string;checksumSha256:string;mediaType:string;bytes:number}[]};
export function confirmsBind(v:unknown,a:BindAttempt):v is BindResult {
 const object=(x:unknown):x is Record<string,unknown>=>!!x&&typeof x==='object'&&!Array.isArray(x),text=(x:unknown):x is string=>typeof x==='string'&&x.length>0;
 if(!object(v)||Object.keys(v).length!==3||v.setId!==a.setId||v.version!==a.expectedVersion+1||!Array.isArray(v.manifest)||!v.manifest.length||v.manifest.length>100)return false;
 const ids=new Set<string>();return v.manifest.every(item=>{if(!object(item)||Object.keys(item).length!==6||!text(item.linkId)||!text(item.sourceId)||!text(item.purposeKey)||typeof item.checksumSha256!=='string'||!/^[a-f0-9]{64}$/.test(item.checksumSha256)||!text(item.mediaType)||!Number.isSafeInteger(item.bytes)||Number(item.bytes)<=0||ids.has(item.linkId))return false;ids.add(item.linkId);return true;});
}
