import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {refundWorkspace} from '@/server/refunds';
import {RefundConsole} from '@/components/modules/refund-console';
export const dynamic='force-dynamic';
export default async function RefundPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug);
 return <RefundConsole base={workspaceBase(publicScope(actor))} data={await refundWorkspace(actor,id)}/>;
}
