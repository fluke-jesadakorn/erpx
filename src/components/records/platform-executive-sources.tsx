"use client";
import { useSearchParams } from "@/lib/shared/destination-search";
import Link from "@/components/workplace/workspace-link";
import {RecordCollectionDrawer,RecordFacts} from './record-collection';
import {Table,TableFrame} from '@/components/ui/table';
import {TimeText} from "@/components/people_payroll/time/time-preference-context";
type Sources={id:string;name:string;metric:string;rows:Record<string,unknown>[];page:number;hasNext:boolean};
export function PlatformExecutiveSources(){
 const q=useSearchParams();let args:Record<string,string>={};try{args=JSON.parse(q.get('detailArgs')??'{}');}catch{}
 const params=new URLSearchParams({metric:args.metric??'',group:q.get('detailId')??'',from:args.from??'',to:args.to??'',page:q.get('detailPage')??'1'});
 function href(page:number){const next=new URLSearchParams(q);next.set('detailPage',String(page));return `?${next}`;}
 return <RecordCollectionDrawer<Sources> kind="platform_executive_sources" title="Platform source observations" endpoint={`/api/v1/platform/executive?${params}`} select={(snapshot,id)=>snapshot.id===id?{id,name:snapshot.name}:null} render={(record,snapshot)=><><RecordFacts record={record}/><TableFrame appearance="standard" label="Source observations"><Table><thead><tr><th>Observation</th><th>State</th><th>Time</th></tr></thead><tbody>{snapshot.rows.map((row,i)=><tr key={i}><td>{String(row.component??'Provisioning job')}</td><td>{String(row.status??row.action)}</td><td><TimeText appearance="standard" value={Number(row.checked_at??row.created_at)}/></td></tr>)}</tbody></Table></TableFrame>{!snapshot.rows.length&&<p>No matching source observations.</p>}<nav aria-label="Source record pages">{snapshot.page>1&&<Link href={href(snapshot.page-1)}>Previous</Link>}{snapshot.hasNext&&<Link href={href(snapshot.page+1)}>Next</Link>}</nav></>}/>;
}
