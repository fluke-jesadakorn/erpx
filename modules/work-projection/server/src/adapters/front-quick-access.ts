import "server-only";
import {frontCompanyApp} from "@erpx/work-projection-server/public/front-app";
import {workspaceChoices} from "@erpx/organization-server/public/workspace-choices";
import {frontActiveCompanies} from "@erpx/organization-server/public/front-company-options";
import {holdingBindingBySlug,holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {createFrontQuickAccessService} from "@erpx/work-projection-server/application/front-quick-access";
export {frontActionIds,frontActionItems,frontCompanyItems} from "@erpx/work-projection-server/domain/front-quick-access";
export const {frontCompanyChoices}=createFrontQuickAccessService({workspaces:workspaceChoices,frontCompanyApp,activeCompanies:async (holding,ids)=>{const binding=await holdingBindingBySlug(holding);const {write}=await holdingResources(binding.id);return frontActiveCompanies(write,ids);}});
