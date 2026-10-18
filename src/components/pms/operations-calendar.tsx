'use client';
import {FrontLocalizedTree} from '@/components/front/front-language';
import {useEffect,useState} from 'react';
import {Button,Field,Table,TableFrame,TextInput} from '@/components/ui/foundation';
import {CollectionRecordLink} from '@/components/records/record-collection';
import {records,dates,plusDays,type Snapshot} from "@/server/modules/pms/public/rules/pms-model";
import {workFetch} from "@/lib/work_projection/work-fetch";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
export function PmsOperationsCalendar({base,propertyId,today}:{base:string;propertyId:string;today:string}){
 const [from,setFrom]=useState(today),[loaded,setLoaded]=useState<{base:string;propertyId:string;from:string;snapshot:Snapshot}|null>(null),[error,setError]=useState(''),[retry,setRetry]=useState(0);
 useEffect(()=>{
  const abort=new AbortController(),assertWork=captureWorkCommandContext(),front=document.querySelector<HTMLElement>('[data-front-app]'),frontScope=front?[front.dataset.frontApp,front.dataset.frontLocation].join(':'):null;
  workFetch(`/api/v1${base}/pms?${new URLSearchParams({property:propertyId,view:'calendar',from,to:plusDays(from,14)})}`,{signal:abort.signal}).then(async r=>{
   const json=await r.json();if(!r.ok)throw new Error(json.error?.message??'Calendar unavailable');
   if(!abort.signal.aborted){
    assertWork();
    if(front&&(front.isConnected===false||document.querySelector('[data-front-app]')!==front||frontScope!==[front.dataset.frontApp,front.dataset.frontLocation].join(':')))throw Object.assign(new Error('Your Front location changed. Reload the workspace.'),{code:'WORK_CONTEXT_CHANGED'});
    setLoaded({base,propertyId,from,snapshot:json.data.snapshot});setError('');
   }
  }).catch(e=>{if(!abort.signal.aborted&&e?.code!=='WORK_CONTEXT_CHANGED')setError(e instanceof Error?e.message:'Calendar unavailable');});
  return()=>abort.abort();
 },[base,propertyId,from,retry]);
 const currentLoaded=loaded?.base===base&&loaded.propertyId===propertyId?loaded:null;
 const s=currentLoaded?.snapshot,days=dates(currentLoaded?.from??from,plusDays(currentLoaded?.from??from,14));
 return <FrontLocalizedTree><section className="card"><h2>Room calendar</h2><Field>From<TextInput type="date" value={from} onChange={e=>{if(e.target.value)setFrom(e.target.value);}}/></Field>{currentLoaded?.from!==from&&<p role="status">Loading calendar…</p>}{error&&<p role="alert">{error} <Button onClick={()=>setRetry(retry+1)}>Retry</Button></p>}{s&&<><TableFrame label="Room calendar"><Table><thead><tr><th>Room</th>{days.map(d=><th key={d}>{d.slice(5)}</th>)}</tr></thead><tbody>{records(s,'room').filter(r=>r.data.active).map(room=><tr key={room.id}><th>{room.data.number}</th>{days.map(date=>{const stay=records(s,'stay').find(st=>st.data.arrival<=date&&st.data.departure>date&&(st.data.segments.length?st.data.segments.some(segment=>segment.roomId===room.id&&segment.from<=date&&segment.to>date):st.data.roomId===room.id)),block=records(s,'block').find(b=>(!b.data.expiresAt||b.data.expiresAt>Date.now())&&b.data.roomId===room.id&&b.data.from<=date&&(b.data.kind==='maintenance'||b.data.to>date));return <td key={date}>{stay?<CollectionRecordLink kind="pms_record" id={stay.id}>{stay.data.state==='in_house'?'In house':'Reserved'}</CollectionRecordLink>:block?'Unavailable':'Available'}</td>;})}</tr>)}</tbody></Table></TableFrame><p>Unassigned reservations</p>{records(s,'stay').filter(st=>!st.data.roomId).map(st=><p key={st.id}><CollectionRecordLink kind="pms_record" id={st.id}>{records(s,'room_type').find(t=>t.id===st.data.typeId)?.data.name} · {st.data.arrival} → {st.data.departure}</CollectionRecordLink></p>)}</>}</section></FrontLocalizedTree>;
}
