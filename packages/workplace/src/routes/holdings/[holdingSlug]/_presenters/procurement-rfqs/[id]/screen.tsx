import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {rfqDetail,rfqWorkspace} from "@erpx/server-procurement/public/procurement-rfqs";
import { RfqConsole } from "@erpx/feature-records/components/loaders/modules--procurement-rfq-console";
import { PageContent } from "@erpx/ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await rfqDetail(a,id).catch(e=>pageError(e,holdingSlug)),options=await rfqWorkspace(a).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><div className="ui-form-actions"><Link href={canonicalUiHref(`${base}/work`)}>Back to Work</Link><Link href={canonicalUiHref(`${base}/procurement-rfqs`)}>All quotation requests</Link></div><h1>{String(data.rfq.title)}</h1><RfqConsole key={String(data.rfq.id)} base={base} data={data} suppliers={options.suppliers}/></PageContent>;}
