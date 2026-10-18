"use client";
import { useSearchParams } from "@/lib/shared/destination-search";
import WorkspaceLink from "@/components/workplace/workspace-link";
import {pmsSourceLink} from "@/server/modules/pms/public/rules/pms-source-link";
import {RecordCollectionDrawer,RecordFacts,CollectionRecordLink} from './record-collection';
import {Table,TableFrame} from '@/components/ui/table';
import type {pmsHotelSources} from "@/server/modules/pms/public/pms/home";
import type {PmsWorkspace} from "@/server/modules/pms/public/pms";
type Sources=Awaited<ReturnType<typeof pmsHotelSources>>;
export function PmsHotelSources({api,frontHref}:{api:string;frontHref:string}){
 const query=useSearchParams(),legacy:Record<string,string>={};
 try{const parsed:unknown=JSON.parse(query.get('detailArgs')??'{}');if(parsed&&typeof parsed==='object'&&!Array.isArray(parsed))for(const [key,value] of Object.entries(parsed))if(['metric','businessDate','property'].includes(key)&&typeof value==='string')legacy[key]=value;}catch{}
 // Collection links carry owner arguments in the ordinary query. Retain the
 // historical JSON form only when the current link does not supply that field.
 const argument=(key:'metric'|'businessDate'|'property')=>query.get(key)??legacy[key];
 const id=query.get('detailId')??'',metric=argument('metric')??'occupancy',page=query.get('detailPage')??'1';
 const next=(p:number)=>{const q=new URLSearchParams(query.toString());q.set('detailPage',String(p));return `?${q}`;};
 return <><RecordCollectionDrawer<Sources> kind="pms_hotel_sources" title="Hotel overview sources" endpoint={`${api}/hotel-sources/${encodeURIComponent(id)}?${new URLSearchParams({metric,businessDate:argument('businessDate')??'',page})}`} select={(snapshot,propertyId)=>snapshot.hotel.id===propertyId?snapshot.hotel:null} render={(record,snapshot)=><><RecordFacts record={record} fields={['name','businessDate','state','currency','capacity','sold','occupancy','roomRevenue','adr','revpar']}/><p>Room revenue uses minor currency units. Occupancy = sold / capacity; ADR = room revenue / sold; RevPAR = room revenue / capacity.</p><TableFrame appearance="standard" label="Hotel metric sources"><Table><thead><tr><th>Record</th><th>Type</th><th>Date / state</th><th>Net room revenue (minor units)</th></tr></thead><tbody>{snapshot.rows.map(row=><tr key={row.id}><td><CollectionRecordLink kind="pms_hotel_record" id={row.id} args={{property:snapshot.hotel.id}}>{String(row.data.name??row.data.number??row.data.description??row.id)}</CollectionRecordLink></td><td>{row.kind}</td><td>{String(row.data.date??row.data.arrival??row.data.cleanliness??row.data.state??'')}</td><td>{row.kind==='entry'?String(row.data.net):'—'}</td></tr>)}</tbody></Table></TableFrame><nav aria-label="Hotel source pages">{snapshot.page>1&&<a href={next(snapshot.page-1)}>Previous</a>} {snapshot.hasNext&&<a href={next(snapshot.page+1)}>Next</a>}</nav></>}/>
 <RecordCollectionDrawer<PmsWorkspace> kind="pms_hotel_record" title="Hotel source record" endpoint={`${api}/hotel-record/${encodeURIComponent(id)}?${new URLSearchParams({property:argument('property')??''})}`} select={(snapshot,recordId)=>{const row=snapshot.snapshot?.records.find(r=>r.id===recordId);return row?{id:row.id,version:row.version,kind:row.kind,...row.data}:null;}} render={(record,snapshot)=>{
  const property=snapshot.snapshot?.property,row=snapshot.snapshot?.records.find(item=>item.id===record.id);
  const source=property&&row?pmsSourceLink({id:row.id,kind:row.kind,version:row.version,property_id:property.id,property_name:property.name,data:{
   number:'number' in row.data&&typeof row.data.number==='string'?row.data.number:undefined,
   guestAccountId:'guestAccountId' in row.data&&typeof row.data.guestAccountId==='string'?row.data.guestAccountId:undefined,
  }}):null;
  return <><RecordFacts record={record} fields={['id','version','kind','name','number','description','state','cleanliness','active','roomId','guestAccountId','arrival','departure','date','reference','quantity','net','service','tax','total']}/>{source&&<WorkspaceLink href={frontHref+source.href.slice('/pms/'.length)}>Open full hotel record</WorkspaceLink>}</>;
 }}/></>;
}
