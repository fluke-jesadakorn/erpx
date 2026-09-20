import "server-only";
import { featureSpecs } from "@/lib/legal/features";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import type { WorkItem } from "@/lib/work-item";
import type { CalendarItem, CalendarQuery } from "@/lib/calendar";
import { hasPermission } from "@/lib/permissions";
import { visibility } from "./access";
import type { LegalItem } from "@/lib/legal/model";
export async function legalWork(
  db: SqlDatabase,
  a: WorkContext,
): Promise<WorkItem[]> {
  if (!hasPermission(a, "legal.read")) return [];
  const v = visibility(a);
  const records = (
    await db
      .prepare(
        `SELECT i.* FROM legal_items i WHERE ${v.sql} AND i.state NOT IN ('archived','closed','cancelled','terminated','expired','completed') AND (i.owner_id=? OR i.created_by=? OR EXISTS(SELECT 1 FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.item_id=i.id AND e.state='sent' AND e.expires_at>? AND s.user_id=? AND s.state='pending')) ORDER BY i.updated_at DESC LIMIT 200`,
      )
      .bind(...v.values, a.userId, a.userId, Date.now(), a.userId)
      .all<LegalItem>()
  ).results.map((i) => ({
    id: i.id,
    type: "legal",
    title: i.title,
    detail: `${i.kind} · ${i.state}`,
    href: `/legal?record=${encodeURIComponent(i.id)}`,
    status: i.state,
    updatedAt: i.updated_at,
    queue: i.owner_id === a.userId ? "todo" : "waiting",
    canAct: i.owner_id === a.userId && hasPermission(a, "legal.edit"),
    responsibleQueue: i.owner_id,
    relationships: [i.owner_id === a.userId ? "assigned" : "requested"],
  })) as WorkItem[];
  const tasks: WorkItem[] = [];
  for (const spec of Object.values(featureSpecs)) {
    const rows = (
      await db
        .prepare(
          `SELECT f.*,i.title record_title FROM ${spec.table} f JOIN legal_items i ON i.id=f.item_id WHERE ${v.sql} AND f.owner_id=? AND f.state NOT IN ('completed','closed','cancelled','acknowledged','waived','accepted','billed') AND NOT(f.category='custody' AND f.state='received') ORDER BY f.due_date NULLS LAST,f.id LIMIT 200`,
        )
        .bind(...v.values, a.userId)
        .all<{
          id: string;
          item_id: string;
          title: string;
          record_title: string;
          state: string;
          updated_at: number;
        }>()
    ).results;
    tasks.push(
      ...rows.map((r) => ({
        id: `legal-task:${r.id}`,
        type: "legal" as const,
        title: r.title,
        detail: `${r.record_title} · ${r.state}`,
        href: `/legal?record=${encodeURIComponent(r.item_id)}`,
        status: r.state,
        updatedAt: r.updated_at,
        queue: "todo" as const,
        canAct: hasPermission(a, "legal.edit"),
        responsibleQueue: a.userId,
        relationships: ["assigned" as const],
      })),
    );
  }
  const approvals = (
    await db
      .prepare(
        `SELECT i.* FROM legal_items i JOIN approval_instances ai ON ai.record_type='legal' AND ai.record_id=i.id AND ai.state='pending' JOIN approval_steps st ON st.instance_id=ai.id AND st.ordinal=ai.current_step WHERE ${v.sql} AND iam_same_person(st.approver_user_id,?,i.company_id) ORDER BY i.updated_at DESC LIMIT 200`,
      )
      .bind(...v.values, a.userId)
      .all<LegalItem>()
  ).results;
  for (const i of approvals)
    tasks.push({
      id: `legal-approval:${i.id}`,
      type: "legal",
      title: i.title,
      detail: "Legal approval assigned to you",
      href: `/legal?record=${encodeURIComponent(i.id)}`,
      status: "approval",
      updatedAt: i.updated_at,
      queue: "todo",
      canAct: hasPermission(a, "legal.approve"),
      responsibleQueue: a.userId,
      relationships: ["assigned"],
    });
  return [...records, ...tasks];
}
export async function legalCalendar(
  db: SqlDatabase,
  a: WorkContext,
  q: CalendarQuery,
  from: string,
  to: string,
  take: number,
): Promise<CalendarItem[]> {
  if (!hasPermission(a, "legal.read")) return [];
  const v = visibility(a);
  return (
    await db
      .prepare(
        `SELECT i.* FROM (SELECT id,id item_id,title,description,due_date,timezone,department_id,branch_id,owner_id,state,version,company_id,classification FROM legal_items UNION ALL ${Object.values(
          featureSpecs,
        )
          .map(
            (spec) =>
              `SELECT f.id,i.id item_id,f.title,i.description,f.due_date,i.timezone,i.department_id,i.branch_id,f.owner_id,f.state,f.version,i.company_id,i.classification FROM ${spec.table} f JOIN legal_items i ON i.id=f.item_id`,
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
      .all<LegalItem & { item_id: string }>()
  ).results.map((i) => ({
    id: `legal:${i.id}`,
    source: "legal",
    sourceId: i.item_id,
    kind: "task",
    title: i.title,
    description: i.description,
    allDay: true,
    startDate: i.due_date,
    endDate: i.due_date,
    startsAt: null,
    endsAt: null,
    timezone: i.timezone,
    departmentId: i.department_id,
    branchId: i.branch_id,
    participants: [],
    owner: i.owner_id,
    status: i.state,
    version: i.version,
    permittedActions: [],
    href: `/legal?record=${encodeURIComponent(i.item_id)}`,
    warnings: [],
    history: [],
  }));
}
