const record=(value:unknown):Record<string,unknown>|null=>value!==null&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:null;
/** Match the original native Legal receipt shapes. Preview/import and queued
 * inspection are explicit owner responses, not generic mutation success. */
export function legalCommandAcknowledged(path:string,input:unknown,value:unknown,strict=false){
 const request=record(input),data=record(value),action=String(request?.action??'');if(!data)return false;
 if(action==='import.preview')return Array.isArray(data.rows)&&Array.isArray(data.errors)&&typeof data.digest==='string'&&Number.isSafeInteger(data.count)&&data.count===data.rows.length+data.errors.length&&(!Array.isArray(request?.rows)||data.count===request.rows.length);
 if(action==='import.commit')return Array.isArray(request?.rows)&&Array.isArray(data.items)&&data.items.length===request.rows.length&&data.items.every(item=>{const receipt=record(item);return typeof receipt?.id==='string'&&Boolean(receipt.id)&&Number.isSafeInteger(receipt.version)&&Number(receipt.version)>0;});
 if(typeof data.id!=='string'||!data.id)return false;
 if(/\/documents\/[^/]+\/(ocr|verify)$/.test(path))return typeof data.jobId==='string'&&Boolean(data.jobId)&&data.state==='queued';
 const originalRecord=['update','document','transition','approve','member','comment','link','assign','share','share.revoke','reference.add','source.attach','template.apply','ai.apply','finance.prepare','executed.review'].includes(action)||action.startsWith('hold.')||action.startsWith('feature.')||action.startsWith('approval.')||action.startsWith('law.')||action.startsWith('knowledge.document.')||action.startsWith('retention.')&&!['retention.policy','retention.approve-policy','retention.retire-policy'].includes(action);
 if(originalRecord)return data.id===request?.id&&Number.isSafeInteger(request?.expectedVersion)&&data.version===Number(request?.expectedVersion)+1;
 if(action==='ai.generate')return data.id===request?.id&&data.version===request?.expectedVersion;
 if(['create','revision.create','request.convert'].includes(action)&&strict)return data.version===1;
 return !strict||Number.isSafeInteger(data.version)&&Number(data.version)>0;
}
