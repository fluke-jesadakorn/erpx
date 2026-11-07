import {nativeInitialCount,NativeInitialCountError} from '@erpx/feature-field_service/public/native-initial-count';
import {ApiError} from '@erpx/server-runtime/public/http';
import {headers} from 'next/headers';
import {nativePartsStockSelected} from '@erpx/feature-field_service/public/native-parts-setup';
import {partsSetupContext} from '../../../../../../../lib/parts-setup-context';
import {ServiceInitialPhysicalStockCount} from '@erpx/feature-field_service/components/initial-physical-stock-count';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
 const incoming=await headers(),nativeContext=partsSetupContext(actor);
 {const selection=new URLSearchParams();for(const key of ['warehouseId','productId','countDate']){const value=query[key];for(const item of Array.isArray(value)?value:value===undefined?[]:[value])selection.append(key,item);}const initial=await nativeInitialCount(incoming,holdingSlug,nativeContext,{kind:'selection',query:selection.size?`?${selection.toString()}`:''}).catch(error=>pageError(error instanceof NativeInitialCountError?new ApiError(error.status,error.code,error.message):error,holdingSlug));if(!initial)throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');return <ServiceInitialPhysicalStockCount base={workspaceBase(publicScope(actor))} initial={initial} nativeCount nativeContext={nativeContext} nativeStock={nativePartsStockSelected(incoming,holdingSlug,nativeContext)}/>;}
}
