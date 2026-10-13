import "server-only";
import type { z } from "zod";
import { decimalToUnits } from "@/lib/finance/finance-money";
import { samePerson } from "@/lib/identity_access/iam";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { financeTransaction } from "@/server/modules/finance/public/finance-transactions";
import { requireFinanceTool } from "@/server/modules/finance/public/finance-company-tools";
import { requireFinanceMoneyScale } from "@/server/modules/finance/public/finance-money-policy";
import { executeCommand, replayCommand, assertion } from "@/server/modules/workflow/public/commands";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { ApiError } from "@/server/modules/runtime/public/http";
import {createIntercompanyPairService,type IntercompanyPairRepository} from '../application/intercompany-pairs';
import type {PairFactRow as Row,intercompanyPairSchema,intercompanyPairDecisionSchema} from '../domain/intercompany-pairs';
export {intercompanyPairSchema,intercompanyPairDecisionSchema} from '../domain/intercompany-pairs';
function nativePairRepository(db:SqlDatabase):IntercompanyPairRepository{return {moneyScale:(companyId,currency)=>requireFinanceMoneyScale(db,companyId,currency),tool:companyId=>requireFinanceTool(db,companyId,'group_reporting'),replay:(actor,key,op,id,input)=>replayCommand(db,actor,key,op,id,input),entity:async(holdingId,companyId)=>db
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
        db
          .prepare(
            "INSERT INTO finance_intercompany_document_pairs(id,holding_id,company_id,reference,source_company_id,source_journal_id,source_account_id,target_company_id,target_journal_id,target_account_id,currency,amount_minor,source_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.holdingId,
            actor.companyId,
            input.reference,
            input.source.companyId,
            input.source.journalId,
            input.source.accountId,
            input.target.companyId,
            input.target.journalId,
            input.target.accountId,
            input.currency,
            String(decimalToUnits(input.amount)),
            JSON.stringify({ input, evidence: e.evidence }),
            e.hash,
            actor.userId,
            input.reason,
            now,
            now,
          ),
        ...[input.source, input.target].map((s) =>
          db
            .prepare(
              "INSERT INTO finance_intercompany_pair_claims(pair_id,journal_id,account_id) VALUES(?,?,?)",
            )
            .bind(id, s.journalId, s.accountId),
        ),
      ],
    );},
pair:async(actor,id)=>db
      .prepare(
        "SELECT * FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=? AND company_id=?",
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
        db
          .prepare(
            "UPDATE finance_intercompany_document_pairs SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(status, actor.userId, now, id),
      ];
    if (action !== "confirm")
      steps.push(
        db
          .prepare(
            "DELETE FROM finance_intercompany_pair_claims WHERE pair_id=?",
          )
          .bind(id),
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
function nativePairs(provided?:SqlDatabase){return createIntercompanyPairService({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,hash:sha256,now:Date.now,transaction:async(actor,run)=>financeTransaction(provided??(await holdingResources(actor.holdingId)).write,db=>run(nativePairRepository(db))),workspace:async actor=>{const write=provided??(await holdingResources(actor.holdingId)).write;  return (
    await write
      .prepare(
        "SELECT * FROM finance_intercompany_document_pairs WHERE holding_id=? AND company_id=? ORDER BY created_at DESC,id LIMIT 200",
      )
      .bind(actor.holdingId, actor.companyId)
      .all<Row>()
  ).results;}});}
export const intercompanyPairSource=(db:SqlDatabase,actor:WorkContext,input:z.infer<typeof intercompanyPairSchema>)=>nativePairs(db).intercompanyPairSource(nativePairRepository(db),actor,input);
export const prepareIntercompanyPair=(actor:WorkContext,raw:z.input<typeof intercompanyPairSchema>,key:string)=>nativePairs().prepareIntercompanyPair(actor,raw,key);
export const decideIntercompanyPair=(actor:WorkContext,id:string,raw:z.input<typeof intercompanyPairDecisionSchema>,action:'confirm'|'reject'|'reverse',key:string)=>nativePairs().decideIntercompanyPair(actor,id,raw,action,key);
export const intercompanyPairWorkspace=(actor:WorkContext,provided?:SqlDatabase)=>nativePairs(provided).intercompanyPairWorkspace(actor);
