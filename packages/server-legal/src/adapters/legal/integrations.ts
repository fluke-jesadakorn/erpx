import {legalReadScope} from '@erpx/server-legal/adapters/purchases';
import {legalItemSelectSql} from '../legal-read-tail-r16-sources';
import { legalEnvelopeSignerReadHead } from '../legal-read-tail-r43-sources';
import contract from '../legal-integrations-sources.json' with {type:'json'};
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('LEGAL_INTEGRATIONS_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
import {filterLegalPurchasedWork} from '@erpx/server-legal/adapters/work-purchases';
import { finalRootOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/final-root-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {featureSpecs} from "@erpx/feature-legal/lib/legal/features";
import type {CalendarQuery} from "@erpx/feature-work_projection/lib/calendar";
import type {LegalItem} from "@erpx/server-legal/domain/model";
import {visibility} from "@erpx/server-legal/adapters/legal/access";
import {createLegalIntegrations} from "@erpx/server-legal/application/integrations";
function application(db:SqlDatabase,a:WorkContext){const v=visibility(a);let purchase:ReturnType<typeof legalReadScope>|undefined;const readScope=()=>purchase??=legalReadScope(db,a);return createLegalIntegrations({canRead:()=>hasPermission(a,"legal.read"),canEdit:()=>hasPermission(a,"legal.edit"),canApprove:()=>hasPermission(a,"legal.approve"),clock:{now:()=>Date.now()},featureCategories:Object.keys(featureSpecs),repository:{records:async(now)=>{const purchase=await readScope();return db
      .prepare(
        `${legalItemSelectSql}${v.sql} AND ${purchase.where} AND i.state NOT IN ('archived','closed','cancelled','terminated','expired','completed') AND (i.owner_id=? OR i.created_by=? OR EXISTS(${legalEnvelopeSignerReadHead('1')}e.item_id=i.id AND e.state='sent' AND e.expires_at>? AND s.user_id=? AND s.state='pending')) ORDER BY i.updated_at DESC LIMIT 200`,
      )
      .bind(...v.values, a.userId, a.userId, now, a.userId)
      .all<LegalItem>();},tasks:async(category)=>{const purchase=await readScope();return db
        .prepare(
          fill(contract.tasks,{table:featureSpecs[category as keyof typeof featureSpecs].table,visibility:v.sql,purchase:purchase.where}),
        )
        .bind(...v.values, a.userId)
        .all<{
          id: string;
          item_id: string;
          title: string;
          record_title: string;
          state: string;
          updated_at: number;
        }>();},approvals:async()=>{const purchase=await readScope();return db
      .prepare(
        fill(contract.approvals,{approvalInstances:workflowProjectionSources.approval_instances,approvalSteps:workflowProjectionSources.approval_steps,visibility:v.sql,purchase:purchase.where}),
      )
      .bind(...v.values, a.userId)
      .all<LegalItem>();},calendar:async(q,from,to,take)=>{const purchase=await readScope();return db
      .prepare(
        fill(contract.calendarRows,{sources:[contract.calendarBase,...Object.values(featureSpecs).map(spec=>fill(contract.calendarFeature,{table:spec.table}))].join(" UNION ALL "),visibility:v.sql.replace("lm.item_id=i.id", "lm.item_id=i.item_id"),purchase:purchase.where}),
      )
      .bind(
        ...v.values,
        from,
        to,
        q.mode ?? "mine",
        a.userId,
        q.search,
        q.search,
        take,
      )
      .all<LegalItem & { item_id: string }>();}}});}
export async function legalWork(db:SqlDatabase,a:WorkContext){return filterLegalPurchasedWork(db,a,await application(db,a).legalWork(a));}
export function legalCalendar(db:SqlDatabase,a:WorkContext,q:CalendarQuery,from:string,to:string,take:number){return application(db,a).legalCalendar(a,q,from,to,take);}
