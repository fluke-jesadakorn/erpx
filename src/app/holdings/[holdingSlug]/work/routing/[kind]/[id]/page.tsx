import Link from 'next/link';
import {z} from 'zod';
import {flowKinds} from '@/lib/flow';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {requireRoutingManager} from '@/server/routing-attention';
import {requireFlowAccess} from '@/server/flow-access';
import {FlowPanel} from '@/components/workplace/flow-panel';
export const dynamic='force-dynamic';
export default async function RoutingRepairPage({params}:{params:Promise<{holdingSlug:string;kind:string;id:string}>}) {
 const p=await params,kind=z.enum(flowKinds).parse(p.kind),actor=await requireWorkContext(p.holdingSlug),base=workspaceBase(publicScope(actor));
 await requireRoutingManager(actor);await requireFlowAccess(actor,kind,p.id);
 return <div className="content-stack"><Link href={`${base}/work/routing?kind=${kind}`}>← Source routing review</Link><h1>Review responsibility</h1><p>{kind.replaceAll('_',' ')} · {p.id}</p><p>Choose an eligible person for the source workflow. This changes routing only; it does not approve, post, pay or complete the source document.</p><FlowPanel actor={actor} kind={kind} id={p.id} base={base}/></div>;
}
