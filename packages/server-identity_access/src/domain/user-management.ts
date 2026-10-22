import {BusinessRuleError} from '@erpx/server-shared/index';import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";import {validateAttributeValue as validateValue,type AttributeDefinition,type ManagementChange,type ManagementSnapshot,type ManagementImpact} from "@erpx/feature-identity_access/lib/user-management";import type {IamActor} from '@erpx/server-identity_access/domain/iam-actor';
export function checkedAttributeValue(field: AttributeDefinition, value: unknown) {
  try {
    return validateValue(field, value);
  } catch (error) {
    throw new BusinessRuleError(
      422,
      "ATTRIBUTE_INVALID",
      error instanceof Error ? error.message : "Check the attribute value.",
    );
  }
}
export function fail(
  message: string,
  code = "MANAGEMENT_CHANGED",
  status = 409,
): never {
  throw new BusinessRuleError(status, code, message);
}
export function commandPermission(change: ManagementChange, actor: IamActor) {
  if (
    (["group", "membership"].includes(change.operation) ||
      (change.operation === "values" && change.input.target === "group")) &&
    hasPermission(actor, "iam.directory.manage")
  )
    return "iam.directory.manage";
  return change.operation === "attribute"
    ? "iam.attributes.manage"
    : change.operation === "values"
      ? change.input.target === "person"
        ? "people.custom_attributes.write"
        : change.input.target === "unit"
          ? "organization.manage"
          : "iam.manage"
      : "iam.manage";
}
export function managementImpact(
  before: ManagementSnapshot,
  after: ManagementSnapshot,
): ManagementImpact {
  const memberships = (s: ManagementSnapshot) =>
    s.groups.flatMap((g) =>
      g.members.map(
        (p) => `${s.people.find((v) => v.id === p)?.name ?? p} → ${g.name}`,
      ),
    );
  const old = memberships(before),
    next = memberships(after);
  return {
    membersAdded: next.filter((v) => !old.includes(v)),
    membersRemoved: old.filter((v) => !next.includes(v)),
    accessRemoved: before.candidates
      .filter(
        (v) =>
          ["active", "pending"].includes(v.status) &&
          !after.candidates.some((n) => n.id === v.id && n.status === v.status),
      )
      .map(
        (v) =>
          `${before.people.find((p) => p.id === v.personId)?.name} · ${v.roleId}`,
      ),
    accessPending: after.candidates
      .filter(
        (v) =>
          v.status === "pending" &&
          !before.candidates.some((o) => o.id === v.id),
      )
      .map(
        (v) =>
          `${after.people.find((p) => p.id === v.personId)?.name} · ${v.roleId}`,
      ),
    accessBlocked: after.candidates
      .filter(
        (v) =>
          v.status === "blocked" &&
          !before.candidates.some((o) => o.id === v.id),
      )
      .map(
        (v) =>
          (after.people.find((p) => p.id === v.personId)?.name ?? "Person") +
          " · " +
          v.explanation,
      ),
    matchingPeople: [],
  };
}
export type ManagementObjectKind='attribute'|'group'|'policy'|'attachment';
