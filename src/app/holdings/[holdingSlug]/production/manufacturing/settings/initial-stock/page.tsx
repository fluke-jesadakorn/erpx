import {ManufacturingInitialPhysicalStockCount} from '@/components/inventory/manufacturing-initial-physical-stock-count';
import {manufacturingInitialPhysicalStockCountWorkspace,manufacturingInitialPhysicalStockCountSelectionSchema} from '@/server/modules/inventory/public/manufacturing-initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
 const selection=manufacturingInitialPhysicalStockCountSelectionSchema.parse({warehouseId:query.warehouseId,productId:query.productId,countDate:query.countDate}),initial=await manufacturingInitialPhysicalStockCountWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug));
 return <ManufacturingInitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial}/>;
}
