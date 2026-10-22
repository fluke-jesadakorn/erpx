import {workFetch,type WorkCommandCompletion} from "@erpx/feature-work_projection/lib/work-fetch";
export type AssetCommandPath="assets"|"maintenance"|"maintenance/plans";
export type AssetPurchase={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export type AssetPurchases=Partial<Record<string,AssetPurchase>>|undefined;
export type AssetCommandAttempt={path:AssetCommandPath;body:string;context:string;key:string;completion?:WorkCommandCompletion};
export function assetCommandContext(){const node=typeof document==="undefined"?null:document.querySelector<HTMLElement>("[data-work-revision]");return node?[node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment].join(":"):"server";}
export function assetFormUnchanged(completion:WorkCommandCompletion|undefined){return !completion||completion.form.isConnected&&completion.form.getAttribute("data-erpx-edit-version")===completion.formEditVersion;}
export function assetCommandAllowed(path:AssetCommandPath,input:{action?:string;id?:string},purchases:AssetPurchases){
 const capability=path==="assets"?(input.action==="place"?"assets.custody":"assets.register"):path==="maintenance/plans"?"assets.preventive":input.action==="issue-part"?"assets.parts-costs":"assets.maintenance",settlement=path==="assets"?!!input.id&&input.action!=="place":path==="maintenance/plans"?["pause","cancel"].includes(input.action??""):!!input.id;
 return purchases===undefined||purchases[capability]?.[settlement?"settleExisting":"newWork"]===true;
}
export function validAssetReceipt(attempt:AssetCommandAttempt,value:unknown):value is {id:string;version:number}{
 if(!value||typeof value!=="object"||Array.isArray(value)||!("id" in value)||typeof value.id!=="string"||!("version" in value)||!Number.isSafeInteger(value.version))return false;
 const input=JSON.parse(attempt.body) as {id?:string;expectedVersion?:number;action?:string},prefix=attempt.path==="assets"?"asset":attempt.path==="maintenance/plans"?"maintenance-plan":"maintenance";
 if(input.id?value.id!==input.id||value.version!==Number(input.expectedVersion)+1:value.id!==`${prefix}-${attempt.key}`||value.version!==1)return false;
 return !(attempt.path==="maintenance/plans"&&input.action==="generate")||"workOrderId" in value&&value.workOrderId===`maintenance-${attempt.key}:work`&&"nextDueAt" in value&&Number.isSafeInteger(value.nextDueAt)&&"nextDueLocal" in value&&typeof value.nextDueLocal==="string"&&value.nextDueLocal.length>0;
}
export function createAssetAttempts(){let pending:AssetCommandAttempt|null=null;return {select(next:AssetCommandAttempt,currentContext:string){if(next.context!==currentContext)throw Error("Your workspace changed. Return to the original Assets role before retrying.");if(pending&&(next.path!==pending.path||next.body!==pending.body||next.context!==pending.context))throw Error("Retry the submitted Asset or Maintenance change first. Your newer draft is retained.");return pending??next;},unresolved(value:AssetCommandAttempt){pending=value;},resolved(){pending=null;},retry(){return pending;}};}
export function sendAssetCommand(base:string,attempt:AssetCommandAttempt,send:typeof workFetch=workFetch){return send(`/api/v1${base}/${attempt.path}`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.key},body:attempt.body},attempt.completion,"caller");}
