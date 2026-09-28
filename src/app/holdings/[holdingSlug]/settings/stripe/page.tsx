import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {holdingResources} from '@/server/runtime';
import {hasPermission} from '@/lib/permissions';
import {workspaceBase} from '@/lib/workspace';
import {accessPage} from '@/server/page-error';
import {CompanyStripe} from '@/components/finance/company-stripe';

export const dynamic='force-dynamic';
export default async function StripeSettings({params}:PageProps<'/holdings/[holdingSlug]/settings/stripe'>){
 const actor=await requireWorkContext((await params).holdingSlug);
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'pos.payments.configure')&&!hasPermission(actor,'finance.approve'))return accessPage('permission',actor.holdingSlug);
 const base=workspaceBase(publicScope(actor)),{write}=await holdingResources(actor.holdingId);
 const connection=await write.prepare('SELECT * FROM pos_connections WHERE company_id=?').bind(actor.companyId).first();
 const jobs=(await write.prepare("SELECT id,kind,state,last_error,payload_json FROM pos_jobs WHERE company_id=? AND kind IN ('connect','connect_refresh') ORDER BY created_at DESC LIMIT 10").bind(actor.companyId).all()).results;
 return <div className="content-stack"><Link href={`${base}/settings`}>← Settings</Link><h1>Company Stripe</h1><CompanyStripe base={base} connection={connection as Parameters<typeof CompanyStripe>[0]['connection']} jobs={jobs as Parameters<typeof CompanyStripe>[0]['jobs']} liveEnabled={process.env.POS_STRIPE_LIVE_ENABLED==='true'}/></div>;
}
