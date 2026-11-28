import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {rfqDetail,rfqWorkspace} from "@erpx/procurement-server/public/procurement-rfqs";
import { RfqConsole } from "@erpx/records-ui/components/loaders/modules--procurement-rfq-console";
import { PageContent } from "@erpx/platform-ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await rfqDetail(a,id).catch(e=>pageError(e,holdingSlug)),options=await rfqWorkspace(a).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><div className="ui-form-actions"><Link href={canonicalUiHref(`${base}/work`)}>Back to Work</Link><Link href={canonicalUiHref(`${base}/procurement-rfqs`)}>All quotation requests</Link></div><h1>{String(data.rfq.title)}</h1><RfqConsole key={String(data.rfq.id)} base={base} data={data} suppliers={options.suppliers}/></PageContent>;}
