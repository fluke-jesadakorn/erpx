import {ManufacturingInitialPhysicalStockCount} from '@erpx/inventory-ui/components/manufacturing-initial-physical-stock-count';
import {manufacturingInitialPhysicalStockCountWorkspace,manufacturingInitialPhysicalStockCountFlow} from '@erpx/inventory-server/public/manufacturing-initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),initial=await manufacturingInitialPhysicalStockCountWorkspace(actor,{countId:id}).catch(error=>pageError(error,holdingSlug)),flow=initial.available&&initial.record?await manufacturingInitialPhysicalStockCountFlow(actor,id).catch(error=>pageError(error,holdingSlug)):null;
 return <ManufacturingInitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial} flow={flow}/>;
}
