import "server-only";
import {frontCompanyApp} from "@erpx/server-work_projection/public/front-app";
import {workspaceChoices} from "@erpx/server-organization/public/workspace-choices";
import {frontActiveCompanies} from "@erpx/server-organization/public/front-company-options";
import {holdingBindingBySlug,holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {createFrontQuickAccessService} from "@erpx/server-work_projection/application/front-quick-access";
export {frontActionItems,frontCompanyItems} from "@erpx/server-work_projection/domain/front-quick-access";
export const {frontCompanyChoices}=createFrontQuickAccessService({workspaces:workspaceChoices,frontCompanyApp,activeCompanies:async (holding,ids)=>{const binding=await holdingBindingBySlug(holding);const {write}=await holdingResources(binding.id);return frontActiveCompanies(write,ids);}});
