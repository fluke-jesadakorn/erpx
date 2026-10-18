import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {commerceCommandContext,commerceFormUnchanged} from '@/lib/commerce/commerce-command-fetch';
import {salesServiceProductSchema,type SalesServiceProductInput,type SalesServiceProductResult} from '@/server/modules/commerce/public/rules/sales-service-products';
export type SalesServiceProductSource={base:string;companyId:string;contextKey:string|null};
export type SalesServiceProductDraft={sku:string;name:string;unit:string;price:string;taxPercent:string};
export const emptySalesServiceProductDraft=():SalesServiceProductDraft=>({sku:'',name:'',unit:'each',price:'',taxPercent:'0'});
export function salesServiceProductInput(draft:SalesServiceProductDraft):SalesServiceProductInput{
 if(!/^\d+(?:\.\d{1,2})?$/.test(draft.taxPercent))throw Error('Use a tax percentage with at most two decimal places.');
 const [whole,fraction='']=draft.taxPercent.split('.'),taxBps=Number(whole)*100+Number(fraction.padEnd(2,'0'));
 return salesServiceProductSchema.parse({itemType:'service',trackingMode:'none',sku:draft.sku,name:draft.name,unit:draft.unit,price:draft.price,taxBps,status:'active'});
}
const same=(a:SalesServiceProductSource,b:SalesServiceProductSource)=>a.base===b.base&&a.companyId===b.companyId;
type Attempt={source:SalesServiceProductSource;work:string;input:SalesServiceProductInput;body:string;key:string;uncertain:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>;hadCompletion:boolean};
/** A later draft never replaces the first uncertain request or its edit revision. */
export function createSalesServiceProductCommands(current:()=>SalesServiceProductSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original service item request.');
  const before=current();if(!before||!same(before,captured.source)||commerceCommandContext()!==captured.work)throw Error('Return to the original Company and work role to confirm the service item.');
  const source={...before},assertWork=captureWorkCommandContext(),wasUncertain=captured.uncertain;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||!same(source,live)||source.contextKey!==live.contextKey||commerceCommandContext()!==captured.work)throw Object.assign(Error('Your Sales context changed. The original request and newer draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  busy=true;
  try{
   const response=await send(`/api/v1${source.base}/commerce/sales-service-products`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:{id?:unknown;version?:unknown};error?:{code?:string;message?:string}};
   assertCurrent();
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED')attempt=null;throw Object.assign(Error(body.error?.message??'The service item was not confirmed.'),{code:body.error?.code});}
   if(typeof body.data?.id!=='string'||body.data.id!==`products-${captured.key}`||body.data.version!==1)throw Error('The service item receipt is incomplete. Confirm the original request.');
   captured.uncertain=true;const result:SalesServiceProductResult={id:body.data.id,version:1};
   return {result,submitted:captured.input,unchanged:()=>!captured.hadCompletion||!!captured.completion()&&commerceFormUnchanged(captured.completion()!),acknowledge(){
    assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,commerceAcknowledged:true}}));announceWorkChange();return true;
   }};
  }catch(error){if(attempt===captured)captured.uncertain=true;throw error;}finally{busy=false;}
 }
 return {submit(raw:SalesServiceProductInput,completion?:WorkCommandCompletion){const source=current();if(!source?.contextKey)throw Error('Review current Sales posting before creating a service item.');const input=salesServiceProductSchema.parse(raw),body=JSON.stringify(input);if(attempt&&(attempt.body!==body||!same(source,attempt.source)||commerceCommandContext()!==attempt.work))throw Error('Confirm the original service item first. Your newer fields remain.');attempt??={source:{...source},work:commerceCommandContext(),input,body,key:crypto.randomUUID(),uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);},retry(){if(!attempt)throw Error('There is no original service item request to confirm.');return run(attempt);},pending:()=>attempt!==null};
}
