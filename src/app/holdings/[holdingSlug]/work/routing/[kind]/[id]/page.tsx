import { RecordLink as Link } from "@/components/records/record-link";
import {z} from 'zod';
import {flowKinds} from "@/lib/shared/flow";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {requireRoutingManager} from "@/server/modules/workflow/public/routing-attention";
import {requireFlowAccess} from "@/server/modules/workflow/public/flow-access";
import {FlowPanel} from '@/components/workplace/flow-panel';
import {pageError} from "@/server/modules/work_projection/public/page-error";
import { PageContent } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function RoutingRepairPage({params}:{params:Promise<{holdingSlug:string;kind:string;id:string}>}) {
 const p=await params,kind=z.enum(flowKinds).parse(p.kind),actor=await requireWorkContext(p.holdingSlug),base=workspaceBase(publicScope(actor));
 await requireRoutingManager(actor).catch(error=>pageError(error,p.holdingSlug));await requireFlowAccess(actor,kind,p.id).catch(error=>pageError(error,p.holdingSlug));
 return <PageContent as="div" className="ui-page-content"><Link href={`${base}/work/routing?kind=${kind}`}>← Source routing review</Link><h1>Review responsibility</h1><p>{kind.replaceAll('_',' ')} · {p.id}</p><p>Choose an eligible person for the source workflow. This changes routing only; it does not approve, post, pay or complete the source document.</p><FlowPanel actor={actor} kind={kind} id={p.id} base={base}/></PageContent>;
}
