import {headers} from 'next/headers';
import {requiredNativeFieldSupportRead,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';
import {pageError} from '@erpx/server-work_projection/public/page-error';
import {partsSetupContext} from '../../../../../../../lib/parts-setup-context';
export const dynamic='force-dynamic';
/** Field entry readback only; canonical changes continue in Field Service. */
export default async function Page({params}:{params:Promise<{holdingSlug:string;ticketId:string}>}){
 const {holdingSlug,ticketId}=await params,actor=await requireWorkContext(holdingSlug);
 const data=await requiredNativeFieldSupportRead(await headers(),holdingSlug,partsSetupContext(actor),ticketId).catch(error=>pageError(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error,holdingSlug));
 return <section className="ui-page-content"><h1>{data.row.title}</h1><p>Support case · {data.row.status} · Version {data.row.version}</p><p style={{whiteSpace:'pre-wrap',overflowWrap:'anywhere'}}>{data.row.issue}</p><p>{data.managedInFieldService?'This case is managed in Field Service. Continue its service order to change its state.':'Support case readback. Continue in the Support workspace to manage this case.'}</p></section>;
}
