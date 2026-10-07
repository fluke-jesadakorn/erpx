import "server-only";

import { holdingBindingBySlug, holdingResources } from "@/server/modules/runtime/public/runtime";
import { workspaceChoices } from "@/server/modules/organization/public/workspace-choices";
import { currentSession, requireAccountAccess } from "@/server/modules/identity_access/public/authorization";
import { ApiError } from "@/server/modules/runtime/public/http";
import { frontOrigin } from "@/lib/front-host";
import { iamRoles } from "@/server/modules/identity_access/public/iam-store";
import { readWorkSelection, selectWorkRole } from "../public/work-role-store";
import {publishedBrandIdentity} from "@/server/modules/brand/public/brand-identity";


import type {SqlDatabase,FileStore} from "../../runtime/public/runtime/types";
import {createFrontAppService,type FrontFacts} from "../application/front-app";
import {frontCompanyFacts} from "../../organization/public/front-company-facts";
import {frontLogoFacts} from "../../files/public/front-logo-facts";
export {frontOperationAllowed,frontPmsSections,frontCookieName,frontLocationScope,frontRouteForLocation} from "../domain/front-app";
function bind(){let write:SqlDatabase,files:FileStore;const service=createFrontAppService({error:(status,code,message)=>new ApiError(status,code,message),isApiError:(error):error is ApiError=>error instanceof ApiError,now:Date.now,prepareSlug:holdingBindingBySlug,prepareHolding:async id=>{({write,files}=await holdingResources(id));},accountAccess:requireAccountAccess,roles:(company,ids)=>iamRoles(write,company,ids),selection:(session,user,company)=>readWorkSelection(write,session,user,company),select:input=>selectWorkRole(write,input),publishedBrand:company=>publishedBrandIdentity(write,company),workspaces:workspaceChoices,currentUserId:async()=>{const {session}=await currentSession();return session.user.id;},file:key=>files.get(key),origin:frontOrigin,facts:{companyBrand:(...args:Parameters<FrontFacts["companyBrand"]>)=>frontCompanyFacts(write).companyBrand(...args),branch:(...args:Parameters<FrontFacts["branch"]>)=>frontCompanyFacts(write).branch(...args),department:(...args:Parameters<FrontFacts["department"]>)=>frontCompanyFacts(write).department(...args),branchDepartment:(...args:Parameters<FrontFacts["branchDepartment"]>)=>frontCompanyFacts(write).branchDepartment(...args),logoPublication:(...args:Parameters<FrontFacts["logoPublication"]>)=>frontLogoFacts(write).logoPublication(...args),actorCompany:(...args:Parameters<FrontFacts["actorCompany"]>)=>frontCompanyFacts(write).actorCompany(...args)}});return {service,db:()=>write};}
export async function frontRoleState(...args:Parameters<ReturnType<typeof createFrontAppService>["frontRoleState"]>){const bound=bind();const result=await bound.service.frontRoleState(...args);return {...result,write:bound.db()};}
export async function ensureFrontRole(...args:Parameters<ReturnType<typeof createFrontAppService>["ensureFrontRole"]>){const bound=bind();const result=await bound.service.ensureFrontRole(...args);return {...result,write:bound.db()};}
export async function frontCompanyBrand(...args:Parameters<ReturnType<typeof createFrontAppService>["frontCompanyBrand"]>){const bound=bind();const result=await bound.service.frontCompanyBrand(...args);return result;}
export async function frontCompanyApp(...args:Parameters<ReturnType<typeof createFrontAppService>["frontCompanyApp"]>){const bound=bind();const result=await bound.service.frontCompanyApp(...args);return result;}
export async function frontScopeFromLocation(...args:Parameters<ReturnType<typeof createFrontAppService>["frontScopeFromLocation"]>){const bound=bind();const result=await bound.service.frontScopeFromLocation(...args);return result;}
export async function storefrontLogo(...args:Parameters<ReturnType<typeof createFrontAppService>["storefrontLogo"]>){const bound=bind();const result=await bound.service.storefrontLogo(...args);return result;}
export async function frontWorkspaceHref(...args:Parameters<ReturnType<typeof createFrontAppService>["frontWorkspaceHref"]>){const bound=bind();const result=await bound.service.frontWorkspaceHref(...args);return result;}
