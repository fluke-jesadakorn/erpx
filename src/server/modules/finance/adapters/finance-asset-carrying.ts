import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
import { sha256 } from "@/server/modules/runtime/public/crypto";

import {createAssetCarryingEvidence} from '../application/asset-carrying';
import type {AssetCarryBook,AssetCarryingRow as Row} from '../domain/asset-carrying';
export type {AssetCarryBasis,AssetCarryBook} from '../domain/asset-carrying';
export const assetCarryingEvidence=(db:SqlDatabase,a:Row,books:AssetCarryBook[])=>createAssetCarryingEvidence({error:(...args)=>new ApiError(...args),hash:sha256,lines:async(ids)=>(await db
      .prepare(
        `SELECT j.id journal_id,j.chart_id,j.company_id,j.entry_date,j.status,j.source_type,j.source_id,j.currency,j.fx_rate_micros,l.account_id,l.ordinal,l.dimension_json,l.base_debit_minor,l.base_credit_minor FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.id IN (${ids.map(() => "?").join(",")}) ORDER BY j.id,l.ordinal`,
      )
      .bind(...ids)
      .all<Row>()).results,
legacy:async(assetId)=>(await db
      .prepare(
        "SELECT * FROM finance_asset_depreciations WHERE asset_id=? ORDER BY ordinal",
      )
      .bind(assetId)
      .all<Row>()).results,
runs:async(assetId)=>(await db
      .prepare(
        "SELECT id,ordinal,status,result_json,journal_ids_json,reversal_ids_json,entry_date,reversal_entry_date FROM finance_asset_policy_runs WHERE asset_id=? AND status IN ('posted','reversed') ORDER BY ordinal,id",
      )
      .bind(assetId)
      .all<Row>()).results,
openingRule:async(companyId,primaryChartId,secondaryChartId)=>db
        .prepare(
          "SELECT * FROM finance_book_rules WHERE company_id=? AND primary_chart_id=? AND secondary_chart_id=? AND status='approved' ORDER BY effective_date,id LIMIT 1",
        )
        .bind(companyId, primaryChartId, secondaryChartId)
        .first<Row>(),
primaryOpening:async(companyId,chartId,effectiveDate)=>(await db
          .prepare(
            `SELECT l.account_id AS "accountId",l.dimension_json AS dimensions,SUM(l.base_debit_minor-l.base_credit_minor) AS minor FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<? GROUP BY l.account_id,l.dimension_json HAVING SUM(l.base_debit_minor-l.base_credit_minor)<>0 ORDER BY l.account_id,l.dimension_json`,
          )
          .bind(companyId, chartId, effectiveDate)
          .all<Row>()).results,
mappedOpening:async(ruleId,companyId)=>(await db
          .prepare(
            "SELECT l.account_id,l.dimension_json,l.base_debit_minor-l.base_credit_minor minor,m.primary_account_id FROM finance_journal_lines l JOIN finance_book_account_mappings m ON m.rule_id=? AND m.secondary_account_id=l.account_id JOIN finance_journals j ON j.id=l.journal_id WHERE j.id=? AND j.company_id=? AND j.status='posted' AND j.source_type='book_opening' ORDER BY m.primary_account_id,l.dimension_json",
          )
          .bind(ruleId, `book-opening-${ruleId}`, companyId)
          .all<Row>()).results,
projections:async(afterIds,chartId)=>(await db
              .prepare(
                `SELECT p.primary_journal_id,p.secondary_journal_id FROM finance_book_postings p JOIN finance_journals j ON j.id=p.secondary_journal_id WHERE p.primary_journal_id IN (${afterIds.map(() => "?").join(",")}) AND j.chart_id=? ORDER BY p.primary_journal_id`,
              )
              .bind(...afterIds, chartId)
              .all<Row>()).results,
adjustments:async(companyId,chartId,primaryIds)=>(await db
              .prepare(
                `SELECT id,status,journal_id,reversal_journal_id FROM finance_book_adjustments WHERE company_id=? AND chart_id=? AND primary_journal_id IN (${primaryIds.map(() => "?").join(",")}) AND status IN ('posted','reversed') ORDER BY id`,
              )
              .bind(companyId, chartId, ...primaryIds)
              .all<Row>()).results})(a,books);
