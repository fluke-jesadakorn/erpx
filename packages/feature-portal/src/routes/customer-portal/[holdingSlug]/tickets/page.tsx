import {publicHoldingPath} from '@erpx/feature-runtime/lib/holding-paths';
import {SupportTicketPortal} from '@erpx/feature-customer_authority/components/support-ticket-portal';
import {supportTicketPortalProjectionRequest} from '@erpx/server-support/public/ticket-portal';
import {redirect} from 'next/navigation';
import {ApiError} from '@erpx/server-runtime/public/http';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,initial=await supportTicketPortalProjectionRequest(holdingSlug).catch(error=>{if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401)redirect('/login?'+new URLSearchParams({next:`${new URL(publicHoldingPath(holdingSlug,'customer-portal'),'https://path.invalid').pathname}/tickets`}));throw error;});return <SupportTicketPortal holdingSlug={holdingSlug} initial={initial}/>;
}
