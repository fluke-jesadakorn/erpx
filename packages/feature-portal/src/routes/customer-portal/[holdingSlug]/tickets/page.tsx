import {publicHoldingPath} from '@erpx/feature-runtime/lib/holding-paths';
import {SupportTicketPortal} from '@erpx/feature-customer_authority/components/support-ticket-portal';
import {createNativeCustomerTicketPortal} from '../../../../adapters/server/native-customer-ticket-portal';
import {headers} from 'next/headers';
import {redirect} from 'next/navigation';
import {ApiError} from '@erpx/server-runtime/public/http';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,config=process.env.ERPX_NATIVE_SUPPORT_TICKET_PORTAL_CONFIG;
 if(!config)throw new ApiError(503,'NATIVE_CUSTOMER_TICKET_PORTAL_UNAVAILABLE','The native customer ticket-portal service is unavailable.');
 const initial=await createNativeCustomerTicketPortal(config).projection(await headers(),holdingSlug).catch(error=>{if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401)redirect('/login?'+new URLSearchParams({next:`${new URL(publicHoldingPath(holdingSlug,'customer-portal'),'https://path.invalid').pathname}/tickets`}));throw error;});return <SupportTicketPortal holdingSlug={holdingSlug} initial={initial}/>;
}
