import {headers} from 'next/headers';
import {ApiError} from '@erpx/server-runtime/public/http';
import {nativePartsStock,nativePartsSetup,nativePartsPrerequisites} from '@erpx/feature-field_service/public/native-parts-setup';
import {partsSetupContext} from '../../../../../../../lib/parts-setup-context';
import {ServiceParts} from '@erpx/feature-field_service/components/parts-setup';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),incoming=await headers(),context=partsSetupContext(actor);
 const [mapping,prerequisites,workspace]=await Promise.all([
  nativePartsSetup(incoming,holdingSlug,context,'GET').then(value=>{if(!value)throw new ApiError(503,'NATIVE_PARTS_SETUP_UNAVAILABLE','Service parts preparation is unavailable.');return value;}),
  nativePartsPrerequisites(incoming,holdingSlug,context,'read').then(value=>{if(!value)throw new ApiError(503,'NATIVE_PARTS_SETUP_UNAVAILABLE','Service parts preparation is unavailable.');return value;}),
  nativePartsStock(incoming,holdingSlug,context,'read').then(value=>{if(!value)throw new ApiError(503,'NATIVE_PARTS_SETUP_UNAVAILABLE','Service parts preparation is unavailable.');return value;}),
 ]).catch(error=>pageError(error,holdingSlug));
 return <ServiceParts base={workspaceBase(publicScope(actor))} nativeMapping nativePrerequisites nativeStock nativeContext={context} initial={{mapping,prerequisites,workspace}}/>;
}
