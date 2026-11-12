import {withAssetPurchasedRead,withAssetPurchasedCommand,assetPurchaseControls} from "@erpx/server-assets/adapters/purchases";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as maintenanceProjectionSources } from "@erpx/server-maintenance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import 'server-only';
import {createAssetRegistration} from '@erpx/server-assets/application/registration';
import {createAssetApplication, type AssetRepository} from '@erpx/server-assets/application/assets';
import {createAssetSchema, type Asset} from '@erpx/server-assets/domain/assets';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import {ApiError} from '@erpx/server-runtime/public/http';
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import {currencyScale,decimalUnits} from "@erpx/feature-finance/lib/company-money";
const assetScope="a.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?)",args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const reviewer=(alias:'a'|'w')=>`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=${alias}.company_id AND x.user_id=? AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.review') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=${alias}.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
function domain(a:WorkContext,_module:'assets',write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'assets'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access assets.');}
async function createAssetOwned(a: WorkContext, raw: unknown, key: string,provided?:SqlDatabase) {
  return createAssetRegistration({
    authorize:()=>domain(a,'assets',true),authorizeFinanceLink:()=>requirePermission(a,'finance.write'),parse:raw=>createAssetSchema.parse(raw),scope:a,clock:{now:()=>Date.now()},decimalUnits,error:(status,code,message)=>new ApiError(status,code,message),
    repository:async()=>{
      const write=provided??(await holdingResources(a.holdingId)).write;let money:Awaited<ReturnType<typeof snapshotMoney>>|undefined;
      return {
        replay:command=>replayCommand(write,a,command.key,command.operation,command.id,command.input),
        currencyScale:async currency=>currencyScale(currency,(await write.prepare(`SELECT scale FROM (${financeProjectionSources.company_currency_scales}) company_currency_scales WHERE company_id=? AND currency=?`).bind(a.companyId,currency).first<{scale:number}>())?.scale),
        prepareAcquisitionCost:async(command,costMinor)=>{money=await snapshotMoney(write,a,{sourceType:'operational_asset',sourceId:command.id,amountKind:'acquisition_cost',transactionDate:command.input.acquiredDate,currency:command.input.currency,amountUnits:String(costMinor)});},
        register:async(command,{branch,department,costMinor,now})=>{
          if(!money)throw new Error('ASSET_ACQUISITION_COST_NOT_PREPARED');
          const input=command.input,id=command.id,key=command.key,op=command.operation;
  return executeCommand(write, a, key, op, input, id, 1, { id, version: 1 }, [
    assertion(write, key, 'branch', `EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind IN ('branch','headquarters') AND status='active')`, [branch, a.companyId]),
    ...(department ? [assertion(write, key, 'department', `EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND kind='department' AND status='active')`, [department, a.companyId, branch])] : []),
    write.prepare('INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)')
      .bind(id, a.companyId, branch, department, input.code, input.name, input.category, input.currency, costMinor!, input.acquiredDate, input.warrantyEnd ?? null, a.userId, now, now),
    money.statement,
    ...(input.financeAssetId ? [
      write.prepare(`SELECT id FROM (${financeProjectionSources.finance_assets}) finance_assets WHERE id=? FOR SHARE`).bind(input.financeAssetId),
      assertion(write,key,'finance-source',`EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_assets}) finance_assets WHERE id=? AND company_id=? AND version=? AND status='in_service' AND cost_minor=? AND currency=?)`,[input.financeAssetId,a.companyId,input.financeAssetVersion!,costMinor!,input.currency]),
      write.prepare('UPDATE operational_assets SET finance_asset_id=?,finance_asset_version=? WHERE id=?').bind(input.financeAssetId,input.financeAssetVersion!,id),
      write.prepare('INSERT INTO operational_asset_finance_links VALUES(?,?,?,1,?,?,?,?,?,?)').bind(`asset-finance-link-${key}`,id,input.financeAssetId,input.financeAssetVersion!,costMinor!,input.currency,'Finance recognition linked at physical registration',a.userId,now),
    ] : []),
    write.prepare("INSERT INTO operational_asset_events VALUES(?,?,1,'register',?,'Asset registered','good','registered',NULL,NULL,?)").bind(`asset-event-${key}`, id, a.userId, now),
  ]);
        },
      };
    },
  })(raw,key);
}
function bindRepository(write: SqlDatabase, a: WorkContext): AssetRepository {
 return {
  items:async()=>(await write.prepare(`SELECT a.*,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=a.company_id AND user_id=a.custodian_id AND merged_into_id IS NULL LIMIT 1) custodian_name,CASE WHEN ${reviewer('a')} THEN 1 ELSE 0 END can_review FROM operational_assets a WHERE ${assetScope} ORDER BY a.updated_at DESC,a.id`).bind(a.userId,...args(a)).all<Asset>()).results,
  people:async()=>(await write.prepare(`SELECT DISTINCT p.user_id id,p.legal_name name FROM (${people_payrollProjectionSources.hr_people}) p JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id WHERE p.company_id=? AND p.user_id IS NOT NULL AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' ORDER BY p.legal_name`).bind(a.companyId).all<Row>()).results,
  nodes:async()=>(await write.prepare(`SELECT id,kind,display_name,branch_id FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') ORDER BY kind,display_name`).bind(a.companyId).all<Row>()).results,
  financeCandidates:async()=>(await write.prepare(`SELECT f.id,f.asset_number,f.name,f.currency,f.cost_minor,f.version,f.status FROM (${financeProjectionSources.finance_assets}) f WHERE f.company_id=? AND f.status IN ('in_service','retired') AND NOT EXISTS(SELECT 1 FROM operational_assets a WHERE a.finance_asset_id=f.id) ORDER BY f.asset_number`).bind(a.companyId).all<Row>()).results,
  finance:id=>write.prepare(`SELECT id,asset_number,status,currency,cost_minor,accumulated_depreciation_minor,version FROM (${financeProjectionSources.finance_assets}) finance_assets WHERE id=? AND company_id=?`).bind(id,a.companyId).first<Row>(),
  history:async id=>(await write.prepare('SELECT * FROM operational_asset_events WHERE asset_id=? ORDER BY version').bind(id).all<Row>()).results,
  maintenance:async id=>(await write.prepare(`SELECT id,title,status,priority,owner_id,scheduled_at,version FROM (${maintenanceProjectionSources.maintenance_work_orders}) maintenance_work_orders WHERE asset_id=? ORDER BY updated_at DESC,id`).bind(id).all<Row>()).results,
 };
}
function application(a:WorkContext,provided?:SqlDatabase){return createAssetApplication({actorId:a.userId,authorize:write=>domain(a,'assets',write),canReadFinance:()=>hasPermission(a,'finance.read'),canWrite:()=>hasPermission(a,'records.write'),canWriteFinance:()=>hasPermission(a,'finance.write'),error:(status,code,message)=>new ApiError(status,code,message),repository:async()=>bindRepository(provided??(await holdingResources(a.holdingId)).write,a)});}
export async function assetWorkspace(a:WorkContext){return withAssetPurchasedRead(a,"assets.register",async db=>({...await application(a,db).assetWorkspace(),purchases:await assetPurchaseControls(a)}));}
export async function assetDetail(a:WorkContext,id:string){return withAssetPurchasedRead(a,"assets.register",async db=>({...await application(a,db).assetDetail(id),purchases:await assetPurchaseControls(a)}));}
export async function createAsset(a:WorkContext,raw:unknown,key:string){const input=createAssetSchema.parse(raw);return withAssetPurchasedCommand(a,{key,operation:"asset.create",capability:"assets.register",purpose:"new-work",target:{kind:"asset",id:`asset-${key}`},creating:true,permissions:["records.write",...(input.financeAssetId?["finance.write"]:[])]},db=>createAssetOwned(a,raw,key,db));}
