import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import { groupCompanyPurchases, groupRecordPurchases, requireGroupCompanyHistory, groupHistoryPredicate, groupSourceSql, groupRecordHistorySql } from "@erpx/server-finance/adapters/group-purchase-sources";
import "server-only";
import type { z } from "zod";
import { decimalToUnits } from "@erpx/feature-finance/lib/finance-money";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { financeTransaction } from "@erpx/server-finance/public/finance-transactions";
import { requireFinanceTool } from "@erpx/server-finance/public/finance-company-tools";
import { requireFinanceMoneyScale } from "@erpx/server-finance/public/finance-money-policy";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import {createIntercompanyPairService,type IntercompanyPairRepository} from '@erpx/server-finance/application/intercompany-pairs';
import type {PairFactRow as Row,intercompanyPairSchema,intercompanyPairDecisionSchema} from '@erpx/server-finance/domain/intercompany-pairs';
import { financeIntercompanyPairs } from '@erpx/server-collaboration/adapters/native-finance-intercompany-pairs';
export {intercompanyPairSchema,intercompanyPairDecisionSchema} from '@erpx/server-finance/domain/intercompany-pairs';
function nativePairRepository(db:SqlDatabase,historySql:(company:string)=>string=()=>"TRUE"):IntercompanyPairRepository{return {moneyScale:(companyId,currency)=>requireFinanceMoneyScale(db,companyId,currency),tool:companyId=>requireFinanceTool(db,companyId,'group_reporting'),replay:(actor,key,op,id,input)=>replayCommand(db,actor,key,op,id,input),entity:async(holdingId,companyId)=>db
        .prepare(
          "SELECT * FROM finance_group_entities WHERE holding_id=? AND company_id=? AND status='active'",
        )
        .bind(holdingId, companyId)
        .first<Row>(),
journal:async(journalId,companyId,currency)=>db
        .prepare(
          "SELECT * FROM finance_journals WHERE id=? AND company_id=? AND status='posted' AND currency=?",
        )
        .bind(journalId, companyId, currency)
        .first<Row>(),
lines:async(journalId,accountId,companyId)=>(await db
        .prepare(
          "SELECT l.*,a.code,a.category,a.version account_version FROM finance_journal_lines l JOIN finance_accounts a ON a.id=l.account_id WHERE l.journal_id=? AND l.account_id=? AND a.company_id=? ORDER BY l.ordinal",
        )
        .bind(journalId, accountId, companyId)
        .all<Row>()).results,
prepareCommit:async(facts)=>{const {actor,key,op,input,id,e,now}=facts;return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1 },
      [
        await financeIntercompanyPairs(db, actor.companyId).createPair({ id, holdingId: actor.holdingId, companyId: actor.companyId, reference: input.reference, sourceCompanyId: input.source.companyId, sourceJournalId: input.source.journalId, sourceAccountId: input.source.accountId, targetCompanyId: input.target.companyId, targetJournalId: input.target.journalId, targetAccountId: input.target.accountId, currency: input.currency, amountMinor: String(decimalToUnits(input.amount)), sourceJson: JSON.stringify({ input, evidence: e.evidence }), sourceHash: e.hash, userId: actor.userId, reason: input.reason, now }),
        ...await Promise.all([input.source, input.target].map(async (s) =>
          await financeIntercompanyPairs(db, actor.companyId).claim({ pairId: id, journalId: s.journalId, accountId: s.accountId }),
        )),
      ],
    );},
pair:async(actor,id)=>db
      .prepare(
        groupSourceSql("SELECT * FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=? AND company_id=?",[groupRecordHistorySql("pair","finance_intercompany_document_pairs",historySql)]),
      )
      .bind(id, actor.holdingId, actor.companyId)
      .first<Row>(),
decisionCommit:async(facts)=>{const {actor,id,input,action,key,op,expected,status,now}=facts;    const steps = [
        assertion(
          db,
          key,
          "intercompany-pair-version",
          "EXISTS(SELECT 1 FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=? AND company_id=? AND status=? AND version=?)",
          [
            id,
            actor.holdingId,
            actor.companyId,
            expected,
            input.expectedVersion,
          ],
        ),
          await financeIntercompanyPairs(db, actor.companyId).decide({ status, userId: actor.userId, now, id }),
      ];
    if (action !== "confirm")
      steps.push(
          await financeIntercompanyPairs(db, actor.companyId).clearClaims({ pairId: id }),
      );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      input.expectedVersion + 1,
      { id, status, version: input.expectedVersion + 1 },
      steps,
    );}};}
function nativePairs(provided?:SqlDatabase,owned=false){return createIntercompanyPairService({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,hash:sha256,now:Date.now,transaction:async(actor,run)=>{const historySql=await groupHistoryPredicate();return provided&&owned?run(nativePairRepository(provided,historySql)):financeTransaction(provided??(await holdingResources(actor.holdingId)).write,db=>run(nativePairRepository(db,historySql)))},workspace:async actor=>{const historySql=await groupHistoryPredicate();const write=provided??(await holdingResources(actor.holdingId)).write;  return (
    await write
      .prepare(
        groupSourceSql("SELECT * FROM finance_intercompany_document_pairs WHERE holding_id=? AND company_id=? ORDER BY created_at DESC,id LIMIT 200",[groupRecordHistorySql("pair","finance_intercompany_document_pairs",historySql)]),
      )
      .bind(actor.holdingId, actor.companyId)
      .all<Row>()
  ).results;}});}
export const intercompanyPairSource=async(db:SqlDatabase,actor:WorkContext,input:z.infer<typeof intercompanyPairSchema>)=>{for(const source of [input.source,input.target])await requireGroupCompanyHistory(db,source.companyId);return nativePairs(db).intercompanyPairSource(nativePairRepository(db),actor,input);};
export const prepareIntercompanyPair=(actor:WorkContext,raw:z.input<typeof intercompanyPairSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.intercompany-pair.prepare",purpose:"settle-existing",capability:"finance.advanced",companyPurchases:()=>Promise.resolve(groupCompanyPurchases([raw.source.companyId,raw.target.companyId]))},tx=>nativePairs(tx,true).prepareIntercompanyPair(actor,raw,key));
export const decideIntercompanyPair=(actor:WorkContext,id:string,raw:z.input<typeof intercompanyPairDecisionSchema>,action:'confirm'|'reject'|'reverse',key:string)=>withFinancePurchasedCommand(actor,{key,operation:`finance.intercompany-pair.${action}`,purpose:"settle-existing",capability:"finance.advanced",companyPurchases:db=>groupRecordPurchases(db,actor,"pair",id)},tx=>nativePairs(tx,true).decideIntercompanyPair(actor,id,raw,action,key));
export const intercompanyPairWorkspace=(actor:WorkContext,provided?:SqlDatabase)=>provided?nativePairs(provided).intercompanyPairWorkspace(actor):withFinancePurchasedRead(actor,db=>nativePairs(db).intercompanyPairWorkspace(actor),"finance.advanced");
