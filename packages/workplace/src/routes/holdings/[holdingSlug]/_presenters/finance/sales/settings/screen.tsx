import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import Link from "@erpx/workplace/components/workspace-link";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { SalesDefaultsControls } from "@erpx/feature-finance/components/sales-controls";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import {salesDefaultsWorkspace} from '@erpx/server-finance/public/sales';
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
export const dynamic = "force-dynamic";
export default async function SalesSettings({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params, actor=await requireWorkContext(holdingSlug), base=workspaceBase(publicScope(actor));
  const data=await salesDefaultsWorkspace(actor).catch(error=>pageError(error,holdingSlug));
  return <FeaturePage className="ui-page-content"><FinanceWorkspace base={base} active="finance-sales" title="Sales settings" description="Finance approves the accounts used by customer documents."><p><Link href={canonicalUiHref(`${base}/finance/sales`)}>Sales & collections</Link> · <Link href={canonicalUiHref(`${base}/finance/setup`)}>Ledgers, accounts & periods</Link> · <Link href={canonicalUiHref(`${base}/commerce?view=products`)}>Items & services</Link></p><p>Quotations, invoices and receipts receive automatic Company and calendar-year numbers. Existing document numbers are retained.</p>{data.issues.map(issue=><InlineNotice key={issue.currency} tone="warning">{issue.currency}: {issue.message} <Link href={canonicalUiHref(`${base}/finance/setup`)}>Review accounts</Link></InlineNotice>)}<SalesDefaultsControls base={base} editions={data.editions} charts={data.options.charts} accounts={data.options.accounts} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canReview={hasPermission(actor,"finance.approve")} /></FinanceWorkspace></FeaturePage>;
}
