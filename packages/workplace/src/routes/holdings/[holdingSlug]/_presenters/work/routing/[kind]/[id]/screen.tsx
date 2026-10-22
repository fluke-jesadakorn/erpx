import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {z} from 'zod';
import {flowKinds} from "@erpx/feature-shared/lib/flow";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {requireRoutingManager} from "@erpx/server-workflow/public/routing-attention";
import {requireFlowAccess} from "@erpx/server-workflow/public/flow-access";
import {FlowPanel} from '@erpx/workplace/components/flow-panel';
import {pageError} from "@erpx/server-work_projection/public/page-error";
import { PageContent } from "@erpx/ui/components/foundation";
export const dynamic='force-dynamic';
export default async function RoutingRepairPage({params}:{params:Promise<{holdingSlug:string;kind:string;id:string}>}) {
 const p=await params,kind=z.enum(flowKinds).parse(p.kind),actor=await requireWorkContext(p.holdingSlug),base=workspaceBase(publicScope(actor));
 await requireRoutingManager(actor).catch(error=>pageError(error,p.holdingSlug));await requireFlowAccess(actor,kind,p.id).catch(error=>pageError(error,p.holdingSlug));
 return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/work/routing?kind=${kind}`)}>← Source routing review</Link><h1>Review responsibility</h1><p>{kind.replaceAll('_',' ')} · {p.id}</p><p>Choose an eligible person for the source workflow. This changes routing only; it does not approve, post, pay or complete the source document.</p><FlowPanel actor={actor} kind={kind} id={p.id} base={base}/></PageContent>;
}
