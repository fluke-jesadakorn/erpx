import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {nativeWorkflowReadAll} from "./native-workflow-read";

/** Owner-native reads: both statements are planned by the mounted Workflow
 * wave3 command planner (WF_NATIVE_WORK_REPORT_DECISIONS and
 * WF_NATIVE_WORK_REPORT_SAVED_ROUTES) and executed on the caller's open unit.
 * This module carries no SQL for these reads and keeps the owner result shape. */
export function workReportWorkflowFacts(db:SqlDatabase){return {
  decisions:async (companyId:string,ids:string)=>({results:await nativeWorkflowReadAll<{record_id:string;id:string;action:string;actor:string;at:number;reason:string;step:string;cycle:number;policy:string|null}>(
    'wf_native_work_report_decisions', [companyId, ids], companyId, db)}),
  savedRoutes:async (companyId:string,ids:string)=>({results:await nativeWorkflowReadAll<{record_id:string;cycle:number;ordinal:number;approver_user_id:string;stage:string|null;authority:string;route_snapshot_json:string|null}>(
    'wf_native_work_report_saved_routes', [companyId, ids], companyId, db)}),
};}
