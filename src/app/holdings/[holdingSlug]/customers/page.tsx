import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {customerDirectory} from '@/server/customer-timeline';
import {pageError} from '@/server/page-error';
import { PageContent, Card, Table, TableFrame } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),rows=await customerDirectory(a).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={`${base}/commerce?view=sales`}>Sales</Link><h1>Customer 360</h1><p>Customers with a sales document or qualified opportunity visible in your current workspace.</p><Card as="section" appearance="standard" className="ui-panel"><TableFrame appearance="standard" label="Customer directory"><Table><thead><tr><th>Customer code</th><th>Name</th></tr></thead><tbody>{rows.slice(0,200).map(r=><tr key={r.id}><td><Link href={`${base}/customers/${r.id}`}>{r.code}</Link></td><td>{r.name}</td></tr>)}</tbody></Table></TableFrame>{!rows.length&&<p>No customers with visible source records.</p>}{rows.length>200&&<p>Showing the first 200 customers. Open additional customer timelines from their sales documents.</p>}</Card></PageContent>;}
