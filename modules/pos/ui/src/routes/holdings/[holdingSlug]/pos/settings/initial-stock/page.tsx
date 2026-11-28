import {PosInitialPhysicalStockCount} from '@erpx/pos-ui/components/pos-initial-physical-stock-count';
import {posInitialPhysicalStockCountWorkspace,posInitialPhysicalStockCountSelectionSchema} from '@erpx/inventory-server/public/pos-initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
 const selection=posInitialPhysicalStockCountSelectionSchema.parse({warehouseId:query.warehouseId,productId:query.productId,countDate:query.countDate}),initial=await posInitialPhysicalStockCountWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug));
 return <PosInitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial}/>;
}
