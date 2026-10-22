import {InitialPhysicalStockCount} from '@erpx/feature-inventory/components/initial-physical-stock-count';
import {initialPhysicalStockCountWorkspace,initialPhysicalStockCountSelectionSchema} from '@erpx/server-inventory/public/initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
 const selection=initialPhysicalStockCountSelectionSchema.parse({warehouseId:query.warehouseId,productId:query.productId,countDate:query.countDate}),initial=await initialPhysicalStockCountWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug));
 return <InitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial}/>;
}
