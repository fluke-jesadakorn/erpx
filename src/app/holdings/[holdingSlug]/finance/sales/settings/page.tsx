import Link from "next/link";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { SalesDefaultsControls } from "@/components/finance/sales-controls";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { salesDefaultsWorkspace } from "@/server/modules/finance/public/sales";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { InlineNotice } from "@/components/ui/foundation";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
export const dynamic = "force-dynamic";
export default async function SalesSettings({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params, actor=await requireWorkContext(holdingSlug), base=workspaceBase(publicScope(actor));
  const data=await salesDefaultsWorkspace(actor).catch(error=>pageError(error,holdingSlug));
  return <FeaturePage className="ui-page-content"><FinanceWorkspace base={base} active="finance-sales" title="Sales settings" description="Finance approves the accounts used by customer documents."><p><Link href={`${base}/finance/sales`}>Sales & collections</Link> · <Link href={`${base}/finance/setup`}>Ledgers, accounts & periods</Link> · <Link href={`${base}/commerce?view=products`}>Items & services</Link></p><p>Quotations, invoices and receipts receive automatic Company and calendar-year numbers. Existing document numbers are retained.</p>{data.issues.map(issue=><InlineNotice key={issue.currency} tone="warning">{issue.currency}: {issue.message} <Link href={`${base}/finance/setup`}>Review accounts</Link></InlineNotice>)}<SalesDefaultsControls base={base} editions={data.editions} charts={data.options.charts} accounts={data.options.accounts} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canReview={hasPermission(actor,"finance.approve")} /></FinanceWorkspace></FeaturePage>;
}
