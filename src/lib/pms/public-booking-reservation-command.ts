import {guestInputSchema} from '@/lib/pms/pms-booking-domain';

export type PublicReservationScope={base:string;propertyId:string;owner:string;draft:number};
export type PublicReservationInput={propertyId:string;quoteToken:string;guest:{name:string;email:string;phone:string};notes:string};
export type PublicReservationReceipt={id:string;number:string;version:number;state:'confirmed'|'payment_pending'|'expired'|'refund_review'|'cancelled';hotel:{id:string;name:string;address:string;contactEmail:string;contactPhone:string};guest:{name:string;email:string};currency:string;total:number;paymentState:'received'|'pay_at_hotel';managementToken:string;attemptToken?:string;checkoutUrl?:string|null;expiresAt?:number;refundState?:string|null};
export type PublicReservationFetch=(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>;
type Completion={form:HTMLFormElement;edit:string|null};
type Attempt={scope:PublicReservationScope;body:string;key:string;sent:boolean;completion?:{form:WeakRef<HTMLFormElement>;edit:string|null};turn?:symbol};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const same=(a:PublicReservationScope|null,b:PublicReservationScope)=>!!a&&a.base===b.base&&a.propertyId===b.propertyId&&a.owner===b.owner;
const changed=()=>new DOMException('The booking page changed. Return to the original hotel to recover your reservation.','AbortError');
export function publicBookingCheckoutUrl(value:unknown):value is string{if(!text(value))return false;try{const url=new URL(value);return url.protocol==='https:'&&url.hostname==='checkout.stripe.com'&&!url.username&&!url.password&&!url.port;}catch{return false;}}
/** These three native quote failures occur before reservation.create, and receipt
 * replay precedes readQuote. They prove no commit only on the first invocation. */
const firstQuoteRejection=(status:number,code:unknown)=>typeof code==='string'&&(status===409&&['PMS_QUOTE_EXPIRED','PMS_QUOTE_CHANGED'].includes(code)||status===422&&code==='PMS_QUOTE_INVALID');
export function validPublicReservationReceipt(value:unknown,input:PublicReservationInput):value is PublicReservationReceipt{
 if(!object(value)||!text(value.id)||!text(value.number)||!Number.isSafeInteger(value.version)||Number(value.version)<1||!text(value.state)||!['confirmed','payment_pending','expired','refund_review','cancelled'].includes(value.state)||!object(value.hotel)||value.hotel.id!==input.propertyId||!['name','address','contactEmail','contactPhone'].every(key=>typeof value.hotel==='object'&&value.hotel!==null&&typeof Reflect.get(value.hotel,key)==='string')||!object(value.guest)||value.guest.name!==input.guest.name.trim()||value.guest.email!==input.guest.email.trim()||value.currency!=='THB'||!Number.isSafeInteger(value.total)||Number(value.total)<0||!text(value.paymentState)||!['received','pay_at_hotel'].includes(value.paymentState)||!text(value.managementToken))return false;
 if(value.checkoutUrl!==undefined&&value.checkoutUrl!==null&&!publicBookingCheckoutUrl(value.checkoutUrl))return false;
 if(value.refundState!==undefined&&value.refundState!==null&&typeof value.refundState!=='string')return false;
 if(value.attemptToken!==undefined&&(!text(value.attemptToken)||!Number.isSafeInteger(value.expiresAt)||Number(value.expiresAt)<=0))return false;
 return value.state!=='payment_pending'||text(value.attemptToken)&&publicBookingCheckoutUrl(value.checkoutUrl);
}
/** One anonymous native reservation. It owns the first exact body/key even when
 * Checkout or original JSON fails after the native transaction has committed. */
export function createPublicReservationClient(current:()=>PublicReservationScope|null=()=>null,send:PublicReservationFetch=fetch,newKey:()=>string=()=>crypto.randomUUID()){
 let attempt:Attempt|null=null,busy=false;
 const assertCurrent=(selected:Attempt)=>{if(!same(current(),selected.scope))throw changed();};
 async function run(selected:Attempt){
  if(busy)throw Error('Wait for the original reservation request.');assertCurrent(selected);busy=true;const turn=Symbol('public-reservation');selected.turn=turn;
  try{
   const first=!selected.sent;selected.sent=true;
   const response=await send(`${selected.scope.base}/reservations`,{method:'POST',cache:'no-store',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body}),envelope:unknown=await response.json();assertCurrent(selected);
   if(!response.ok){const error=object(envelope)&&object(envelope.error)?envelope.error:{};if(first&&firstQuoteRejection(response.status,error.code))attempt=null;throw Error(typeof error.message==='string'?error.message:'The reservation could not be confirmed. Recover the original request before starting another.');}
   const input:PublicReservationInput=JSON.parse(selected.body),data=object(envelope)?envelope.data:null;
   if(!validPublicReservationReceipt(data,input))throw Error('The reservation confirmation is incomplete. Recover the original request.');
   const unchanged=()=>{const form=selected.completion?.form.deref();return current()?.draft===selected.scope.draft&&(!selected.completion||!!form&&form.isConnected&&form.getAttribute('data-erpx-edit-version')===selected.completion.edit);};
   return {data,assertCurrent:()=>assertCurrent(selected),unchanged,acknowledge(){assertCurrent(selected);if(attempt!==selected||selected.turn!==turn)return false;attempt=null;const form=selected.completion?.form.deref();if(form&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form,formEditVersion:selected.completion?.edit}}));return true;}};
  }finally{busy=false;}
 }
 return {bind(read:()=>PublicReservationScope|null){current=read;},submit(input:PublicReservationInput,completion?:Completion){const scope=current();if(!scope||scope.propertyId!==input.propertyId)throw changed();const guest=guestInputSchema.safeParse(input.guest);if(!guest.success)throw Error(guest.error.issues[0]?.message??'Check the guest details.');if(!text(input.quoteToken)||input.quoteToken.length>100000||typeof input.notes!=='string'||input.notes.trim().length>2000)throw Error('Review the original quote and guest details.');const body=JSON.stringify(input);if(attempt&&(attempt.body!==body||!same(scope,attempt.scope)))throw Error('Recover the original reservation first. Your newer details are retained.');attempt??={scope:{...scope},body,key:newKey(),sent:false,completion:completion?{form:new WeakRef(completion.form),edit:completion.edit}:undefined};return run(attempt);},recover(){if(!attempt)throw Error('There is no original reservation to recover.');return run(attempt);},hasPending:()=>attempt!==null,isBusy:()=>busy};
}
