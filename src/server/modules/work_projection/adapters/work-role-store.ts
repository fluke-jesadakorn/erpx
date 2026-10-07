import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
import {createWorkRoleStore} from "../application/work-role-store";
import {workRoleSelectionFacts} from "../../identity_access/public/work-role-selection-facts";
const service=createWorkRoleStore({clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),errorCode:error=>error instanceof ApiError?error.code:undefined});
export function readWorkSelection(db:SqlDatabase,sessionId:string,userId:string,companyId:string){return service.readWorkSelection(workRoleSelectionFacts(db),sessionId,userId,companyId);}
export function selectWorkRole(db:SqlDatabase,input:{sessionId:string;userId:string;companyId:string;assignmentId:string;assignmentVersion:number;revision:number;expiresAt:number;restoring?:boolean;rememberPreference?:boolean}){return service.selectWorkRole(workRoleSelectionFacts(db),input);}
export function restoreWorkSelection(db:SqlDatabase,input:{sessionId:string;userId:string;companyId:string;expiresAt:number},assignments:{id:string;version?:number}[]){return service.restoreWorkSelection(workRoleSelectionFacts(db),input,assignments);}
export function initializeWorkSelection(db:SqlDatabase,input:{sessionId:string;userId:string;companyId:string;expiresAt:number},assignments:{id:string;version?:number}[],eligible:{id:string;version?:number}[]){return service.initializeWorkSelection(workRoleSelectionFacts(db),input,assignments,eligible);}
export type {ActiveWorkSelection,WorkAuthority,SelectWorkRole} from "../domain/work-role-store";
