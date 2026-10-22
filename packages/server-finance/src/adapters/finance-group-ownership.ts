import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import { groupCompanyPurchases, groupRecordPurchases, groupHistoryPredicate, groupSourceSql, groupRecordHistorySql } from "@erpx/server-finance/adapters/group-purchase-sources";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { financeTransaction } from "@erpx/server-finance/public/finance-transactions";
import { requireFinanceTool } from "@erpx/server-finance/public/finance-company-tools";
import { requireFinanceMoneyScale } from "@erpx/server-finance/public/finance-money-policy";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type Row, ownershipEditionSchema, acquisitionEditionSchema, groupEvidenceDecisionSchema } from '@erpx/server-finance/domain/finance-group-ownership';
import { createFinanceGroupOwnershipApplication, type FinanceGroupOwnershipRepository, type FinanceGroupOwnershipPorts } from '@erpx/server-finance/application/finance-group-ownership';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
export { ownershipEditionSchema } from '@erpx/server-finance/domain/finance-group-ownership';
export { acquisitionEditionSchema } from '@erpx/server-finance/domain/finance-group-ownership';
export { groupEvidenceDecisionSchema } from '@erpx/server-finance/domain/finance-group-ownership';
function nativePorts(provided?: SqlDatabase, owned = false, inheritedHistory?: (company:string)=>string): FinanceGroupOwnershipPorts<SqlStatement> {
    let database = provided!;
    let historySql: (company:string)=>string = inheritedHistory ?? (()=>"TRUE");
    const repository: FinanceGroupOwnershipRepository<SqlStatement> = { readGetRow(id: string, holdingId: string, companyId: string, table: "finance_ownership_interests" | "finance_acquisitions") {
            return database.prepare(groupSourceSql(`SELECT * FROM ${table} WHERE id=? AND holding_id=? AND company_id=?`, [groupRecordHistorySql(table === "finance_ownership_interests" ? "ownership" : "acquisition",table,historySql)]))
                .bind(id, holdingId, companyId)
                .first<Row>();
        },
        readOwnershipSourceEntities(holdingId: string, investorCompanyId: string, investeeCompanyId: string) {
            return database.prepare("SELECT * FROM finance_group_entities WHERE holding_id=? AND company_id IN (?,?) AND status='active' ORDER BY company_id")
                .bind(holdingId, investorCompanyId, investeeCompanyId)
                .all<Row>();
        },
        readOwnershipSourceOverlap(holdingId: string, id: string | number | null, id2: string | number | null, value: string | number, value2: string, effectiveFrom: string) {
            return database.prepare("SELECT 1 FROM finance_ownership_interests WHERE holding_id=? AND investor_entity_id=? AND investee_entity_id=? AND status='approved' AND id<>? AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)")
                .bind(holdingId, id, id2, value, value2, effectiveFrom)
                .first();
        },
        stagePrepareOwnershipEditionEffect(id: string, holdingId: string, companyId: string, value: string | null, id2: string | number | null, id3: string | number | null, method: "subsidiary" | "equity" | "cost" | "excluded", ownershipBps: number, votingBps: number, controlAssessment: string, effectiveFrom: string, value2: string | null, userId: string, now: number, now2: number, hash: string, value3: string, reason: string) {
            return database.prepare("INSERT INTO finance_ownership_interests(id,holding_id,company_id,parent_id,investor_entity_id,investee_entity_id,accounting_method,ownership_bps,voting_bps,control_assessment,effective_from,effective_to,created_by,created_at,updated_at,source_hash,source_json,reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, holdingId, companyId, value, id2, id3, method, ownershipBps, votingBps, controlAssessment, effectiveFrom, value2, userId, now, now2, hash, value3, reason);
        },
        stageDecideOwnershipEditionEffect(end: string, now: number, id: string | number | null, version: string | number | null) {
            return database.prepare("UPDATE finance_ownership_interests SET status='retired',effective_to=?,version=version+1,updated_at=? WHERE id=? AND version=?")
                .bind(end, now, id, version);
        },
        guard6(key: string, value: "ownership-version", id: string, holdingId: string, companyId: string, expectedVersion: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM finance_ownership_interests WHERE id=? AND holding_id=? AND company_id=? AND status='draft' AND version=?)", [id, holdingId, companyId, expectedVersion]);
        },
        stageDecideOwnershipEditionEffect2(value: "approved" | "rejected", userId: string, now: number, id: string) {
            return database.prepare("UPDATE finance_ownership_interests SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, userId, now, id);
        },
        readAcquisitionSourceEffect(companyId: string, ownershipId: string, acquisitionDate: string, currency: string, value: string | number, currentId: string) {
            return database.prepare("SELECT 1 FROM finance_acquisitions WHERE company_id=? AND ownership_id=? AND acquisition_date=? AND currency=? AND source_hash IS NOT NULL AND status IN ('draft','approved','impaired') AND id NOT IN (?,?)")
                .bind(companyId, ownershipId, acquisitionDate, currency, value, currentId)
                .first();
        },
        stagePrepareAcquisitionEditionEffect(calcId: string, companyId: string, holdingId: string, currency: string, calcHash: string, value: string, value2: string, value3: string, userId: string, now: number, now2: number) {
            return database.prepare("INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_version,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,prepared_by,created_at,updated_at) VALUES(?,?,?,'consolidation',1,?,2,?,?,?,?,'prepared',?,?,?)")
                .bind(calcId, companyId, holdingId, currency, calcHash, value, value2, value3, userId, now, now2);
        },
        stagePrepareAcquisitionEditionEffect2(value: string, calcId: string, userId: string, reason: string, value2: string, now: number) {
            return database.prepare("INSERT INTO finance_calculation_events VALUES(?,?,1,'prepare',?,?,?,?)")
                .bind(value, calcId, userId, reason, value2, now);
        },
        stagePrepareAcquisitionEditionEffect3(id: string, holdingId: string, companyId: string, ownershipId: string, investee_entity_id: string | number | null, acquisitionDate: string, currency: string, value: string, value2: string, nciMeasurement: "fair_value" | "proportionate", value3: string, value4: string, value5: string, value6: string, value7: string, cguReference: string, calcId: string, userId: string, now: number, now2: number, hash: string, value8: string, reason: string, value9: string | null, value10: number | null) {
            return database.prepare("INSERT INTO finance_acquisitions(id,holding_id,company_id,ownership_id,investee_entity_id,acquisition_date,currency,consideration_minor,previous_interest_minor,nci_measurement,nci_minor,identifiable_net_assets_minor,deferred_tax_minor,goodwill_minor,bargain_purchase_minor,cgu_reference,calculation_run_id,created_by,created_at,updated_at,source_hash,source_json,reason,parent_id,parent_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, holdingId, companyId, ownershipId, investee_entity_id, acquisitionDate, currency, value, value2, nciMeasurement, value3, value4, value5, value6, value7, cguReference, calcId, userId, now, now2, hash, value8, reason, value9, value10);
        },
        readDecideAcquisitionEditionCalc(calculation_run_id: string | number | null, companyId: string, holdingId: string) {
            return database.prepare("SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=? AND holding_id=? AND status='prepared'")
                .bind(calculation_run_id, companyId, holdingId)
                .first<Row>();
        },
        guard13(key: string, value: "acquisition-parent", id: string | number | null, companyId: string, holdingId: string, value2: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM finance_acquisitions WHERE id=? AND company_id=? AND holding_id=? AND status='approved' AND version=?)", [
                id,
                companyId,
                holdingId,
                value2,
            ]);
        },
        stageDecideAcquisitionEditionEffect(now: number, id: string | number | null) {
            return database.prepare("UPDATE finance_acquisitions SET status='superseded',version=version+1,updated_at=? WHERE id=?")
                .bind(now, id);
        },
        guard15(key: string, value: "acquisition-version", id: string, companyId: string, holdingId: string, expectedVersion: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM finance_acquisitions WHERE id=? AND company_id=? AND holding_id=? AND status='draft' AND version=?)", [id, companyId, holdingId, expectedVersion]);
        },
        stageDecideAcquisitionEditionEffect2(status: "approved" | "rejected", userId: string, now: number, id: string) {
            return database.prepare("UPDATE finance_acquisitions SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?")
                .bind(status, userId, now, id);
        },
        stageDecideAcquisitionEditionEffect3(status: "approved" | "rejected", userId: string, userId2: string, now: number, calculation_run_id: string | number | null, companyId: string) {
            return database.prepare("UPDATE finance_calculation_runs SET status=?,reviewed_by=?,approved_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='prepared'")
                .bind(status, userId, userId2, now, calculation_run_id, companyId);
        },
        stageDecideAcquisitionEditionEffect4(value: string, calculation_run_id: string | number | null, action: "approve" | "reject", userId: string, reason: string, value2: string, now: number) {
            return database.prepare("INSERT INTO finance_calculation_events VALUES(?,?,2,?,?,?,?,?)")
                .bind(value, calculation_run_id, action, userId, reason, value2, now);
        },
        readGroupOwnershipWorkspaceOwnershipacquisitionsentities(holdingId: string, companyId: string) {
            return database.prepare(groupSourceSql("SELECT o.*,i.company_id investor_company,v.company_id investee_company,i.reporting_name investor_name,v.reporting_name investee_name FROM finance_ownership_interests o JOIN finance_group_entities i ON i.id=o.investor_entity_id JOIN finance_group_entities v ON v.id=o.investee_entity_id WHERE o.holding_id=? AND o.company_id=? ORDER BY o.created_at DESC,o.id LIMIT 200", [groupRecordHistorySql("ownership","o",historySql)]))
                .bind(holdingId, companyId)
                .all<Row>();
        },
        readGroupOwnershipWorkspaceOwnershipacquisitionsentities2(holdingId: string, companyId: string) {
            return database.prepare(groupSourceSql("SELECT a.*,e.reporting_name investee_name FROM finance_acquisitions a JOIN finance_group_entities e ON e.id=a.investee_entity_id WHERE a.holding_id=? AND a.company_id=? ORDER BY a.created_at DESC,a.id LIMIT 200", [groupRecordHistorySql("acquisition","a",historySql)]))
                .bind(holdingId, companyId)
                .all<Row>();
        },
        readGroupOwnershipWorkspaceOwnershipacquisitionsentities3(holdingId: string) {
            return database.prepare("SELECT * FROM finance_group_entities WHERE holding_id=? AND status='active' ORDER BY reporting_name,id")
                .bind(holdingId)
                .all<Row>();
        } };
    return { repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            historySql=await groupHistoryPredicate();
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, batch: effects => database.batch(effects), transaction: run => owned && provided ? run(nativePorts(provided,true,historySql)) : financeTransaction(database, tx => run(nativePorts(tx,true,historySql))), digestBytes: bytes => crypto.subtle.digest('SHA-256', bytes), requirePermission: (...args) => fromNativeSync(() => requirePermission(...args)), requireFinanceTool: (...args) => fromNative(() => requireFinanceTool(database, ...args)), requireFinanceMoneyScale: (...args) => fromNative(() => requireFinanceMoneyScale(database, ...args)), sha256: (...args) => fromNative(() => sha256(...args)) };
}
function application(provided?: SqlDatabase, owned = false) { return createFinanceGroupOwnershipApplication(nativePorts(provided,owned)); }
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
function fromNativeSync<Result>(run: () => Result): Result {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function prepareOwnershipEdition(actor: WorkContext, raw: z.input<typeof ownershipEditionSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.ownership.prepare",purpose:"new-work",capability:"finance.advanced",companyPurchases:()=>Promise.resolve(groupCompanyPurchases([raw.investorCompanyId,raw.investeeCompanyId]))},tx=>invoke(()=>application(tx,true).prepareOwnershipEdition(actor,raw,key))); }
export async function decideOwnershipEdition(actor: WorkContext, id: string, raw: z.input<typeof groupEvidenceDecisionSchema>, action: "approve" | "reject", key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.ownership.${action}`,purpose:"settle-existing",capability:"finance.advanced",companyPurchases:db=>groupRecordPurchases(db,actor,"ownership",id)},tx=>invoke(()=>application(tx,true).decideOwnershipEdition(actor,id,raw,action,key))); }
export async function prepareAcquisitionEdition(actor: WorkContext, raw: z.input<typeof acquisitionEditionSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.acquisition.prepare",purpose:"new-work",capability:"finance.advanced",companyPurchases:db=>groupRecordPurchases(db,actor,"ownership",raw.ownershipId)},tx=>invoke(()=>application(tx,true).prepareAcquisitionEdition(actor,raw,key))); }
export async function decideAcquisitionEdition(actor: WorkContext, id: string, raw: z.input<typeof groupEvidenceDecisionSchema>, action: "approve" | "reject", key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.acquisition.${action}`,purpose:"settle-existing",capability:"finance.advanced",companyPurchases:db=>groupRecordPurchases(db,actor,"acquisition",id)},tx=>invoke(()=>application(tx,true).decideAcquisitionEdition(actor,id,raw,action,key))); }
export async function groupOwnershipWorkspace(actor: WorkContext) { return withFinancePurchasedRead(actor,db=>invoke(()=>application(db).groupOwnershipWorkspace(actor)),"finance.advanced"); }
