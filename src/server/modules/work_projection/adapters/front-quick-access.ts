import "server-only";
import {frontCompanyApp} from "../public/front-app";
import {workspaceChoices} from "../../organization/public/workspace-choices";
import {frontActiveCompanies} from "../../organization/public/front-company-options";
import {holdingBindingBySlug,holdingResources} from "../../runtime/public/runtime";
import {createFrontQuickAccessService} from "../application/front-quick-access";
export {frontActionItems,frontCompanyItems} from "../domain/front-quick-access";
export const {frontCompanyChoices}=createFrontQuickAccessService({workspaces:workspaceChoices,frontCompanyApp,activeCompanies:async (holding,ids)=>{const binding=await holdingBindingBySlug(holding);const {write}=await holdingResources(binding.id);return frontActiveCompanies(write,ids);}});
