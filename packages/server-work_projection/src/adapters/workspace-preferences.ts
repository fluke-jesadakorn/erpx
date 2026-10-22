import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {workspacePreferenceFacts} from "@erpx/server-identity_access/public/workspace-preference-facts";
import {createWorkspacePreferences} from "@erpx/server-work_projection/application/workspace-preferences";
const service=createWorkspacePreferences({now:()=>Date.now()});
export const readWorkspacePreference=(db:SqlDatabase,userId:string)=>service.readWorkspacePreference(workspacePreferenceFacts(db),userId);
export const readCompanyPreference=(db:SqlDatabase,userId:string,holdingId:string)=>service.readCompanyPreference(workspacePreferenceFacts(db),userId,holdingId);
export const rememberWorkspace=(db:SqlDatabase,userId:string,path:string,company?:{holdingId:string;companyId:string})=>service.rememberWorkspace(workspacePreferenceFacts(db),userId,path,company);
export const rememberPlatformRole=(db:SqlDatabase,userId:string,assignmentId:string)=>service.rememberPlatformRole(workspacePreferenceFacts(db),userId,assignmentId);
