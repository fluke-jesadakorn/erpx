'use client';
import {FrontLocalizedTree} from '@/components/front/front-language';
import {usePathname,useRouter,useSearchParams} from 'next/navigation';
import {Button,Field,Select,TextInput} from '@/components/ui/foundation';
import type {pmsWorkspace} from '@/server/pms';
type Meta=NonNullable<NonNullable<Awaited<ReturnType<typeof pmsWorkspace>>['snapshot']>['listMeta']>;
export function PmsListPagination({meta}:{meta:Meta}){
 const router=useRouter(),pathname=usePathname(),params=useSearchParams();
 const booking=meta.section==='reservations'||meta.section==='front-desk';
 const dated=booking||['cashier','night-audit','inventory'].includes(meta.section);
 const states=meta.kind==='room'?['dirty','clean','inspected']:meta.kind==='folio'?['open','closed']:meta.kind==='shift'?['open','closed','reviewed']:meta.kind==='night'?['running','blocked','completed']:[];
 function page(value:number){const next=new URLSearchParams(params);next.set('page',String(value));router.push(`${pathname}?${next}`);}
 return <FrontLocalizedTree><section className="card" aria-label="List filters"><form onSubmit={e=>{e.preventDefault();const fields=new FormData(e.currentTarget),next=new URLSearchParams(params);for(const name of ['q','from','to','status'])if(fields.has(name))next.set(name,String(fields.get(name)??''));next.set('page','0');router.push(`${pathname}?${next}`);}} className="content-stack"><div className="form-grid"><Field>{booking?'Guest name or confirmation':'Search records'}<TextInput name="q" defaultValue={meta.q} maxLength={200}/></Field>{dated&&<><Field>From<TextInput name="from" type="date" required defaultValue={meta.from}/></Field><Field>Through<TextInput name="to" type="date" required defaultValue={meta.to}/></Field></>}{states.length>0&&<Field>Status<Select name="status" defaultValue={meta.status}><option value="">All</option>{states.map(state=><option value={state} key={state}>{state.replaceAll('_',' ')}</option>)}</Select></Field>}</div><Button clientManaged type="submit" variant="secondary">{booking?'Search bookings':'Search records'}</Button></form><p role="status">Page {meta.page+1} · {meta.ids.length} {booking?'booking records':'records'}</p><Button type="button" variant="secondary" disabled={meta.page===0} onClick={()=>page(meta.page-1)}>Previous page</Button><Button type="button" variant="secondary" disabled={!meta.hasNext} onClick={()=>page(meta.page+1)}>Next page</Button></section></FrontLocalizedTree>;
}
