import {PosInitialPhysicalStockCount} from '@/components/pos/pos-initial-physical-stock-count';
import {posInitialPhysicalStockCountWorkspace,posInitialPhysicalStockCountFlow} from '@/server/modules/inventory/public/pos-initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),initial=await posInitialPhysicalStockCountWorkspace(actor,{countId:id}).catch(error=>pageError(error,holdingSlug)),flow=initial.available&&initial.record?await posInitialPhysicalStockCountFlow(actor,id).catch(error=>pageError(error,holdingSlug)):null;
 return <PosInitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial} flow={flow}/>;
}
