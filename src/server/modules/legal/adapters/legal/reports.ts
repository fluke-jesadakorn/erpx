import "server-only";
import type {SqlDatabase} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {requirePermission} from "../../../identity_access/public/iam-permissions";
import {featureSpecs} from "@/lib/legal/features";
import {visibility} from "./access";
import {createLegalReportsQuery} from "../../application/reports";
function application(db:SqlDatabase,a:WorkContext){const v=visibility(a);return createLegalReportsQuery({require:()=>requirePermission(a,"legal.read"),clock:{now:()=>Date.now()},featureCategories:Object.keys(featureSpecs),repository:{registers:async(now)=>db
      .prepare(
        `SELECT i.kind,i.state,count(*) count,count(*) FILTER(WHERE i.due_date::date<(clock_timestamp() AT TIME ZONE i.timezone)::date AND i.state NOT IN ('closed','completed','expired','terminated','archived','cancelled')) overdue,count(*) FILTER(WHERE i.sla_due_at<? AND i.state NOT IN ('closed','completed','archived','cancelled')) sla_breached FROM legal_items i WHERE ${v.sql} GROUP BY i.kind,i.state ORDER BY i.kind,i.state`,
      )
      .bind(now, ...v.values)
      .all(),upcoming:async()=>db
      .prepare(
        `SELECT i.id,i.title,i.kind,i.due_date,i.owner_id,i.state FROM legal_items i WHERE ${v.sql} AND i.due_date::date BETWEEN (clock_timestamp() AT TIME ZONE i.timezone)::date AND (clock_timestamp() AT TIME ZONE i.timezone)::date+90 AND i.state NOT IN ('closed','completed','expired','terminated','archived','cancelled') ORDER BY i.due_date,i.id LIMIT 100`,
      )
      .bind(...v.values)
      .all(),workflow:async(category)=>db
          .prepare(
            `SELECT f.state,count(*) count,count(*) FILTER(WHERE f.due_date::date<(clock_timestamp() AT TIME ZONE i.timezone)::date) overdue FROM ${featureSpecs[category as keyof typeof featureSpecs].table} f JOIN legal_items i ON i.id=f.item_id WHERE ${v.sql} GROUP BY f.state`,
          )
          .bind(...v.values)
          .all(),costs:async()=>db
      .prepare(
        `SELECT i.currency,sum(COALESCE(NULLIF(c.data->>'budget',''),'0')::numeric) budget_minor,sum(COALESCE(NULLIF(c.data->>'fees',''),'0')::numeric) fees_minor FROM legal_counsel c JOIN legal_items i ON i.id=c.item_id WHERE ${v.sql} GROUP BY i.currency`,
      )
      .bind(...v.values)
      .all(),access:async(now)=>db
      .prepare(
        `SELECT i.id,i.title,i.classification,count(DISTINCT m.user_id) members,count(DISTINCT s.id) FILTER(WHERE s.revoked_at IS NULL AND s.expires_at>?) external_shares FROM legal_items i LEFT JOIN legal_members m ON m.item_id=i.id LEFT JOIN legal_shares s ON s.item_id=i.id WHERE ${v.sql} GROUP BY i.id ORDER BY i.title LIMIT 200`,
      )
      .bind(now, ...v.values)
      .all(),jobs:async()=>db
      .prepare(
        `SELECT p.id,p.item_id,p.kind,p.state,p.error,p.attempts,p.created_at FROM legal_processing_jobs p JOIN legal_items i ON i.id=p.item_id WHERE ${v.sql} ORDER BY p.created_at DESC LIMIT 100`,
      )
      .bind(...v.values)
      .all(),invitations:async()=>db
      .prepare(
        `SELECT j.id,s.item_id,j.state,j.error,j.attempts,j.created_at FROM legal_invitation_jobs j JOIN legal_shares s ON s.id=j.share_id JOIN legal_items i ON i.id=s.item_id WHERE ${v.sql} ORDER BY j.created_at DESC LIMIT 100`,
      )
      .bind(...v.values)
      .all()}});}
export function legalReports(db:SqlDatabase,a:WorkContext){return application(db,a)();}
