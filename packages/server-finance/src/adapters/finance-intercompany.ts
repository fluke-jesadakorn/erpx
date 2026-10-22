import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import { groupCompanyPurchases, groupRecordPurchases, groupHistoryPredicate, groupSourceSql, groupRecordHistorySql } from "@erpx/server-finance/adapters/group-purchase-sources";
import "server-only";
import type { z } from "zod";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { decimalToMinor } from "@erpx/server-finance/public/finance";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";

import {createIntercompanyService} from '@erpx/server-finance/application/intercompany';
import type {DecisionAction,IntercompanyRow,GroupEntity,groupEntitySchema,intercompanyTransactionSchema,intercompanyDecisionSchema} from '@erpx/server-finance/domain/intercompany';
import {financeIntercompanyAvailableCompanySource,financeIntercompanyActiveCompany} from '@erpx/server-organization/public/finance-intercompany-sources';
export {groupEntitySchema,intercompanyTransactionSchema,intercompanyDecisionSchema} from '@erpx/server-finance/domain/intercompany';
export type {GroupEntity,IntercompanyRow} from '@erpx/server-finance/domain/intercompany';
function nativeIntercompany(provided?:SqlDatabase){let write:SqlDatabase;let historySql:(company:string)=>string=()=>"TRUE";return createIntercompanyService({error:(...args)=>new ApiError(...args),permission:requirePermission,allowed:hasPermission,ready:async actor=>{historySql=await groupHistoryPredicate();write=provided??(await holdingResources(actor.holdingId)).write},amount:decimalToMinor,now:Date.now,replay:(actor,key,op,id,input)=>replayCommand(write,actor,key,op,id,input),entities:async(actor,targetCompanyId)=>(await write.prepare("SELECT company_id FROM finance_group_entities WHERE holding_id=? AND company_id IN (?,?) AND status='active'").bind(actor.holdingId, actor.companyId, targetCompanyId).all<{ company_id: string }>()).results,
transaction:async(actor,id)=>write.prepare(groupSourceSql("SELECT * FROM finance_intercompany_transactions WHERE id=? AND holding_id=?",[groupRecordHistorySql("intercompany","finance_intercompany_transactions",historySql)])).bind(id, actor.holdingId).first<IntercompanyRow>(),
journal:async(id,companyId,currency)=>write.prepare("SELECT id FROM finance_journals WHERE id=? AND company_id=? AND currency=? AND status='posted'").bind(id, companyId, currency).first(),
workspace:async(actor)=>{  const [transactions, groupEntities, availableCompanies] = await Promise.all([
    write.prepare(groupSourceSql("SELECT * FROM finance_intercompany_transactions WHERE holding_id=? ORDER BY created_at DESC,id DESC LIMIT 200",[groupRecordHistorySql("intercompany","finance_intercompany_transactions",historySql)])).bind(actor.holdingId).all<IntercompanyRow>(),
    write.prepare("SELECT company_id,reporting_name,functional_currency,status,version,created_by,updated_at FROM finance_group_entities WHERE holding_id=? AND status='active' ORDER BY reporting_name,company_id").bind(actor.holdingId).all<GroupEntity>(),
    write.prepare(`${financeIntercompanyAvailableCompanySource} AND NOT EXISTS (SELECT 1 FROM finance_group_entities e WHERE e.holding_id=? AND e.company_id=n.id) ORDER BY n.display_name,n.id`).bind(actor.holdingId).all<{company_id:string;display_name:string}>().then(result=>result.results),
  ]);
return {transactions:transactions.results,groupEntities:groupEntities.results,availableCompanies};},
company:(id)=>financeIntercompanyActiveCompany(write,id),
existingEntity:async(actor,companyId)=>write.prepare("SELECT id FROM finance_group_entities WHERE holding_id=? AND company_id=?").bind(actor.holdingId, companyId).first<{ id: string }>(),
createEntityCommit:async(facts)=>{const {actor,input,key,id,now}=facts;return executeCommand(write, actor, key, "finance.group-entity.create", input, id, 1, { id, companyId: input.companyId, status: "active", version: 1 }, [
    assertion(write, key, "group-entity-unique", "NOT EXISTS(SELECT 1 FROM finance_group_entities WHERE holding_id=? AND company_id=?)", [actor.holdingId, input.companyId]),
    write.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'active',1,?,?,?)").bind(id, actor.holdingId, input.companyId, input.reportingName, input.functionalCurrency, actor.userId, now, now),
  ], { companyId: input.companyId, branchId: null, departmentId: null });},
createCommit:async(facts)=>{const {actor,input,key,id,now,result,amount}=facts;return executeCommand(write, actor, key, "finance.intercompany.create", input, id, 1, result, [
    write.prepare("INSERT INTO finance_intercompany_transactions(id,holding_id,source_company_id,target_company_id,reference,currency,amount_minor,source_journal_id,target_journal_id,status,version,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,'unmatched',1,?,?)").bind(id, actor.holdingId, actor.companyId, input.targetCompanyId, input.reference, input.currency, amount, input.sourceJournalId ?? null, input.targetJournalId ?? null, actor.userId, now),
    write.prepare("INSERT INTO finance_intercompany_events(id,transaction_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`intercompany-event-${key}`, id, actor.holdingId, "created", null, "unmatched", actor.userId, 1, "Intercompany item prepared for independent matching.", JSON.stringify({ sourceJournalId: input.sourceJournalId ?? null, targetJournalId: input.targetJournalId ?? null }), now),
  ]);},
previousActor:async(id)=>write.prepare("SELECT actor_id FROM finance_intercompany_events WHERE transaction_id=? AND action IN ('created','matched','settled') ORDER BY version DESC LIMIT 1").bind(id).first<{ actor_id: string }>(),
decisionCommit:async(facts)=>{const {actor,id,input,key,rule,row,version,now,result}=facts;  const steps: SqlStatement[] = [
    assertion(write, key, "intercompany-version", "EXISTS(SELECT 1 FROM finance_intercompany_transactions WHERE id=? AND holding_id=? AND status=? AND version=?)", [id, actor.holdingId, rule.from, input.expectedVersion]),
    assertion(write, key, "intercompany-independent", "NOT EXISTS(SELECT 1 FROM finance_intercompany_events WHERE transaction_id=? AND version=(SELECT MAX(version) FROM finance_intercompany_events WHERE transaction_id=?) AND actor_id=?)", [id, id, actor.userId]),
    write.prepare("UPDATE finance_intercompany_transactions SET status=?,version=version+1 WHERE id=? AND holding_id=?").bind(rule.to, id, actor.holdingId),
    write.prepare("INSERT INTO finance_intercompany_events(id,transaction_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`intercompany-event-${key}`, id, actor.holdingId, rule.event, row.status, rule.to, actor.userId, version, input.reason, JSON.stringify({ previousVersion: input.expectedVersion }), now),
  ];
  return executeCommand(write, actor, key, rule.operation, input, id, version, result, steps);},
detail:async(actor,id)=>write.prepare(groupSourceSql("SELECT * FROM finance_intercompany_transactions WHERE id=? AND holding_id=?",[groupRecordHistorySql("intercompany","finance_intercompany_transactions",historySql)])).bind(id,actor.holdingId).first<IntercompanyRow>()})}
export const intercompanyWorkspace=(actor:WorkContext,provided?:SqlDatabase)=>provided?nativeIntercompany(provided).intercompanyWorkspace(actor):withFinancePurchasedRead(actor,db=>nativeIntercompany(db).intercompanyWorkspace(actor),"finance.advanced");
export const createGroupEntity=(actor:WorkContext,input:z.infer<typeof groupEntitySchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.group-entity.create",purpose:"new-work",capability:"finance.advanced",companyPurchases:()=>Promise.resolve(groupCompanyPurchases([input.companyId]))},tx=>nativeIntercompany(tx).createGroupEntity(actor,input,key));
export const createIntercompany=(actor:WorkContext,input:z.infer<typeof intercompanyTransactionSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.intercompany.create",purpose:"new-work",capability:"finance.advanced",companyPurchases:()=>Promise.resolve(groupCompanyPurchases([actor.companyId,input.targetCompanyId]))},tx=>nativeIntercompany(tx).createIntercompany(actor,input,key));
export const decideIntercompany=(actor:WorkContext,id:string,action:DecisionAction,input:z.infer<typeof intercompanyDecisionSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:`finance.intercompany.${action}`,purpose:"settle-existing",capability:"finance.advanced",companyPurchases:db=>groupRecordPurchases(db,actor,"intercompany",id)},tx=>nativeIntercompany(tx).decideIntercompany(actor,id,action,input,key));
export const intercompanyDetail=(actor:WorkContext,id:string,provided?:SqlDatabase)=>provided?nativeIntercompany(provided).intercompanyDetail(actor,id):withFinancePurchasedRead(actor,db=>nativeIntercompany(db).intercompanyDetail(actor,id),"finance.advanced");
