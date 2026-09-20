"use client";
import { FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { useNavigationCounts } from "@/components/workplace/navigation-counts";
import { selfServiceActions } from "@/lib/self-service";
import { workFetch } from "@/lib/work-fetch";
import { lastWorkChange,scheduleWorkRefresh } from "@/lib/work-refresh";
import type { SelfServiceSeed,SelfServiceSummary } from "@/server/self-service";
import { ArrowUpRight,CalendarDays,CircleAlert,Clock3,Palmtree,RefreshCw,ShoppingBag } from "lucide-react";
import Link from "next/link";
import { Suspense,use,useCallback,useEffect,useRef,useState,type ReactNode } from "react";
import styles from "./self-service.module.css";
const icons={shifts:CalendarDays,leave:Palmtree,overtime:Clock3,purchase_request:ShoppingBag};
export function SelfServiceHome({base,children,enabled,initial}:{base:string;children:ReactNode;enabled:boolean;initial?:Promise<SelfServiceSeed|null>}) {
 if(!enabled)return children;
 return <WidgetGrid pageKey="home" className={styles.homeGrid}>
  <nav className={styles.shortcuts} aria-label="Staff self-service">{selfServiceActions.map(action=>{const Icon=icons[action.id];return <Link key={action.id} href={`${base}${action.path}`}><Icon size={21}/><strong>{action.label}</strong><span>{action.description}</span><ArrowUpRight className={styles.arrow} size={15}/></Link>;})}</nav>
  <Suspense fallback={<><p role="status">Loading your day…</p></>}>
   <SelfServiceDay base={base} initial={initial}/>
  </Suspense>
  <FeatureWidget id="home-feed" title="Feed">{children}</FeatureWidget>
 </WidgetGrid>;
}
function SelfServiceDay({base,initial}:{base:string;initial?:Promise<SelfServiceSeed|null>}) {
 const { publishWork } = useNavigationCounts();
 const seed = initial ? use(initial) : null;
 const [data,setData]=useState<SelfServiceSummary|null>(seed?.data ?? null),[error,setError]=useState('');
 const [workCount,setWorkCount] = useState<number|null>(seed?.workCount ?? null);
 const generation = useRef(0);
 const load=useCallback(async(signal?:AbortSignal)=>{const current = ++generation.current;try{const response=await workFetch(`/api/v1${base}/self-service/summary`,{cache:'no-store',signal});const body=await response.json();if(!response.ok)throw Error(body.error?.message??'My day is unavailable.');if(!signal?.aborted && current === generation.current){setData(body.data);setError('');const count = response.headers.get('X-ERPX-Work-Count');setWorkCount(count === null || count === 'unknown' ? null : Number(count));}}catch(e){if(!signal?.aborted && current === generation.current)setError(e instanceof Error?e.message:'My day is unavailable.');}},[base]);
 // Reuse streamed data; refresh only if it was invalidated or has aged out.
 useEffect(()=>{const controller=new AbortController();const timer= !seed || Date.now()-seed.data.asOf > 30_000 || seed.data.asOf < lastWorkChange() ? setTimeout(()=>void load(controller.signal),0) : undefined;const stop=scheduleWorkRefresh(()=>void load(controller.signal));return()=>{clearTimeout(timer);controller.abort();stop();};},[seed,load]);
 useEffect(() => {
  if (!data) return;
  const timer = setTimeout(() => publishWork(workCount), 0);
  return () => clearTimeout(timer);
 }, [data,workCount,publishWork]);
 return (
  <>
   {error?<p role="alert">{error} <button className="button secondary" onClick={()=>void load()}>Try again</button></p>:!data?<p role="status">Loading your day…</p>:<>
    <FeatureWidget className={styles.dayWidget} id="home-schedule" title="Your next shift">{data.staffSetupRequired?<p>Connect a staff profile to show your personal schedule.</p>:data.unavailableSources.includes('Schedule')?<p>Schedule could not refresh.</p>:data.nextShift?<Link className={styles.shift} href={`${base}/people/roster?view=mine`}><strong>{data.nextShift.name}</strong><span>{data.nextShift.local_date} · {data.nextShift.start_time}–{data.nextShift.end_time}{data.nextShift.end_time<data.nextShift.start_time?' (+1 day)':''}</span><small>{data.nextShift.timezone}</small></Link>:<p>No upcoming published shift.</p>}<Link href={`${base}/calendar`}>Open Calendar <ArrowUpRight size={13}/></Link></FeatureWidget>
    <FeatureWidget className={styles.dayWidget} id="home-attention" title="Needs your attention">{data.attention===null?<p>Work unavailable</p>:data.attention.length?data.attention.map(item=><Link className={styles.miniRow} key={`${item.type}:${item.id}`} href={`${base}${item.href}`}><strong>{item.title}</strong><span>{item.nextAction??item.detail}</span></Link>):<p>{data.unavailableSources.length?"No actions in the available sources.":"No actions waiting for you."}</p>}</FeatureWidget>
    <FeatureWidget className={styles.dayWidget} id="home-requests" title="My requests">{data.recentRequests===null?<p>Requests unavailable</p>:data.recentRequests.length?data.recentRequests.map(item=><Link className={styles.miniRow} key={`${item.type}:${item.id}`} href={`${base}${item.href}`}><strong>{item.title}</strong><span>{item.status.replaceAll('_',' ')} · {item.responsibleQueue??'Review pending'}</span></Link>):<p>{data.unavailableSources.length?"No requests in the available sources.":"No active requests."}</p>}<Link href={`${base}/work?view=requests`}>All my requests <ArrowUpRight size={13}/></Link></FeatureWidget>
    {!!data.setupBlockers.length&&<section className={styles.setup}><CircleAlert size={20} aria-hidden="true"/><div><small>ACCOUNT SETUP</small><h2>{data.staffSetupRequired?'Connect your staff profile':'Complete your setup'}</h2><ul>{data.setupBlockers.map(b=><li key={b}>{b}</li>)}</ul><Link className={styles.setupAction} href={`${base}/people/settings`}>Open People settings <ArrowUpRight size={13}/></Link></div></section>}
    {!!data.unavailableSources.length&&<section className={styles.unavailable} role="status"><div><RefreshCw size={16} aria-hidden="true"/><span><strong>Some information could not refresh</strong><small>{data.unavailableSources.join(' · ')}</small></span></div><button className="button secondary" onClick={()=>void load()}>Try again</button></section>}
   </>}
  </>
 );
}
