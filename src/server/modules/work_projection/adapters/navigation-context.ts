import "server-only";
import { executiveVisible } from "@/server/modules/runtime/public/executive/access";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { administrationAccess } from "@/server/modules/runtime/public/administration";
import { publicScope, type WorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { financeWorkspaceV2Enabled } from "@/server/modules/finance/public/finance-presentation";
import { companyFeatureCatalogue } from "@/server/modules/pms/public/hospitality-rollout";
import { availableNavigationAppIds } from "@/lib/workplace-catalogue";
import {createNavigationContextService} from "../application/navigation-context";
export async function navigationContext(actor:WorkContext){let write:Awaited<ReturnType<typeof holdingResources>>["write"];
 return createNavigationContextService({administration:administrationAccess,prepare:async actor=>{write=(await holdingResources(actor.holdingId)).write;},base:actor=>workspaceBase(publicScope(actor)),production:process.env.NODE_ENV==="production",availableApps:async actor=>availableNavigationAppIds(await companyFeatureCatalogue(actor)),financeV2:financeWorkspaceV2Enabled,executiveVisible:actor=>executiveVisible(write,actor)})(actor);
}
