import {headers} from 'next/headers';
import {nativePartsSetup,nativePartsSetupSelected} from '@erpx/feature-field_service/public/native-parts-setup';
import {partsSetupContext} from '../../../../../../../lib/parts-setup-context';
import {ServiceParts} from '@erpx/feature-field_service/components/parts-setup';
import {servicePartsPostingSetup,servicePartsStockWorkspace} from '@erpx/server-commerce/public/service-parts-posting-setup';
import {servicePartsPrerequisites} from '@erpx/server-finance/public/service-parts-prerequisites';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),incoming=await headers(),context=partsSetupContext(actor);
 const nativeMapping=nativePartsSetupSelected(incoming,holdingSlug,context);
 const [mapping,prerequisites,workspace]=await Promise.all([nativeMapping?nativePartsSetup(incoming,holdingSlug,context,'GET').then(value=>{if(!value)throw Error('Native Service parts setup is unavailable.');return value;}):servicePartsPostingSetup(actor),servicePartsPrerequisites(actor),servicePartsStockWorkspace(actor)]).catch(error=>pageError(error,holdingSlug));
 return <ServiceParts base={workspaceBase(publicScope(actor))} nativeMapping={nativeMapping} nativeContext={nativeMapping?context:undefined} initial={{mapping,prerequisites,workspace}}/>;
}
