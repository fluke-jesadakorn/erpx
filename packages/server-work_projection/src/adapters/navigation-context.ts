import "server-only";
import { executiveVisible } from "@erpx/server-runtime/public/executive/access";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { administrationAccess } from "@erpx/server-runtime/public/administration";
import { publicScope, type WorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { financeWorkspaceV2Enabled } from "@erpx/server-finance/public/finance-presentation";
import { companyFeatureCatalogue } from "@erpx/server-pms/public/hospitality-rollout";
import { availableNavigationAppIds } from "@erpx/feature-work_projection/lib/workplace-catalogue";
import {createNavigationContextService} from "@erpx/server-work_projection/application/navigation-context";
export async function navigationContext(actor:WorkContext){let write:Awaited<ReturnType<typeof holdingResources>>["write"];
 return createNavigationContextService({administration:administrationAccess,prepare:async actor=>{write=(await holdingResources(actor.holdingId)).write;},base:actor=>workspaceBase(publicScope(actor)),production:process.env.NODE_ENV==="production",availableApps:async actor=>availableNavigationAppIds(await companyFeatureCatalogue(actor)),financeV2:financeWorkspaceV2Enabled,executiveVisible:actor=>executiveVisible(write,actor)})(actor);
}
