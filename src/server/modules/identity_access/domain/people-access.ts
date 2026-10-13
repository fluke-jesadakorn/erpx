import type { AccessSubject, PeopleAccessChange } from "@/lib/identity_access/people-access";
import { BusinessRuleError } from "../../shared";

export function requireExternalReady(subject: AccessSubject, now: number) {
  if (subject.kind !== "external") return;
  if (!subject.organizationId || !subject.sponsorPersonId || !subject.expiresAt || subject.expiresAt <= now || subject.status === "suspended")
    throw new BusinessRuleError(422, "EXTERNAL_ACCESS_NOT_READY", "Choose an external organization, an internal sponsor and a future expiry before enabling access.");
}
export function requireDelegatedRole(permissions: string[], ceiling: readonly string[]) {
  if (permissions.some(p => !ceiling.includes(p) || p === "company.owner" || p === "iam.recovery" || p.startsWith("platform.")))
    throw new BusinessRuleError(403, "DELEGATION_EXCEEDED", "The role exceeds your delegated permissions.");
}
export function requireAccessDeadline(change: Extract<PeopleAccessChange, { operation: "grant" | "invite" }>, subject: AccessSubject, authorizingUntil: number, now: number) {
  requireExternalReady(subject, now);
  if (change.validUntil <= now || change.validUntil > authorizingUntil || subject.kind === "external" && change.validUntil > subject.expiresAt!)
    throw new BusinessRuleError(422, "EXPIRY_INVALID", "Access must expire within the external relationship and your authorizing assignment.");
}
