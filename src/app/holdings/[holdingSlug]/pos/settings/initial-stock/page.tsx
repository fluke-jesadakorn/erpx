import {PosInitialPhysicalStockCount} from '@/components/pos/pos-initial-physical-stock-count';
import {posInitialPhysicalStockCountWorkspace,posInitialPhysicalStockCountSelectionSchema} from '@/server/modules/inventory/public/pos-initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
 const selection=posInitialPhysicalStockCountSelectionSchema.parse({warehouseId:query.warehouseId,productId:query.productId,countDate:query.countDate}),initial=await posInitialPhysicalStockCountWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug));
 return <PosInitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial}/>;
}
