import "server-only";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {holdingBindingBySlug,holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {directoryAssignmentActive,readArchivedPeopleDirectory,readDirectorySelection,readHoldingPeopleDirectory,type DirectorySelection} from "@erpx/server-people_payroll/public/people/directory-repository";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createHoldingDirectoryApplication} from "@erpx/server-organization/application/holding-directory";
export type {HoldingDirectoryCompany,HoldingDirectoryNode,HoldingDirectoryLevel,HoldingDirectoryPosition,HoldingDirectoryPerson,HoldingPeopleDirectory} from "@erpx/server-organization/domain/holding-directory";
function application(actor:WorkContext){let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);return createHoldingDirectoryApplication({actor,canManagePeople:hasPermission(actor,'people.manage'),bindingId:async()=>(await holdingBindingBySlug(actor.holdingSlug)).id,assignmentActive:async()=>directoryAssignmentActive(await database(),actor),async authorizeManagePeople(){requirePermission(actor,'people.manage');},directory:async()=>readHoldingPeopleDirectory(await database()),archived:async()=>readArchivedPeopleDirectory(await database(),actor),selection:async(selection:DirectorySelection,archived:boolean)=>readDirectorySelection(await database(),actor,selection,archived)});}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const holdingPeopleDirectory=(a:WorkContext)=>invoke(()=>application(a).directory());
export const archivedPeopleDirectory=(a:WorkContext)=>invoke(()=>application(a).archived());
export const holdingDirectorySelection=(a:WorkContext,selection:DirectorySelection,archived=false)=>invoke(()=>application(a).selection(selection,archived));
