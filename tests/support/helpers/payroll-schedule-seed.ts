import { expenseDb } from "./expense-fixture";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { sha256 } from "@erpx/server-runtime/public/crypto";

/** Test-only payroll schedule save seed. The production save command is
 * native (the payroll_schedule owner port mounted in the People Payroll
 * composition); this preserves the original schedule row, command receipt,
 * audit event and outbox event the retired TypeScript writer produced so
 * retained addon-lifecycle oracles can still exercise their own obligations.
 * Guard and receipt semantics mirror the native port byte-for-byte. */
export async function savePayrollScheduleSeed(
  actor: WorkContext,
  groupId: string,
  input: { expectedVersion: number; enabled: boolean; payDelayDays: number; anchorDate?: string | null },
  key: string,
) {
  const previous = await expenseDb
    .prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation='people.payroll.schedule'")
    .bind(key)
    .first<{ result_json: string }>();
  if (previous) return JSON.parse(previous.result_json) as { id: string; enabled: boolean; payDelayDays: number; anchorDate: string | null; version: number };
  return expenseDb.transaction(async (db) => {
    const group = await db
      .prepare("SELECT id,frequency,status,schedule_version FROM hr_pay_groups WHERE id=? AND company_id=?")
      .bind(groupId, actor.companyId)
      .first<{ id: string; frequency: string; status: string; schedule_version: number }>();
    if (!group || group.status !== "active")
      throw Object.assign(new Error("Active pay group not found."), { status: 404, code: "PAY_GROUP_NOT_FOUND" });
    if (group.schedule_version !== input.expectedVersion)
      throw Object.assign(new Error("Pay group schedule changed. Refresh before saving."), { status: 409, code: "VERSION_CONFLICT" });
    if (input.enabled && ["weekly", "fortnightly"].includes(group.frequency) && !input.anchorDate)
      throw Object.assign(new Error("Set an anchor date for weekly or fortnightly payroll."), { status: 422, code: "PAYROLL_ANCHOR_REQUIRED" });
    const now = Date.now();
    const result = { id: groupId, enabled: input.enabled, payDelayDays: input.payDelayDays, anchorDate: input.anchorDate ?? null, version: group.schedule_version + 1 };
    const canonical = JSON.stringify({ enabled: input.enabled, payDelayDays: input.payDelayDays, anchorDate: input.anchorDate ?? null, expectedVersion: input.expectedVersion });
    const requestHash = await sha256(
      JSON.stringify({ actor: actor.userId, assignment: actor.assignmentId, role: actor.role, company: actor.companyId, holding: actor.holdingId, operation: "people.payroll.schedule", recordId: groupId, input: canonical }),
    );
    const authority = JSON.stringify({ role: actor.role, assignmentId: actor.assignmentId, companyId: actor.companyId, holdingId: actor.holdingId, scopeKind: "holding" });
    await db.batch([
      db
        .prepare("UPDATE hr_pay_groups SET schedule_enabled=?,pay_delay_days=?,schedule_anchor_date=?,schedule_version=schedule_version+1,updated_at=? WHERE id=? AND company_id=? AND status='active' AND schedule_version=?")
        .bind(input.enabled, input.payDelayDays, input.anchorDate ?? null, now, groupId, actor.companyId, input.expectedVersion),
      db
        .prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?)")
        .bind(key, "people.payroll.schedule", actor.userId, requestHash, groupId, JSON.stringify(result), now, authority),
      db
        .prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(gen_random_uuid()::text,?,?,?,?,?,?,?,?,?,?)")
        .bind(key, actor.userId, actor.assignmentId, "people.payroll.schedule", "people_payroll", groupId, result.version, "people.payroll.schedule", now, authority),
      db
        .prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(gen_random_uuid()::text,?,?,?,?,?,?,?)")
        .bind(key, "people.payroll.schedule", "people_payroll", groupId, result.version, JSON.stringify({ authority: JSON.parse(authority), id: groupId, state: result.enabled, companyId: actor.companyId, branchId: null, departmentId: null, version: result.version }), now),
    ]);
    const changed = await db.prepare("SELECT schedule_version FROM hr_pay_groups WHERE id=? AND company_id=?").bind(groupId, actor.companyId).first<{ schedule_version: number }>();
    if (!changed || changed.schedule_version !== result.version) throw Object.assign(new Error("Pay group schedule changed. Refresh before saving."), { status: 409, code: "VERSION_CONFLICT" });
    return result;
  });
}
