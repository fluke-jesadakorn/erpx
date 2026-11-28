import {FeaturePage} from "@erpx/platform-shared-ui/components/features/feature-page";
import {FinanceWorkspace} from "@erpx/finance-ui/components/finance-workspace";
import {FinanceDueControls} from "@erpx/finance-ui/components/finance-due-controls";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";
import {financeDueWorkspace} from "@erpx/finance-server/public/finance-due-drafts";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
export const dynamic="force-dynamic";
export default async function FinanceDuePage({params}:{params:Promise<{holdingSlug:string}>}){const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),rows=await financeDueWorkspace(actor);return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} active="work" title="Scheduled Finance drafts" description="Due work, retained source versions and exceptions to resolve before review"><FinanceDueControls base={base} rows={rows} canWrite={hasPermission(actor,'finance.write')}/></FinanceWorkspace></FeaturePage>;}
