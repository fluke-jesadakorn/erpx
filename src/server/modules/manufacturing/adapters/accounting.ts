import "server-only";

import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {Row} from "@/server/modules/commerce/public/commerce";
import type {SqlDatabase,SqlStatement} from "@/server/modules/runtime/public/runtime/types";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {assertion,executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {prepareJournal} from "@/server/modules/finance/public/finance";

import {snapshotMoney} from "@/server/modules/finance/public/company-money";
import {BusinessRuleError} from '../../shared';import {createManufacturingAccountingApplication,type ManufacturingAccountingRepository} from '../application/accounting';import {manufacturingAccountingSources as owner0} from '../../commerce/public/manufacturing-accounting-sources';
import {manufacturingAccountingSources as owner1} from '../../finance/public/manufacturing-accounting-sources';async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function application(db:SqlDatabase){const repository:ManufacturingAccountingRepository<SqlStatement>={prepareProfile(id:string,companyId:string,chartId:string,inventoryAccountId:string,wipAccountId:string,laborAccountId:string,overheadAccountId:string,scrapAccountId:string,userId:string,now:number,now2:number){return db.prepare(`INSERT INTO manufacturing_accounting_profiles(id,company_id,chart_id,currency,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id,scrap_account_id,status,version,created_by,created_at,updated_at)
      VALUES(?,?,?,'THB',?,?,?,?,?,'draft',1,?,?,?)`)
      .bind(id, companyId, chartId, inventoryAccountId, wipAccountId, laborAccountId, overheadAccountId, scrapAccountId, userId, now, now2);},
async profile(id:string,companyId:string){return db.prepare("SELECT * FROM manufacturing_accounting_profiles WHERE id=? AND company_id=?")
    .bind(id, companyId).first<Row>();},
guardProfileVersion(key:string,value:"profile",id:string,companyId:string,expectedVersion:number,expected:"approved" | "draft",userId:string){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_accounting_profiles WHERE id=? AND company_id=? AND version=? AND status=? AND created_by<>?)", [id, companyId, expectedVersion, expected, userId]);},
prepareProfileReview(status:"approved" | "rejected" | "retired",userId:string,now:number,id:string){return db.prepare("UPDATE manufacturing_accounting_profiles SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?")
      .bind(status, userId, now, id);},
async approvedProfile(companyId:string){return db.prepare("SELECT * FROM manufacturing_accounting_profiles WHERE company_id=? AND status='approved'")
    .bind(companyId).first<Row>();},
guardApprovedProfile(key:string,value:"production-accounting-profile",value2:string,companyId:string,value3:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM manufacturing_accounting_profiles WHERE id=? AND company_id=? AND status='approved' AND version=?)", [value2, companyId, value3]);},
preparePostingEvidence(value:string,companyId:string,orderId:string,orderVersion:number,phase:"output" | "issue" | "report" | "scrap",id:string | number | null,version:string | number | null,id2:string,total:number,value2:string,userId:string,now:number){return db.prepare(`INSERT INTO manufacturing_order_postings(id,company_id,order_id,source_order_version,phase,profile_id,profile_version,journal_id,currency,amount_minor,account_snapshot_json,created_by,created_at)
    VALUES(?,?,?,?,?,?,?,?,'THB',?,?,?,?)`).bind(value, companyId, orderId, orderVersion, phase,
      id, version, id2, total, value2, userId, now);},
prepareCostMovement(id:string,companyId:string,orderId:string,kind:string,value:number,amount:number,sourceId:string,id2:string,entryDate:string,userId:string,now:number){return db.prepare(`INSERT INTO manufacturing_cost_movements(id,company_id,order_id,movement_kind,quantity_milli,amount_minor,source_type,source_id,journal_id,occurred_on,version,created_by,created_at)
      VALUES(?,?,?,?,?,?,'production_event',?,?,?,1,?,?)`)
      .bind(id, companyId, orderId, kind, value, amount, sourceId, id2, entryDate, userId, now);},...owner0(db),...owner1(db)};return createManufacturingAccountingApplication({repository,clock:{now:()=>Date.now()},requirePermission:(...args)=>{try{return requirePermission(...args);}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}},commands:{replay:(...args)=>inward(()=>replayCommand(db,...args)),commit:(...args)=>inward(()=>executeCommand(db,...args))},prepareJournal:(...args)=>inward(()=>prepareJournal(db,...args)),snapshotMoney:(...args)=>inward(()=>snapshotMoney(db,...args))});}export async function createManufacturingAccountingProfile(a: WorkContext,raw: unknown,key: string){const {write:db}=await holdingResources(a.holdingId);return exposed(()=>application(db).createManufacturingAccountingProfile(a,raw,key));}
export async function reviewManufacturingAccountingProfile(a: WorkContext,id: string,raw: unknown,key: string){const {write:db}=await holdingResources(a.holdingId);return exposed(()=>application(db).reviewManufacturingAccountingProfile(a,id,raw,key));}
export async function prepareManufacturingPosting(db: SqlDatabase,a: WorkContext,input: {
  key: string; orderId: string; orderVersion: number; phase: "issue" | "report" | "output" | "scrap";
  entryDate: string; material?: number; labor?: number; overhead?: number; output?: number; quantityMilli?: number;
}){return exposed(()=>application(db).prepareManufacturingPosting(a,input));}

