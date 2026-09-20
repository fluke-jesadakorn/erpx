import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {supplierBatchWorkspace} from '@/server/supplier-payments';
import {SupplierBatchConsole} from '@/components/modules/supplier-batch-console';
export const dynamic='force-dynamic';
export default async function SupplierBatchPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug);
 return <SupplierBatchConsole base={workspaceBase(publicScope(actor))} data={await supplierBatchWorkspace(actor,id)}/>;
}
