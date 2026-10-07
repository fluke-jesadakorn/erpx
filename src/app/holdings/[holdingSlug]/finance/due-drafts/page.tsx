import {FeaturePage} from "@/components/features/feature-page";
import {FinanceWorkspace} from "@/components/finance/finance-workspace";
import {FinanceDueControls} from "@/components/finance/finance-due-controls";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {financeDueWorkspace} from "@/server/modules/finance/public/finance-due-drafts";
import {hasPermission} from "@/lib/permissions";
import {workspaceBase} from "@/lib/workspace";
export const dynamic="force-dynamic";
export default async function FinanceDuePage({params}:{params:Promise<{holdingSlug:string}>}){const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),rows=await financeDueWorkspace(actor);return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} active="work" title="Scheduled Finance drafts" description="Due work, retained source versions and exceptions to resolve before review"><FinanceDueControls base={base} rows={rows} canWrite={hasPermission(actor,'finance.write')}/></FinanceWorkspace></FeaturePage>;}
