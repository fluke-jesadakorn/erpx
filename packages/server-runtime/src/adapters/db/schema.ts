import { bigint, pgTable, text } from "drizzle-orm/pg-core";

export const leaveRequests = pgTable("leave_requests", {
  id: text("id").primaryKey(),
  number: text("number").notNull(),
  employee_id: text("employee_id").notNull(),
  person_id: text("person_id"),
  engagement_id: text("engagement_id"),
  company_id: text("company_id").notNull(),
  branch_id: text("branch_id").notNull(),
  department_id: text("department_id"),
  start_date: text("start_date").notNull(),
  days: bigint("days", { mode: "number" }).notNull(),
  handover_note: text("handover_note").notNull(),
  state: text("state").notNull(),
  version: bigint("version", { mode: "number" }).notNull(),
  created_by: text("created_by").notNull(),
  prepared_by: text("prepared_by"),
  decided_by: text("decided_by"),
  decision_note: text("decision_note"),
  last_command_id: text("last_command_id"),
  created_at: bigint("created_at", { mode: "number" }).notNull(),
  updated_at: bigint("updated_at", { mode: "number" }).notNull(),
});
