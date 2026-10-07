import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {CommercialAiUsageRow as AiRow} from "../../commercial/public/usage-contract";
export function commercialAIUsage(db:SqlDatabase,binding:{id:string},start:number,end:number,branchId:string|null){return db.prepare(`SELECT COALESCE(g.id,a.id) id,COALESCE(g.company_id,a.company_id) company_id,
        CASE WHEN g.id IS NOT NULL THEN g.branch_id ELSE a.branch_id END branch_id,COALESCE(g.model,a.model) model,
        COALESCE(g.feature,a.feature) feature,a.status,
        COALESCE(g.input_tokens,a.input_tokens,0) input_tokens,
        COALESCE(g.output_tokens,a.output_tokens,0) output_tokens,
        CASE WHEN g.id IS NULL THEN COALESCE(a.funding_source,'unknown')
             WHEN a.id IS NULL THEN g.funding_source
             WHEN a.branch_id IS DISTINCT FROM g.branch_id
               OR a.model IS DISTINCT FROM g.model
               OR a.feature IS DISTINCT FROM g.feature THEN 'unknown'
             WHEN a.funding_source=g.funding_source THEN g.funding_source
             ELSE 'unknown' END funding_source,
        COALESCE(g.created_at,a.created_at) created_at,
        g.status gateway_status,g.provider,g.model gateway_model,g.units_json::text units_json,
        e.provider_statement_id,e.thb_satang verified_cost_satang,e.verified_at,e.reconciled_at
      FROM ai_gateway_requests g
      FULL OUTER JOIN ai_provider_usage a ON a.id=g.usage_id AND a.company_id=g.company_id
      LEFT JOIN ai_provider_cost_evidence e ON e.request_id=g.id
      WHERE COALESCE(g.created_at,a.created_at)>=? AND COALESCE(g.created_at,a.created_at)<?
        AND (g.holding_id IS NULL OR g.holding_id=?)
        AND (?::text IS NULL OR (CASE WHEN g.id IS NOT NULL THEN g.branch_id ELSE a.branch_id END)=?)
      ORDER BY COALESCE(g.created_at,a.created_at) DESC,COALESCE(g.id,a.id) DESC`)
      .bind(start,end,binding.id,branchId,branchId).all<AiRow>();}
export function commercialEarliestAIUsage(db:SqlDatabase,){return db.prepare("SELECT MIN(created_at) earliest FROM ai_provider_usage")
          .first<{ earliest: number | null }>();}
