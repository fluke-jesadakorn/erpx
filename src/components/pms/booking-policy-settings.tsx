"use client";
import {FrontLocalizedTree} from '@/components/front/front-language';
import { useEffect,useState } from 'react';
import { FormDisclosure } from '@/components/ui/form-disclosure';
import { Button,Field,Select,TextInput } from '@/components/ui/foundation';
import { RecordDocumentPanel } from '@/components/attachments/record-document-panel';
import { MediaPublicationPanel } from '@/components/attachments/media-publication-panel';
import Image from 'next/image';
import { listMediaPublications,publicMediaPublicationUrl,type MediaPublicationDto } from '@/lib/attachment-client';
import type { Rate,Snapshot } from '@/lib/pms-model';
type Save=(operation:string,input:unknown,version?:number)=>Promise<boolean>;
export function BookingRatePolicy({id,rate,version,save}:{id:string;rate:Rate;version:number;save:Save}){
 const [method,setMethod]=useState(rate.collection?.method??'pay_at_hotel'),[busy,setBusy]=useState(false);
 return <FrontLocalizedTree><FormDisclosure appearance="standard"><summary>Payment policy · {rate.name}</summary><form onSubmit={async e=>{e.preventDefault();setBusy(true);const data=new FormData(e.currentTarget);try{await save('rate.save',{...rate,id,collection:{method,depositPercent:Number(data.get('percent')??30),changesAllowed:data.get('changes')==='on',cancellationsAllowed:data.get('cancellations')==='on',cancellationDays:data.get('days')?Number(data.get('days')):undefined,cancellationTime:String(data.get('time')??'')||undefined,cancellationFeePercent:Number(data.get('fee')??0)}},version);}finally{setBusy(false);}}}>
  <Field appearance="standard">Collection method<Select appearance="standard" value={method} onChange={e=>setMethod(e.target.value as typeof method)}><option value="pay_at_hotel">Pay at hotel</option><option value="full">Pay in full online</option><option value="deposit">Room advance online</option></Select></Field>
  <Field appearance="standard">Room advance (%)<TextInput appearance="standard" name="percent" type="number" required min={1} max={99} defaultValue={rate.collection?.depositPercent??30} disabled={method!=='deposit'}/></Field>
  <Field appearance="standard" orientation="inline"><TextInput appearance="standard" name="changes" type="checkbox" defaultChecked={rate.collection?.changesAllowed??true}/>Allow guest changes before cancellation deadline</Field>
  <Field appearance="standard" orientation="inline"><TextInput appearance="standard" name="cancellations" type="checkbox" defaultChecked={rate.collection?.cancellationsAllowed??true}/>Allow guest cancellation before the deadline</Field>
  <Field appearance="standard">Cancellation: days before arrival<TextInput appearance="standard" name="days" type="number" min={0} max={365} defaultValue={rate.collection?.cancellationDays} placeholder="Use hotel default"/></Field>
  <Field appearance="standard">Deadline in hotel timezone<TextInput appearance="standard" name="time" type="time" defaultValue={rate.collection?.cancellationTime}/></Field>
  <Field appearance="standard">Cancellation fee (% of accepted stay total, including tax)<TextInput appearance="standard" name="fee" type="number" min={0} max={100} defaultValue={rate.collection?.cancellationFeePercent??0}/></Field>
  <p>Room advances are separate from damage deposits. Existing stays retain their accepted policy.</p>
  <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Save payment policy</Button>
 </form></FormDisclosure></FrontLocalizedTree>;
}
export function BookingOnlineAccounts({snapshot,save,canPrepare,canApprove,accounts}:{snapshot:Snapshot;save:Save;canPrepare:boolean;canApprove:boolean;accounts:Array<{id:string;name:string;category:string}>}){
 const mapping=snapshot.property.config.onlineAccounting,[busy,setBusy]=useState(false);
 return <FrontLocalizedTree><FormDisclosure appearance="standard"><summary>Online payment liability accounts · {mapping?.approvedBy?'Approved':'Needs Finance review'}</summary>
  <p>Finance approves separate liabilities for refunds and money received without an available booking.</p>
  {canPrepare&&!mapping?.approvedBy&&<form onSubmit={async e=>{e.preventDefault();setBusy(true);const data=new FormData(e.currentTarget);try{await save('online_accounting.prepare',{refundPayable:String(data.get('refundPayable')),unallocated:String(data.get('unallocated'))},snapshot.property.version);}finally{setBusy(false);}}}>
   {(['refundPayable','unallocated'] as const).map(k=><Field appearance="standard" key={k}>{k==='refundPayable'?'Refunds payable':'Unallocated guest receipts'}<Select appearance="standard" name={k} required defaultValue={mapping?.[k]??''}><option value="">Choose a liability account</option>{accounts.filter(a=>a.category==='liability'&&!Object.values(snapshot.property.config.accounts).includes(a.id)).map(a=><option key={a.id} value={a.id}>{a.name}</option>)}</Select></Field>)}
   <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Prepare online accounts</Button>
  </form>}
  {canApprove&&mapping&&!mapping.approvedBy&&<Button appearance="standard" type="button" variant="primary" disabled={busy} onClick={async()=>{setBusy(true);try{await save('online_accounting.approve',{},snapshot.property.version);}finally{setBusy(false);}}}>Approve independently</Button>}
 </FormDisclosure></FrontLocalizedTree>;
}
export function BookingFeeAccounts({snapshot,save,canPrepare,canApprove,accounts}:{snapshot:Snapshot;save:Save;canPrepare:boolean;canApprove:boolean;accounts:Array<{id:string;name:string;category:string}>}){
 const mapping=snapshot.property.config.bookingFees,[busy,setBusy]=useState(false);
 return <FrontLocalizedTree><FormDisclosure appearance="standard" dismissDisabled={busy}><summary>Cancellation fee accounting · {mapping?.approvedBy?'Approved':'Needs Finance review'}</summary><p>Cancellation fees use the accepted fee percentage and tax treatment. They are recorded separately from room revenue.</p>{canPrepare&&!mapping?.approvedBy&&<form onSubmit={async e=>{e.preventDefault();const account=String(new FormData(e.currentTarget).get('account'));setBusy(true);try{await save('booking_fees.prepare',{account},snapshot.property.version);}finally{setBusy(false);}}}><Field appearance="standard">Cancellation fee revenue<Select appearance="standard" name="account" required defaultValue={mapping?.account??''}><option value="">Choose a separate revenue account</option>{accounts.filter(a=>a.category==='revenue'&&!Object.values(snapshot.property.config.accounts).includes(a.id)).map(a=><option key={a.id} value={a.id}>{a.name}</option>)}</Select></Field><Button appearance="standard" clientManaged type="submit" disabled={busy}>Prepare fee account</Button></form>}{mapping&&!mapping.approvedBy&&canApprove&&<Button appearance="standard" type="button" disabled={busy} onClick={async()=>{setBusy(true);try{await save('booking_fees.approve',{},snapshot.property.version);}finally{setBusy(false);}}}>Approve independently</Button>}</FormDisclosure></FrontLocalizedTree>;
}
export function BookingMediaSetup({base,targetType,targetId,version,canReview,title}:{base:string;targetType:'pms_property'|'pms_room_type';targetId:string;version:number;canReview:boolean;title:string}){
 return <FrontLocalizedTree><FormDisclosure appearance="standard"><summary>Gallery · {title}</summary>
  <RecordDocumentPanel appearance="standard" base={base} targetType={targetType} targetId={targetId} expectedTargetVersion={version} editable purposes={[{key:'booking_media',label:'Booking gallery originals',description:'Upload hotel or room images. Only independently approved sanitized images can appear on the guest website.'}]}/>
  <MediaPublicationPanel appearance="standard" base={base} targetType={targetType} targetId={targetId} purposeKey="booking_media" canCreate canReview={canReview} title="Preview and approve public images"/>
 </FormDisclosure></FrontLocalizedTree>;
}

