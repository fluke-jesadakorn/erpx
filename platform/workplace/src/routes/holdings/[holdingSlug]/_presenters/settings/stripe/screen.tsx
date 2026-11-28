import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { posPresentationQueries } from "@erpx/pos-server/public/presentation-queries";

import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";

import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {accessPage} from "@erpx/work-projection-server/public/page-error";
import {CompanyStripe} from '@erpx/finance-ui/components/company-stripe';
import { PageContent } from "@erpx/platform-ui/components/foundation";

export const dynamic='force-dynamic';
export default async function StripeSettings({params}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }){
 const actor=await requireWorkContext((await params).holdingSlug);
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'pos.payments.configure')&&!hasPermission(actor,'finance.approve'))return accessPage('permission',actor.holdingSlug);
 const base=workspaceBase(publicScope(actor)),presentation=await posPresentationQueries(actor.holdingId);
 const connection=await presentation.stripeEntrypointFacts().connection(actor.companyId);
 const jobs=(await presentation.stripeEntrypointFacts().jobs(actor.companyId)).results;
 return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/settings`)}>← Settings</Link><h1>Company Stripe</h1><CompanyStripe base={base} commandScope={`${actor.holdingId}:${actor.companyId}:${actor.userId}`} connection={connection as Parameters<typeof CompanyStripe>[0]['connection']} jobs={jobs as Parameters<typeof CompanyStripe>[0]['jobs']} liveEnabled={process.env.POS_STRIPE_LIVE_ENABLED==='true'}/></PageContent>;
}
