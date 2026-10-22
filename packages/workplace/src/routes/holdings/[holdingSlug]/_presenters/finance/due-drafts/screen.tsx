import {FeaturePage} from "@erpx/feature-shared/components/features/feature-page";
import {FinanceWorkspace} from "@erpx/feature-finance/components/finance-workspace";
import {FinanceDueControls} from "@erpx/feature-finance/components/finance-due-controls";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {financeDueWorkspace} from "@erpx/server-finance/public/finance-due-drafts";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
export const dynamic="force-dynamic";
export default async function FinanceDuePage({params}:{params:Promise<{holdingSlug:string}>}){const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),rows=await financeDueWorkspace(actor);return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} active="work" title="Scheduled Finance drafts" description="Due work, retained source versions and exceptions to resolve before review"><FinanceDueControls base={base} rows={rows} canWrite={hasPermission(actor,'finance.write')}/></FinanceWorkspace></FeaturePage>;}
