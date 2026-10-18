import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { posPresentationQueries } from "@/server/modules/pos/public/presentation-queries";

import { RecordLink as Link } from "@/components/records/record-link";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";

import {hasPermission} from "@/lib/identity_access/permissions";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {accessPage} from "@/server/modules/work_projection/public/page-error";
import {CompanyStripe} from '@/components/finance/company-stripe';
import { PageContent } from "@/components/ui/foundation";

export const dynamic='force-dynamic';
export default async function StripeSettings({params}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }){
 const actor=await requireWorkContext((await params).holdingSlug);
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'pos.payments.configure')&&!hasPermission(actor,'finance.approve'))return accessPage('permission',actor.holdingSlug);
 const base=workspaceBase(publicScope(actor)),presentation=await posPresentationQueries(actor.holdingId);
 const connection=await presentation.stripeEntrypointFacts().connection(actor.companyId);
 const jobs=(await presentation.stripeEntrypointFacts().jobs(actor.companyId)).results;
 return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/settings`)}>← Settings</Link><h1>Company Stripe</h1><CompanyStripe base={base} commandScope={`${actor.holdingId}:${actor.companyId}:${actor.userId}`} connection={connection as Parameters<typeof CompanyStripe>[0]['connection']} jobs={jobs as Parameters<typeof CompanyStripe>[0]['jobs']} liveEnabled={process.env.POS_STRIPE_LIVE_ENABLED==='true'}/></PageContent>;
}