export function BookingMediaPicker({base,targetType,targetId,name,selected=[]}:{base:string;targetType:string;targetId:string;name:string;selected?:import('@/lib/pms-model').BookingImage[]}){
 const [media,setMedia]=useState<MediaPublicationDto[]>([]),[error,setError]=useState(''),[loaded,setLoaded]=useState(false);
 useEffect(()=>{let active=true;listMediaPublications(base,targetType,targetId).then(result=>{if(active){setMedia(result.items.filter(m=>m.state==='approved'));setLoaded(true);}}).catch(()=>{if(active)setError('Public images are unavailable. Refresh before saving.');});return()=>{active=false;};},[base,targetType,targetId]);
 return <FrontLocalizedTree><fieldset><legend>Guest gallery</legend>{(!loaded||error)&&selected.map(i=><span key={i.publicationId}><input type="hidden" name={name} value={i.publicationId}/><input type="hidden" name={`alt-${i.publicationId}`} value={i.alt}/></span>)}{error&&<p role="alert">{error}</p>}{!media.length&&!error&&<p>Add and approve images in Gallery below, then refresh to select them.</p>}{media.map(m=><Field key={m.id} appearance="standard" orientation="inline"><TextInput appearance="standard" name={name} type="checkbox" value={m.id} defaultChecked={selected.some(i=>i.publicationId===m.id)}/><Image unoptimized src={publicMediaPublicationUrl(base,m.id)} alt={m.alt_text} width={120} height={80}/>{m.alt_text}<input type="hidden" name={`alt-${m.id}`} value={m.alt_text}/></Field>)}</fieldset></FrontLocalizedTree>;
}
