import "server-only";

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";

import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {PlanningContext} from "@erpx/server-planning/public/capacity";
import {BusinessRuleError} from '@erpx/server-shared/index';import {createPlanningResourceApplication,type PlanningResourceRepository} from '@erpx/server-planning/application/resources';import {planningResourceSources as owner0} from '@erpx/server-organization/public/planning-resource-sources';
import {planningResourceSources as owner1} from '@erpx/server-commerce/public/planning-resource-sources';async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function application(db:SqlDatabase){const repository:PlanningResourceRepository<SqlStatement>={guardResourceVersion(key:string,value:"resource",id:string,companyId:string,branchId:string,expectedVersion:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND branch_id=? AND version=?)", [id, companyId, branchId, expectedVersion]);},
guardNoActiveAllocation(key:string,value:"active-resource-allocation",id:string){return assertion(db, key, value, "NOT EXISTS(SELECT 1 FROM planning_resource_allocations WHERE resource_id=? AND released_at IS NULL)", [id]);},
prepareResourceRevision(code:string,name:string,dailyMinutes:number,value:string,status:"active" | "archived",now:number,id:string){return db.prepare("UPDATE production_resources SET code=?,name=?,daily_minutes=?,work_weekdays_json=?,status=?,version=version+1,updated_at=? WHERE id=?")
        .bind(code, name, dailyMinutes, value, status, now, id);},
prepareResource(resourceId:string,companyId:string,branchId:string,code:string,name:string,dailyMinutes:number,value:string,status:"active" | "archived",userId:string,now:number,now2:number){return db.prepare(`INSERT INTO production_resources(id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,status,version,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,1,?,?,?)`).bind(resourceId, companyId, branchId, code, name, dailyMinutes, value, status, userId, now, now2);},
lockResource(id:string){return db.prepare("SELECT id FROM production_resources WHERE id=? FOR UPDATE").bind(id);},
guardActiveResourceVersion(key:string,value:"resource",id:string,companyId:string,resourceVersion:number,branchId:string | null,branchId2:string | null){return assertion(db, key, value, "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active' AND version=? AND (?::text IS NULL OR branch_id=?))", [id, companyId, resourceVersion, branchId, branchId2]);},
guardCalendarVersion(key:string,value:"calendar-version",id:string,workDate:string,expectedVersion:number){return assertion(db, key, value, "COALESCE((SELECT version FROM production_resource_calendar WHERE resource_id=? AND work_date=?),0)=?", [id, workDate, expectedVersion]);},
guardReleasedCapacity(key:string,value:"released-capacity",id:string,workDate:string,availableMinutes:number){return assertion(db, key, value, "COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)<=?", [id, workDate, availableMinutes]);},
prepareCalendar(id:string,workDate:string,availableMinutes:number,reason:string,userId:string,now:number){return db.prepare(`INSERT INTO production_resource_calendar(resource_id,work_date,available_minutes,reason,version,updated_by,updated_at)
      VALUES(?,?,?,?,1,?,?) ON CONFLICT(resource_id,work_date) DO UPDATE SET available_minutes=excluded.available_minutes,reason=excluded.reason,version=production_resource_calendar.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at`)
      .bind(id, workDate, availableMinutes, reason, userId, now);},
lockAllocatedResource(resourceId:string){return db.prepare("SELECT id FROM production_resources WHERE id=? FOR UPDATE").bind(resourceId);},
guardAllocatedResourceVersion(key:string,value:string,resourceId:string,companyId:string,resourceVersion:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND version=? AND status='active')", [resourceId, companyId, resourceVersion]);},
guardAllocatedCalendarVersion(key:string,value:string,resourceId:string,workDate:string,calendarVersion:number){return assertion(db, key, value, "COALESCE((SELECT version FROM production_resource_calendar WHERE resource_id=? AND work_date=?),0)=?", [resourceId, workDate, calendarVersion]);},...owner0(db),...owner1(db)};return createPlanningResourceApplication({repository,clock:{now:()=>Date.now()},requirePermission:(...args)=>{try{return requirePermission(...args);}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}},commands:{replay:(...args)=>inward(()=>replayCommand(db,...args)),commit:(...args)=>inward(()=>executeCommand(db,...args))}});}export async function saveProductionResource(a: WorkContext,id: string | null,raw: unknown,key: string){const {write:db}=await holdingResources(a.holdingId);const i=resourceSchema.parse(raw);return withManufacturingPurchasedCommand(db,a,{key,operation:id?"planning.resource.update":"planning.resource.create",target:{kind:"resource",id:id??`production-resource-${key}`},purpose:id&&i.status==="archived"?"settle-existing":"new-work",permission:"records.review",module:"planning",existing:Boolean(id)},tx=>exposed(()=>application(tx).saveProductionResource(a,id,i,key)));}
export async function saveProductionResourceCalendar(a: WorkContext,id: string,raw: unknown,key: string){const {write:db}=await holdingResources(a.holdingId);const i=calendarSchema.parse(raw);return withManufacturingPurchasedCommand(db,a,{key,operation:"planning.resource.calendar",target:{kind:"resource",id},purpose:"new-work",permission:"records.review",module:"planning",existing:true},tx=>exposed(()=>application(tx).saveProductionResourceCalendar(a,id,i,key)));}
export function planningContextGuards(db: SqlDatabase,a: WorkContext,key: string,context: PlanningContext){return application(db).planningContextGuards(a,key,context);}


import {resourceSchema,calendarSchema} from "@erpx/server-planning/domain/resources";
import {withManufacturingPurchasedCommand} from "@erpx/server-manufacturing/public/purchases";
