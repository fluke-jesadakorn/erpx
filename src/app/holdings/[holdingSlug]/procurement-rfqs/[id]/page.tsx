import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {rfqDetail,rfqWorkspace} from '@/server/procurement-rfqs';
import {RfqConsole} from '@/components/modules/procurement-rfq-console';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await rfqDetail(a,id).catch(e=>pageError(e,holdingSlug)),options=await rfqWorkspace(a).catch(e=>pageError(e,holdingSlug));return <div className="content-stack"><div className="form-actions"><Link href={`${base}/work`}>Back to Work</Link><Link href={`${base}/procurement-rfqs`}>All quotation requests</Link></div><h1>{String(data.rfq.title)}</h1><RfqConsole key={String(data.rfq.version)} base={base} data={data} suppliers={options.suppliers}/></div>;}
