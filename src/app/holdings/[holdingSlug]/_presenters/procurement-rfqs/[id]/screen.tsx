import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordLink as Link } from "@/components/records/record-link";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {rfqDetail,rfqWorkspace} from "@/server/modules/procurement/public/procurement-rfqs";
import { RfqConsole } from "@/components/records/loaders/modules--procurement-rfq-console";
import { PageContent } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await rfqDetail(a,id).catch(e=>pageError(e,holdingSlug)),options=await rfqWorkspace(a).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><div className="ui-form-actions"><Link href={canonicalUiHref(`${base}/work`)}>Back to Work</Link><Link href={canonicalUiHref(`${base}/procurement-rfqs`)}>All quotation requests</Link></div><h1>{String(data.rfq.title)}</h1><RfqConsole key={String(data.rfq.id)} base={base} data={data} suppliers={options.suppliers}/></PageContent>;}
