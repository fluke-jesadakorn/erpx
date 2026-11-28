import "server-only";
import { executiveVisible } from "@erpx/platform-runtime-server/public/executive/access";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { administrationAccess } from "@erpx/platform-runtime-server/public/administration";
import { publicScope, type WorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { financeWorkspaceV2Enabled } from "@erpx/finance-server/public/finance-presentation";
import { companyFeatureCatalogue } from "@erpx/pms-server/public/hospitality-rollout";
import { availableNavigationAppIds } from "@erpx/work-projection-ui/lib/workplace-catalogue";
import {createNavigationContextService} from "@erpx/work-projection-server/application/navigation-context";
export async function navigationContext(actor:WorkContext){let write:Awaited<ReturnType<typeof holdingResources>>["write"];
 return createNavigationContextService({administration:administrationAccess,prepare:async actor=>{write=(await holdingResources(actor.holdingId)).write;},base:actor=>workspaceBase(publicScope(actor)),production:process.env.NODE_ENV==="production",availableApps:async actor=>availableNavigationAppIds(await companyFeatureCatalogue(actor)),financeV2:financeWorkspaceV2Enabled,executiveVisible:actor=>executiveVisible(write,actor)})(actor);
}
