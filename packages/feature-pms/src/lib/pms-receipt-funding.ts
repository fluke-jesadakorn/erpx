import {records,type Snapshot} from "@erpx/server-pms/public/rules/pms-model";
export type ReceiptPart={entryId:string;amount:number};
// Snapshot-local native evidence. It is neither serialized with the snapshot nor
// accepted in a command body. Historical amounts come from the owned posted
// credit-balance journal; the engine captures the same effect before persistence.
const allocationCredits=new WeakMap<Snapshot,Map<string,number>>();
export function captureReceiptAllocationCredit(s:Snapshot,entryId:string,amount:number):void{
 if(!Number.isSafeInteger(amount)||amount<=0)throw new Error('PMS_RECEIPT_CREDIT_INVALID');
 let credits=allocationCredits.get(s);if(!credits){credits=new Map();allocationCredits.set(s,credits);}
 if(credits.has(entryId)&&credits.get(entryId)!==amount)throw new Error('PMS_RECEIPT_CREDIT_CHANGED');
 credits.set(entryId,amount);
}
/** Original receipt budgets, including legacy FIFO allocations without source traces.
 * Holds and settled liabilities consume the same original receipt, independently
 * of current room prices and of other receipts sharing the GuestAccount.
 */
export function availableReceiptSources(s:Snapshot,guestAccountId:string,pocket:'advance'|'deposit',excludeHolds=true):ReceiptPart[]{
 const entries=records(s,'entry').filter(e=>e.data.guestAccountId===guestAccountId).sort((a,b)=>a.data.at-b.data.at||a.id.localeCompare(b.id));
 const advance=new Map(entries.filter(e=>e.data.kind==='payment'&&e.data.reference!=='deposit-application').map(e=>[e.id,e.data.total])),deposit=new Map(entries.filter(e=>e.data.kind==='deposit').map(e=>[e.id,e.data.total]));
 const consumed:Array<{at:number;parts:ReceiptPart[]}>=[];
 const subtract=(map:Map<string,number>,parts:ReceiptPart[])=>{for(const part of parts)map.set(part.entryId,(map.get(part.entryId)??0)-part.amount);};
 for(const e of entries){
  const data=e.data;
  if(data.reference==='deposit-application'){
   // A damage deposit applied to a charge settles receivables directly. It never
   // becomes an unused room advance or a source for a later advance refund.
   subtract(deposit,data.funding??[]);
  }else if(data.description==='payment.refund_liability'||data.kind==='refund'&&data.description!=='payment.refund_settle'){
   const refund=records(s,'refund').find(r=>r.id===data.originalId);
   subtract((data.refundPocket??refund?.data.pocket??'advance')==='deposit'?deposit:advance,data.funding??[]);
  }else if(data.kind==='allocation'&&data.description==='Allocate advance payment'){
   const parts:ReceiptPart[]=[];
   if(data.funding?.length){subtract(advance,data.funding);parts.push(...data.funding.map(part=>({...part})));}
   else{let remaining=data.total;for(const [id,budget] of advance){const n=Math.min(remaining,Math.max(0,budget));advance.set(id,budget-n);remaining-=n;if(n)parts.push({entryId:id,amount:n});if(!remaining)break;}}
   // Only a real fully traced allocation can later release its receipt units.
   const total=parts.reduce((sum,part)=>sum+part.amount,0);
   if(Number.isSafeInteger(data.total)&&data.total>0&&Number.isSafeInteger(total)&&total===data.total&&new Set(parts.map(part=>part.entryId)).size===parts.length&&parts.every(part=>Number.isSafeInteger(part.amount)&&part.amount>0&&entries.some(source=>source.id===part.entryId&&source.data.kind==='payment'&&source.data.reference!=='deposit-application'&&source.data.at<=data.at)))consumed.push({at:data.at,parts});
  }
 }
 // A correction releases only its actual posted credit, not its whole negative
 // charge or the bill's current advance. Consume each original allocation unit
 // once across corrections, then apply all ordinary refunds/carries/holds above.
 for(const e of entries){
  const credit=allocationCredits.get(s)?.get(e.id),original=entries.find(row=>row.id===e.data.originalId);
  if(!credit||e.data.kind!=='adjustment'||e.data.total>=0||!original||original.data.method==='opening'||!['charge','transfer'].includes(original.data.kind)||original.data.total<=0||e.data.total!==-original.data.total||credit>original.data.total||entries.filter(row=>row.data.originalId===original.id).length!==1)continue;
  const eligible=consumed.filter(allocation=>allocation.at<=e.data.at),total=eligible.reduce((sum,allocation)=>sum+allocation.parts.reduce((n,part)=>n+part.amount,0),0);
  if(!Number.isSafeInteger(total)||total<credit)continue;
  let remaining=credit;
  for(const allocation of eligible)for(const part of allocation.parts){const amount=Math.min(remaining,part.amount);part.amount-=amount;remaining-=amount;advance.set(part.entryId,(advance.get(part.entryId)??0)+amount);}
 }
 const budget=pocket==='advance'?advance:deposit;
 if(excludeHolds){const guestAccount=records(s,'guest_account').find(f=>f.id===guestAccountId);for(const part of guestAccount?.data.refundHolds??[])if(part.pocket===pocket)budget.set(part.entryId,(budget.get(part.entryId)??0)-part.amount);}
 return [...budget].filter(([,amount])=>amount>0).map(([entryId,amount])=>({entryId,amount}));
}
