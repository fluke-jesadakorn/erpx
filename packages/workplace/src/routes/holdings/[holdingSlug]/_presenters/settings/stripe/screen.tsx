import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { posPresentationQueries } from "@erpx/server-pos/public/presentation-queries";

import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";

import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {accessPage} from "@erpx/server-work_projection/public/page-error";
import {CompanyStripe} from '@erpx/feature-finance/components/company-stripe';
import { PageContent } from "@erpx/ui/components/foundation";

export const dynamic='force-dynamic';
export default async function StripeSettings({params}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }){
 const actor=await requireWorkContext((await params).holdingSlug);
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'pos.payments.configure')&&!hasPermission(actor,'finance.approve'))return accessPage('permission',actor.holdingSlug);
 const base=workspaceBase(publicScope(actor)),presentation=await posPresentationQueries(actor.holdingId);
 const connection=await presentation.stripeEntrypointFacts().connection(actor.companyId);
 const jobs=(await presentation.stripeEntrypointFacts().jobs(actor.companyId)).results;
 return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/settings`)}>← Settings</Link><h1>Company Stripe</h1><CompanyStripe base={base} commandScope={`${actor.holdingId}:${actor.companyId}:${actor.userId}`} connection={connection as Parameters<typeof CompanyStripe>[0]['connection']} jobs={jobs as Parameters<typeof CompanyStripe>[0]['jobs']} liveEnabled={process.env.POS_STRIPE_LIVE_ENABLED==='true'}/></PageContent>;
}
