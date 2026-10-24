import { servicePurchaseRuntime, executeServiceCommand } from "@erpx/server-field_service/adapters/purchase-boundary";
import { fieldServiceAssetReferencePredicate } from "@erpx/server-assets/public/field-service-references";
import { finalRootOwnerProjectionSources as assetsProjectionSources } from "@erpx/server-assets/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createAgreementApplication, type AgreementPorts} from "@erpx/server-field_service/application/agreement";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import {withServicePurchasedCommand} from "@erpx/server-field_service/adapters/purchases";
import {createAgreementSchema} from "@erpx/server-field_service/domain/agreement";
export async function createAgreement(a:WorkContext,raw:unknown,key:string){
return withServicePurchasedCommand(a,()=>({key,operation:"field.agreement.create",target:{kind:"agreement",id:`service-agreement-${key}`},creating:true,asset:Boolean(createAgreementSchema.parse(raw).assetId)}),provided=>createAgreementIn(a,raw,key,provided));
}
async function createAgreementIn(a:WorkContext,raw:unknown,key:string,provided?:SqlDatabase){
let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=(holdingResources(a.holdingId).then(x=>x.write));
const purchase = servicePurchaseRuntime(a, database);
const ports:AgreementPorts={clock:{now:()=>Date.now()},branchId:a.branchId,departmentId:a.departmentId,async authorize(){requirePermission(a,"records.write");if(!hasModuleAccess(a.modules,"field_service"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Field Service.");await purchase.requireCurrentRead();await purchase.requirePurchase("service.agreements");},
replay:purchase.replay,
async persist(command){const write=await database();const {key,id,operation:op,input,branch,department,now}=command;const money=await snapshotMoney(write,a,{sourceType:"field_service_agreement",sourceId:id,amountKind:"visit_rate",transactionDate:input.startDate,currency:input.currency,currencyScale:2,amountUnits:String(input.visitRateMinor)});
  return executeServiceCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status: "draft" },
    [
      money.statement,
      assertion(
        write,
        key,
        "branch",
        `EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind IN ('branch','headquarters') AND status='active')`,
        [branch, a.companyId],
      ),
      ...(department
        ? [
            assertion(
              write,
              key,
              "department",
              `EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind='department' AND branch_id=? AND status='active')`,
              [department, a.companyId, branch],
            ),
          ]
        : []),
      assertion(
        write,
        key,
        "customer",
        `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND currency=?)`,
        [input.customerId, a.companyId, input.currency],
      ),
      ...(input.assetId
        ? [
            assertion(
              write,
              key,
              "asset",
              `EXISTS(SELECT 1 FROM (${assetsProjectionSources.operational_assets}) a WHERE a.id=? AND a.company_id=? AND a.branch_id=? AND ${fieldServiceAssetReferencePredicate()})`,
              [input.assetId, a.companyId, branch, a.departmentId, a.departmentId],
            ),
          ]
        : []),
      write
        .prepare(
          "INSERT INTO field_service_agreements(id,company_id,branch_id,department_id,customer_id,asset_id,title,start_date,end_date,currency,visit_rate_minor,warranty_discount_bps,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          a.companyId,
          branch,
          department,
          input.customerId,
          input.assetId ?? null,
          input.title,
          input.startDate,
          input.endDate,
          input.currency,
          input.visitRateMinor,
          input.warrantyDiscountBps,
          a.userId,
          now,
          now,
        ),
      write
        .prepare(
          "INSERT INTO field_service_agreement_events VALUES(?,?,1,'create',?,'Agreement drafted for independent review','draft',?)",
        )
        .bind(`service-agreement-event-${key}`, id, a.userId, now),
    ],
    Boolean(input.assetId),
  );
}
};try{return await createAgreementApplication(ports)(raw,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
