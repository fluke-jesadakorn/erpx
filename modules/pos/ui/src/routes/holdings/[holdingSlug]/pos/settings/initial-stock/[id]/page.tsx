import {PosInitialPhysicalStockCount} from '@erpx/pos-ui/components/pos-initial-physical-stock-count';
import {posInitialPhysicalStockCountWorkspace,posInitialPhysicalStockCountFlow} from '@erpx/inventory-server/public/pos-initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),initial=await posInitialPhysicalStockCountWorkspace(actor,{countId:id}).catch(error=>pageError(error,holdingSlug)),flow=initial.available&&initial.record?await posInitialPhysicalStockCountFlow(actor,id).catch(error=>pageError(error,holdingSlug)):null;
 return <PosInitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial} flow={flow}/>;
}
