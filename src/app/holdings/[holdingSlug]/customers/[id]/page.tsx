import { TimeText } from "@/components/time/time-preference-context";
import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {customerTimeline} from '@/server/customer-timeline';
import {customerNotesForCustomer} from '@/server/customer-notes';
import {CustomerNotes} from '@/components/modules/customer-notes';
import {pageError} from '@/server/page-error';
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string}>;searchParams:Promise<{cursor?:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),q=await searchParams,data=await customerTimeline(a,id,{cursor:q.cursor}).catch(e=>pageError(e,holdingSlug)),notes=await customerNotesForCustomer(a,id).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={`${base}/customers`}>Customer 360</Link><h1>{data.customer.name}</h1><p>{data.customer.code} · Internal service timeline</p><p>Open the source to read its evidence and next action. This view does not grant additional access.</p><Card as="section" appearance="standard" className="ui-panel"><h2>Visible source activity</h2>{data.items.map(e=><Link className="ui-record-item" key={e.id} href={`${base}${e.href}`}><strong>{e.title} · {e.status}</strong><span>{<TimeText appearance="standard" value={Number(e.at)} />} · Source version {e.version}</span></Link>)}{!data.items.length&&<p>No activity in this scope.</p>}{data.nextCursor&&<Link href={`${base}/customers/${id}?cursor=${encodeURIComponent(data.nextCursor)}`}>Older activity</Link>}{q.cursor&&<Link href={`${base}/customers/${id}`}>Latest activity</Link>}</Card><CustomerNotes base={base} data={notes}/><Card as="section" appearance="standard" className="ui-panel"><h2>Coverage</h2><p>{data.coverage.join(' · ')}</p><p>{data.limitations}</p></Card></PageContent>;}
