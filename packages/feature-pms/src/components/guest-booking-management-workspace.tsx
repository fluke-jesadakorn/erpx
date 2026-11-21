"use client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Button,Field,Select,TextInput,ActionAnchor,InlineNotice,Panel,FormLayout,FormActions,RecordList,RecordListItem } from '@erpx/ui/components/foundation';
import { DraftFormDrawer } from '@erpx/ui/components/draft-form-drawer';
import { useEffect,useLayoutEffect,useRef,useState } from 'react';
import { money,plusDays } from "@erpx/server-pms/public/rules/pms-model";
import { FrontLocalizedTree,useFrontLanguage } from '@erpx/front/components/front-language';
import {frontText} from '@erpx/feature-shared/lib/front-language';
import {BookingPolicySummary} from "@erpx/feature-pms/components/booking-policy-summary";
import { BookingQuoteDetailsStandard as QuoteDetails } from '@erpx/feature-pms/components/booking-quote-details-standard';
import {createGuestBookingChangeClient,guestBookingCheckoutContinuation,type GuestBookingChangeReceipt} from '@erpx/feature-pms/lib/guest-booking-change-command';
import type { manageBooking } from "@erpx/server-pms/public/pms-booking";
import type { publicChangeQuote } from "@erpx/server-pms/public/pms-booking-changes";
import type {guestBookingDocument} from "@erpx/server-pms/public/pms/guest-document";
import type { RoomSelection } from '@erpx/server-pms/public/rules/pms-booking';
import { registerUnsavedWork, contextIsSwitching, hasUnsavedWork } from '@erpx/feature-work_projection/lib/context-switch';
import styles from '@erpx/feature-pms/components/guest-booking-management-workspace.module.css';
type Booking=Awaited<ReturnType<typeof manageBooking>>;
type Proposal=Awaited<ReturnType<typeof publicChangeQuote>>;
export function GuestBookingManagementWorkspace({slug}:{slug:string}){
 const language=useFrontLanguage(),[access,setAccess]=useState({slug,token:'',version:0}),activeAccess=useRef<typeof access|null>(null);
 useEffect(()=>{
  const storageKey=`booking-access:${slug}`;
  function load(){
   const fragment=new URLSearchParams(window.location.hash.slice(1)),provided=fragment.has('token');
   if(!provided&&activeAccess.current?.slug===slug)return;
   let stored='';try{stored=sessionStorage.getItem(storageKey)??'';}catch{/* Signed links also work when browser storage is unavailable. */}
   const next=provided?fragment.get('token')??'':stored;
   const strip=()=>{if(provided){fragment.delete('token');const rest=fragment.toString();history.replaceState(history.state,'',window.location.pathname+window.location.search+(rest?'#'+rest:''));}};
   if(!next&&!provided&&activeAccess.current?.slug!==slug){activeAccess.current={slug,token:'',version:0};return;}
   if(activeAccess.current?.slug===slug&&activeAccess.current.token===next){strip();return;}
   if(activeAccess.current?.slug===slug&&hasUnsavedWork()&&!window.confirm(frontText('Change booking and leave the current draft? Submitted changes remain with their original booking.',language))){strip();return;}
   strip();if(provided){try{if(next)sessionStorage.setItem(storageKey,next);else sessionStorage.removeItem(storageKey);}catch{/* Keep access in this mounted view only. */}}
   const updated={slug,token:next,version:(activeAccess.current?.version??0)+1};activeAccess.current=updated;setAccess(updated);
  }
  load();window.addEventListener('hashchange',load);return()=>window.removeEventListener('hashchange',load);
 },[slug,language]);
 const visible=access.slug===slug?access:{token:'',version:0};
 return <GuestBookingManagementForHolding key={`${slug}:${visible.version}`} slug={slug} accessToken={visible.token}/>;
}
function GuestBookingManagementForHolding({slug,accessToken}:{slug:string;accessToken:string}){
 const language=useFrontLanguage();
 const [token,setToken]=useState(''),[booking,setBooking]=useState<Booking|null>(null),[error,setError]=useState(''),[notice,setNotice]=useState(''),[busy,setBusy]=useState(false);
 const [selected,setSelected]=useState<string[]>([]),[editor,setEditor]=useState<'change'|'cancel'|null>(null),[proposal,setProposal]=useState<Proposal|null>(null),[drafts,setDrafts]=useState<Array<{stayId:string;room:RoomSelection}>>([]),[checkout,setCheckout]=useState<Pick<GuestBookingChangeReceipt,'checkoutUrl'|'attemptToken'>|null>(null);
 const [baseline,setBaseline]=useState(''),dirtyRef=useRef(false),context=useRef({epoch:0,base:'',token:''});
 const base=`/api/public/booking/${encodeURIComponent(slug)}`,inFlight=useRef(false);
 const [changes]=useState(()=>createGuestBookingChangeClient<Booking>());
 useLayoutEffect(()=>{changes.bind(()=>{const scope=context.current;return booking&&scope.base===base&&scope.token?{base:canonicalUiHref(base),token:scope.token,epoch:scope.epoch,id:booking.id,number:booking.number}:null;});return()=>changes.bind(()=>null);},[base,booking,changes]);
 const dirty=!!editor&&(!!proposal||JSON.stringify(drafts)!==baseline);
 useEffect(()=>{dirtyRef.current=dirty||!!editor&&busy;},[dirty,editor,busy]);
 useEffect(()=>{const unregister=registerUnsavedWork(()=>dirtyRef.current||changes.hasPending()),unload=(event:BeforeUnloadEvent)=>{if((dirtyRef.current||changes.hasPending())&&!contextIsSwitching())event.preventDefault();};window.addEventListener('beforeunload',unload);return()=>{unregister();window.removeEventListener('beforeunload',unload);};},[changes]);
 async function request(action:string,body:unknown={},access=context.current.token,key?:string,method:'GET'|'POST'='POST'){
  const epoch=context.current.epoch;if(context.current.base!==base||access!==context.current.token)throw new DOMException('Booking context changed','AbortError');
  const r=await holdingFetch(canonicalUiHref(`${base}/${action}`),{method,cache:'no-store',headers:{'Content-Type':'application/json',...(access?{Authorization:`Bearer ${access}`} : {}),...(key?{'Idempotency-Key':key}:{})},...(method==='POST'?{body:JSON.stringify(body)}:{})}),result=await r.json();
  if(epoch!==context.current.epoch||context.current.base!==base||access!==context.current.token)throw new DOMException('Booking context changed','AbortError');if(!r.ok)throw new Error(result.error?.message??'Please try again');return result.data;
 }
 useEffect(()=>{
  const value=accessToken,epoch=++context.current.epoch;context.current={epoch,base,token:value};
  if(!value)return()=>{++context.current.epoch;};let active=true;const controller=new AbortController();
  holdingFetch(canonicalUiHref(`${base}/manage`),{method:'POST',cache:'no-store',signal:controller.signal,headers:{'Content-Type':'application/json',Authorization:`Bearer ${value}`},body:'{}'}).then(async r=>{const body=await r.json();if(!r.ok)throw new Error(body.error?.message??'Unable to load booking');if(active&&epoch===context.current.epoch){setToken(value);setBooking(body.data);}}).catch(e=>{if(active&&epoch===context.current.epoch)setError(e.message);});return()=>{active=false;controller.abort();++context.current.epoch;};
 },[base,accessToken]);
 async function downloadDocument(id:string){
  const d=await request('document',{id}) as Awaited<ReturnType<typeof guestBookingDocument>>;
  const escape=(value:unknown)=>String(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
  const content=`<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>${escape(d.number)}</title><style>body{font:16px system-ui;max-width:900px;margin:40px auto;padding:20px;color:#203631}table{width:100%;border-collapse:collapse}th,td{text-align:left;padding:12px;border-bottom:1px solid #ddd}td:last-child{text-align:right}@media print{body{margin:0}}</style><h1>${escape(d.hotel)}</h1><h2>${escape(d.kind)} · ${escape(d.number)}</h2><p>${escape(d.date)} · ${escape(d.guest)} · ${escape(d.guestAccount)}</p><table><thead><tr><th>Date</th><th>Description</th><th>Net</th><th>Service</th><th>Tax</th><th>Total</th></tr></thead><tbody>${d.entries.map(e=>`<tr><td>${escape(e.date)}</td><td>${escape(e.description)}</td><td>${escape(money(e.net,d.currency))}</td><td>${escape(money(e.service,d.currency))}</td><td>${escape(money(e.tax,d.currency))}</td><td>${escape(money(e.total,d.currency))}</td></tr>`).join('')}</tbody></table><h2>Total ${escape(money(d.amount,d.currency))}</h2><p>Balance due at issue ${escape(money(d.dueAtIssue,d.currency))}</p><p>Use your browser's Print command to print or save as PDF.</p></html>`;
  const url=URL.createObjectURL(new Blob([content],{type:'text/html;charset=utf-8'})),link=document.createElement('a');link.href=url;link.download=`${d.number}.html`;link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
 }
 async function action(fn:()=>Promise<void>){if(inFlight.current)return;inFlight.current=true;const epoch=context.current.epoch;setBusy(true);setError('');try{await fn();}catch(e){if(epoch===context.current.epoch&&!(e instanceof DOMException&&e.name==='AbortError'))setError(e instanceof Error?e.message:'Please try again');}finally{inFlight.current=false;if(epoch===context.current.epoch)setBusy(false);}}
 function confirm(recover=false){void action(async()=>{if(!recover&&(!editor||!proposal))return;const outcome=recover?await changes.recover():await changes.confirm(editor!,proposal!.token,proposal!.proposal.refund);if(!outcome.acknowledge())return;setCheckout(outcome.receipt);setBooking(outcome.booking);setSelected([]);setEditor(null);setProposal(null);setNotice(guestBookingCheckoutContinuation(outcome.receipt,outcome.booking)?'Complete the additional payment to apply your change. Your original booking remains confirmed.':outcome.refund>0?'Your booking has been updated. The refund request is awaiting independent approval.':'Your booking has been updated.');});}
 function open(kind:'change'|'cancel'){if(changes.hasPending())return;setProposal(null);const next=booking!.rooms.filter(r=>selected.includes(r.id)).map(r=>({stayId:r.id,room:{typeId:r.typeId,rateId:r.rateId,arrival:r.arrival,departure:r.departure,adults:r.adults,childAges:r.childAges,quantity:1}}));setDrafts(next);setBaseline(JSON.stringify(next));setEditor(kind);}
 function dateBounds(draft:{stayId:string;room:RoomSelection},field:'arrival'|'departure'){
  if(booking?.purchases.newWork)return {};
  const original=booking?.rooms.find(room=>room.id===draft.stayId);if(!original)return {};
  return field==='arrival'?{min:original.arrival,max:plusDays(draft.room.departure,-1)}:{min:plusDays(draft.room.arrival,1),max:original.departure};
 }
 return <FrontLocalizedTree><div className={styles.workspace} data-ui-appearance="standard">
  {error&&<InlineNotice tone="danger">{error}</InlineNotice>}{notice&&<InlineNotice tone="success">{notice}</InlineNotice>}
  {changes.hasPending()&&<InlineNotice tone="warning"><p>{changes.needsHistory()?'Your confirmation was received. Refresh booking history to finish.':'The result of your original confirmation is uncertain. Recover it before making other changes.'}</p><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>confirm(true)}>{changes.needsHistory()?'Refresh confirmed booking':'Recover original confirmation'}</Button></InlineNotice>}
  {booking&&<Panel appearance="standard" aria-label={frontText("Booking details",language)}>
   <span className={styles.eyebrow}>{booking.state.replaceAll('_',' ')}</span><h2>{booking.hotel.name}</h2><p>Confirmation {booking.number}</p><p>{booking.guest.name}</p>
   {!booking.purchases.newWork&&<p className={styles.notice}>{booking.purchases.settleExisting?'Existing bookings remain available. You can shorten or cancel eligible stays; contact the hotel for other changes.':'Your booking history and documents are available. Contact the hotel for further changes.'}</p>}
   <div className={styles.paymentSplit}><p>Current stay total<strong>{money(booking.total,booking.currency)}</strong></p><p>Pay at hotel<strong>{money(booking.dueAtHotel,booking.currency)}</strong></p></div>
   <RecordList>{booking.rooms.map(room=><RecordListItem className={styles.offer} key={room.id}><div><Field appearance="standard" orientation="inline"><TextInput appearance="standard" type="checkbox" aria-label={`${language==='th'?'เลือก':'Select'} ${room.roomName} ${room.arrival}`} disabled={busy||changes.hasPending()||room.state!=='reserved'||Boolean(booking.pendingChange)} checked={selected.includes(room.id)} onChange={e=>setSelected(e.target.checked?[...selected,room.id]:selected.filter(id=>id!==room.id))}/><h3>{room.roomName}</h3></Field><p>{room.arrival} → {room.departure}</p><p>{room.adults} adult(s) · {room.childAges.length} child(ren) · {room.state.replaceAll('_',' ')}</p><BookingPolicySummary policy={room.policy}/></div><strong>{money(room.nights.reduce((n,p)=>n+p.total,0),booking.currency)}</strong></RecordListItem>)}</RecordList>
   {guestBookingCheckoutContinuation(checkout,booking)&&<ActionAnchor appearance="standard" variant="primary" href={guestBookingCheckoutContinuation(checkout,booking)!}>Continue to secure payment</ActionAnchor>}
   {booking.pendingChange&&<div className={styles.notice}><p>Your original booking remains confirmed while the change awaits payment.</p><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>action(async()=>{const result=await request(`reservations?attempt=${encodeURIComponent(booking.pendingChange!.attemptToken)}`,{},token,undefined,'GET');setCheckout(result);setBooking(await request('manage'));})}>Refresh payment status</Button></div>}
   <FormActions><Button appearance="standard" type="button" variant="primary" disabled={!selected.length||busy||changes.hasPending()||!booking.editableRates.length||!booking.purchases.settleExisting&&!booking.purchases.newWork} onClick={()=>open('change')}>Change selected rooms</Button><Button appearance="standard" type="button" variant="secondary" disabled={!selected.length||busy||changes.hasPending()||!booking.purchases.settleExisting} onClick={()=>open('cancel')}>Cancel selected rooms</Button><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>action(async()=>setBooking(await request('manage')))}>Refresh booking</Button></FormActions>
   {booking.refunds.length>0&&<div><h3>Refund tracking</h3>{booking.refunds.map(r=><p key={r.id}>{money(Number(r.amount_minor),booking.currency)} · {r.state==='requested'?'Awaiting independent approval':r.state==='requires_action'?'Waiting for bank details':r.state==='processing'?'Refund in progress':r.state==='succeeded'?'Refund completed':r.state==='failed'?'Refund failed; hotel review required':'Refund approved'} · {r.reason}</p>)}</div>}
   {booking.documents.length>0&&<div><h3>Documents</h3>{booking.documents.map(d=><p key={d.id}>{d.kind} · {new Date(Number(d.created_at)).toLocaleDateString()} · <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>action(()=>downloadDocument(d.id))}>Download document</Button></p>)}</div>}
   {booking.history.length>0&&<details><summary>Booking history</summary>{booking.history.map((h,i)=><p key={i}>{new Date(h.at).toLocaleString()} · {h.action}</p>)}</details>}
   <p>{booking.hotel.address} · {booking.hotel.contactPhone} · <a href={`mailto:${booking.hotel.contactEmail}`}>{booking.hotel.contactEmail}</a></p>
  </Panel>}
  <DraftFormDrawer appearance="standard" open={Boolean(editor)} dirty={dirty} onClose={()=>{if(!busy&&!changes.hasPending()){setEditor(null);setProposal(null);}}} title={frontText(editor==='change'?'Change your stay':'Review cancellation',language)} confirmationTitle={frontText('Discard changes?',language)} confirmationDescription={frontText('Your draft has unsaved changes.',language)} discardLabel={frontText('Discard draft',language)} keepEditingLabel={frontText('Keep editing',language)} dismissDisabled={busy||changes.hasPending()}>
   {proposal?<div><p>Review the changes before confirming. Your original booking remains confirmed until any additional payment succeeds.</p>{proposal.proposal.items.map(item=><div key={item.stayId}>{item.quote?<QuoteDetails quote={item.quote}/>:<p>Cancel room · {money(item.oldTotal,booking!.currency)}</p>}</div>)}<p>Cancellation fee: {money(proposal.proposal.fee,booking!.currency)}</p><p>Selected rooms: pay at hotel after this change: {money(proposal.proposal.items.reduce((n,p)=>n+p.dueAtHotel,0),booking!.currency)}</p><div className={styles.paymentSplit}><p>Additional payment<strong>{money(proposal.proposal.dueNow,booking!.currency)}</strong></p><p>Refund to request<strong>{money(proposal.proposal.refund,booking!.currency)}</strong></p></div><p>Refunds need independent approval. Cancelled rooms are released immediately.</p>{error&&<p role="alert">{error}</p>}<FormActions><Button appearance="standard" type="button" variant="secondary" disabled={busy||changes.hasPending()} onClick={()=>setProposal(null)}>Back</Button><Button appearance="standard" type="button" variant="primary" disabled={busy||changes.hasPending()} onClick={()=>confirm()}>Confirm changes</Button>{changes.hasPending()&&<Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>setEditor(null)}>Close and keep confirmation</Button>}</FormActions></div>:<FormLayout layout="stack" onSubmit={e=>{e.preventDefault();void action(async()=>{setProposal(await request(`${editor}-quote`,{items:drafts.map(d=>editor==='change'?d:{stayId:d.stayId})}));});}}>
    {drafts.map((draft,i)=><fieldset className={styles.fieldset} disabled={busy||changes.hasPending()} key={draft.stayId}><legend>{booking?.rooms.find(r=>r.id===draft.stayId)?.roomName}</legend>{editor==='change'?<div className={styles.grid}>{(['arrival','departure'] as const).map(k=><Field appearance="standard" key={k}>{k==='arrival'?'Check-in':'Check-out'}<TextInput appearance="standard" type="date" required {...dateBounds(draft,k)} value={draft.room[k]} onChange={e=>setDrafts(drafts.map((d,j)=>j===i?{...d,room:{...d.room,[k]:e.target.value}}:d))}/></Field>)}<Field appearance="standard">Room and rate<Select appearance="standard" required disabled={!booking?.purchases.newWork} value={draft.room.rateId} onChange={e=>{const rate=booking!.editableRates.find(r=>r.id===e.target.value)!;setDrafts(drafts.map((d,j)=>j===i?{...d,room:{...d.room,rateId:rate.id,typeId:rate.typeId}}:d));}}>{booking?.editableRates.map(rate=><option key={rate.id} value={rate.id}>{rate.roomName} · {rate.name}</option>)}</Select></Field></div>:<p>This room will be cancelled according to its accepted cancellation terms.</p>}</fieldset>)}{error&&<p role="alert">{error}</p>}<Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||changes.hasPending()}>Review price and terms</Button>
   </FormLayout>}
  </DraftFormDrawer>
  <FormLayout layout="stack" className={styles.recoveryForm} onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);void action(async()=>{await request('recover',{number:f.get('number'),email:f.get('email')});setNotice('If these details match a booking, an access link will be emailed to you');});}}><h2>Find your booking</h2><div className={styles.grid}><Field appearance="standard">Confirmation number<TextInput appearance="standard" name="number" required/></Field><Field appearance="standard">Booking email<TextInput appearance="standard" name="email" type="email" required/></Field></div><FormActions><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Email a private link</Button></FormActions></FormLayout>
 </div></FrontLocalizedTree>;
}
