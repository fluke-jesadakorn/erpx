import {records,type Snapshot} from './pms-model';
export type ReceiptPart={entryId:string;amount:number};
/** Original receipt budgets, including legacy FIFO allocations without source traces.
 * Holds and settled liabilities consume the same original receipt, independently
 * of current room prices and of other receipts sharing the GuestAccount.
 */
export function availableReceiptSources(s:Snapshot,guestAccountId:string,pocket:'advance'|'deposit',excludeHolds=true):ReceiptPart[]{
 const entries=records(s,'entry').filter(e=>e.data.guestAccountId===guestAccountId).sort((a,b)=>a.data.at-b.data.at||a.id.localeCompare(b.id));
 const advance=new Map(entries.filter(e=>e.data.kind==='payment'&&e.data.reference!=='deposit-application').map(e=>[e.id,e.data.total])),deposit=new Map(entries.filter(e=>e.data.kind==='deposit').map(e=>[e.id,e.data.total]));
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
   if(data.funding?.length)subtract(advance,data.funding);
   else{let remaining=data.total;for(const [id,budget] of advance){const n=Math.min(remaining,Math.max(0,budget));advance.set(id,budget-n);remaining-=n;if(!remaining)break;}}
  }
 }
 const budget=pocket==='advance'?advance:deposit;
 if(excludeHolds){const guestAccount=records(s,'guest_account').find(f=>f.id===guestAccountId);for(const part of guestAccount?.data.refundHolds??[])if(part.pocket===pocket)budget.set(part.entryId,(budget.get(part.entryId)??0)-part.amount);}
 return [...budget].filter(([,amount])=>amount>0).map(([entryId,amount])=>({entryId,amount}));
}
