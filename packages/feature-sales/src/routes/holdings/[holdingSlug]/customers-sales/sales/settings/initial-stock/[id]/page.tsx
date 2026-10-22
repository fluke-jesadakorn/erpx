import {InitialPhysicalStockCount} from '@erpx/feature-inventory/components/initial-physical-stock-count';
import {initialPhysicalStockCountWorkspace,initialPhysicalStockCountFlow} from '@erpx/server-inventory/public/initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),initial=await initialPhysicalStockCountWorkspace(actor,{countId:id}).catch(error=>pageError(error,holdingSlug)),flow=initial.available&&initial.record?await initialPhysicalStockCountFlow(actor,id).catch(error=>pageError(error,holdingSlug)):null;
 return <InitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial} flow={flow}/>;
}
