export * from "@erpx/server-operations/public/rules/guided-intake";
import {intakeFields,type IntakeIntent} from "@erpx/server-operations/public/rules/guided-intake";
const handoffAgeMs = 30 * 60 * 1000;
export function guidedIntakeHandoffKey(companyId: string, userId: string, assignmentId: string, base: string, intent: Exclude<IntakeIntent, "unknown">) {
  return `erpx:guided-intake:${JSON.stringify([companyId,userId,assignmentId,base,intent])}`;
}

/** Browser-only handoff. The owning form still checks every field and requires human submission. */
export function saveGuidedIntakeHandoff(companyId: string, userId: string, assignmentId: string, base: string, intent: Exclude<IntakeIntent, "unknown">, draft: Record<string, string>) {
  try {
    const fields = Object.fromEntries(intakeFields[intent].filter(key => typeof draft[key] === "string").map(key => [key, draft[key]]));
    sessionStorage.setItem(guidedIntakeHandoffKey(companyId, userId, assignmentId, base, intent), JSON.stringify({ savedAt: Date.now(), fields }));
    return true;
  } catch { return false; }
}

export function readGuidedIntakeHandoff(companyId: string, userId: string, assignmentId: string, base: string, intent: Exclude<IntakeIntent, "unknown">) {
  try {
    const raw = sessionStorage.getItem(guidedIntakeHandoffKey(companyId, userId, assignmentId, base, intent));
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    const value = parsed as { savedAt?: unknown; fields?: unknown };
    if (typeof value.savedAt !== "number" || Date.now() - value.savedAt > handoffAgeMs || value.savedAt > Date.now() + 60000) {
      sessionStorage.removeItem(guidedIntakeHandoffKey(companyId, userId, assignmentId, base, intent));
      return null;
    }
    const fields = value.fields && typeof value.fields === "object" && !Array.isArray(value.fields) ? value.fields as Record<string, unknown> : {};
    return Object.fromEntries(intakeFields[intent].filter(key => typeof fields[key] === "string").map(key => [key, String(fields[key]).slice(0,key === "body" || key === "description" ? 3000 : 300)]));
  } catch { return null; }
}

export function clearGuidedIntakeHandoff(companyId: string, userId: string, assignmentId: string, base: string, intent: Exclude<IntakeIntent, "unknown">) {
  try { sessionStorage.removeItem(guidedIntakeHandoffKey(companyId, userId, assignmentId, base, intent)); } catch {}
}
