import {InitialPhysicalStockCount} from '@erpx/inventory-ui/components/initial-physical-stock-count';
import {initialPhysicalStockCountWorkspace,initialPhysicalStockCountSelectionSchema} from '@erpx/inventory-server/public/initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
 const selection=initialPhysicalStockCountSelectionSchema.parse({warehouseId:query.warehouseId,productId:query.productId,countDate:query.countDate}),initial=await initialPhysicalStockCountWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug));
 return <InitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial}/>;
}
