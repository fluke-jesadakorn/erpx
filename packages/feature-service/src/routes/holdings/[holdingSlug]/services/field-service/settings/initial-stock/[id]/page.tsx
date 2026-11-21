import {headers} from 'next/headers';
import {nativeInitialCount,NativeInitialCountError} from '@erpx/feature-field_service/public/native-initial-count';
import {partsSetupContext} from '../../../../../../../../lib/parts-setup-context';
import {ApiError} from '@erpx/server-runtime/public/http';
import {ServiceInitialPhysicalStockCount} from '@erpx/feature-field_service/components/initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),incoming=await headers(),nativeContext=partsSetupContext(actor);
 {const failure=(error:unknown)=>pageError(error instanceof NativeInitialCountError?new ApiError(error.status,error.code,error.message):error,holdingSlug),initial=await nativeInitialCount(incoming,holdingSlug,nativeContext,{kind:'record',id}).catch(failure);if(!initial)throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');const flow=initial.available&&initial.record?await nativeInitialCount(incoming,holdingSlug,nativeContext,{kind:'flow',id}).catch(failure):null;if(initial.available&&initial.record&&!flow)throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial Count Flow is unavailable.');return <ServiceInitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial} flow={flow} nativeCount nativeContext={nativeContext}/>;}
}
