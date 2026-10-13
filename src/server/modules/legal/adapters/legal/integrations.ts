import { finalRootOwnerProjectionSources as workflowProjectionSources } from "../../../workflow/public/final-root-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {hasPermission} from "@/lib/identity_access/permissions";
import {featureSpecs} from "@/lib/legal/legal/features";
import type {CalendarQuery} from "@/lib/work_projection/calendar";
import type {LegalItem} from "../../domain/model";
import {visibility} from "./access";
import {createLegalIntegrations} from "../../application/integrations";
function application(db:SqlDatabase,a:WorkContext){const v=visibility(a);return createLegalIntegrations({canRead:()=>hasPermission(a,"legal.read"),canEdit:()=>hasPermission(a,"legal.edit"),canApprove:()=>hasPermission(a,"legal.approve"),clock:{now:()=>Date.now()},featureCategories:Object.keys(featureSpecs),repository:{records:async(now)=>db
      .prepare(
        `SELECT i.* FROM legal_items i WHERE ${v.sql} AND i.state NOT IN ('archived','closed','cancelled','terminated','expired','completed') AND (i.owner_id=? OR i.created_by=? OR EXISTS(SELECT 1 FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.item_id=i.id AND e.state='sent' AND e.expires_at>? AND s.user_id=? AND s.state='pending')) ORDER BY i.updated_at DESC LIMIT 200`,
      )
      .bind(...v.values, a.userId, a.userId, now, a.userId)
      .all<LegalItem>(),tasks:async(category)=>db
        .prepare(
          `SELECT f.*,i.title record_title FROM ${featureSpecs[category as keyof typeof featureSpecs].table} f JOIN legal_items i ON i.id=f.item_id WHERE ${v.sql} AND f.owner_id=? AND f.state NOT IN ('completed','closed','cancelled','acknowledged','waived','accepted','billed') AND NOT(f.category='custody' AND f.state='received') ORDER BY f.due_date NULLS LAST,f.id LIMIT 200`,
        )
        .bind(...v.values, a.userId)
        .all<{
          id: string;
          item_id: string;
          title: string;
          record_title: string;
          state: string;
          updated_at: number;
        }>(),approvals:async()=>db
      .prepare(
        `SELECT i.* FROM legal_items i JOIN (${workflowProjectionSources.approval_instances}) ai ON ai.record_type='legal' AND ai.record_id=i.id AND ai.state='pending' JOIN (${workflowProjectionSources.approval_steps}) st ON st.instance_id=ai.id AND st.ordinal=ai.current_step WHERE ${v.sql} AND iam_same_person(st.approver_user_id,?,i.company_id) ORDER BY i.updated_at DESC LIMIT 200`,
      )
      .bind(...v.values, a.userId)
      .all<LegalItem>(),calendar:async(q,from,to,take)=>db
      .prepare(
        `SELECT i.* FROM (SELECT id,id item_id,title,description,due_date,timezone,department_id,branch_id,owner_id,state,version,company_id,classification,kind,created_by FROM legal_items UNION ALL ${Object.values(
          featureSpecs,
        )
          .map(
            (spec) =>
              `SELECT f.id,i.id item_id,f.title,i.description,f.due_date,i.timezone,i.department_id,i.branch_id,f.owner_id,f.state,f.version,i.company_id,i.classification,i.kind,i.created_by FROM ${spec.table} f JOIN legal_items i ON i.id=f.item_id`,
          )
          .join(
            " UNION ALL ",
          )}) i WHERE ${v.sql.replace("lm.item_id=i.id", "lm.item_id=i.item_id")} AND i.due_date>=? AND i.due_date<? AND i.state NOT IN ('archived','cancelled') AND (?='team' OR i.owner_id=?) AND (?='' OR strpos(lower(i.title),lower(?))>0) ORDER BY i.id LIMIT ?`,
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
      .all<LegalItem & { item_id: string }>()}});}
export function legalWork(db:SqlDatabase,a:WorkContext){return application(db,a).legalWork(a);}
export function legalCalendar(db:SqlDatabase,a:WorkContext,q:CalendarQuery,from:string,to:string,take:number){return application(db,a).legalCalendar(a,q,from,to,take);}
