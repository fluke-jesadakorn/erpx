/** Shared, serializable IAM contracts. Provider credentials never cross this boundary. */
export type PermissionDefinition = {
  identityBased?: boolean;
  permittedScopes?: readonly string[];
  id: string;
  label: string;
  module: string;
  roles: readonly string[];
  risk: "normal" | "privileged";
  conditions: readonly string[];
  availability: "live" | "preview";
  namespace: "customer" | "platform";
};
export type RoleRevision = {
  id: string;
  name: string;
  companyId: string;
  revision: number;
  builtIn: boolean;
  permissions: string[];
  grantablePermissions: string[];
  scopes: string[];
  status: string;
};
export type ScopedAssignment = {
  id: string;
  userId: string;
  role: string;
  companyId: string;
  scopeKind: string;
  scopeNodeId: string | null;
  validFrom: number;
  validUntil: number;
  revokedAt: number | null;
  version: number;
  parentId: string | null;
  source: "manual" | "directory" | "legacy" | "emergency" | "policy" | "lifecycle" | "provisioning";
  sourceId: string | null;
};
export type DirectoryProvider = "active_directory" | "entra_id" | "univention";
export type DirectoryConfig = {
  mode: "ldaps" | "oidc";
  secretProfile: string;
  authority: string;
  ldapUrl?: string;
  baseDn?: string;
  userFilter?: string;
  groupBaseDn?: string;
  issuer?: string;
  tenantId?: string;
  clientId?: string;
};
export type DirectoryConnection = {
  id: string;
  companyId: string;
  name: string;
  provider: DirectoryProvider;
  config: DirectoryConfig;
  status: "draft" | "tested" | "active" | "disabled";
  version: number;
  testedVersion: number | null;
  lastSyncAt: number | null;
  health: string;
  lastError: string | null;
};
export type DirectoryIdentity = {
  id: string;
  connectionId: string;
  externalId: string;
  username: string;
  name: string;
  email: string | null;
  enabled: boolean;
  groupIds: string[];
  observedAt: number;
  userId: string | null;
  personId: string | null;
};
export type MigrationBatch = {
  id: string;
  companyId: string;
  status: string;
  version: number;
  connectionId: string;
  createdBy: string;
  users: {
    localUserId: string;
    identityId: string;
    personId: string;
    grants: unknown[];
  }[];
};
export type AccessDecision = {
  result: "allowed" | "denied" | "conditional" | "unavailable";
  permission: string;
  reasons: string[];
  conditions: string[];
  role: string;
  revision: number;
  assignmentId?: string;
  scope?: string;
  source?: string;
  expiresAt?: number;
};
export type PermissionActor = {
  scope?: { kind: string };
  userId?: string;
  role: string;
  permissions?: readonly string[];
  policyRevision?: number;
  assignmentId?: string;
  companyId?: string;
  assignmentScope?: string;
  iam?: {
    queuedAssignmentVersion?: number;
    roleRevision: number;
    identityVersion: number;
    permissions: string[];
    grantablePermissions: string[];
    principalId: string;
    personId: string | null;
    personUserIds?: string[];
    emergencyUntil?: number | null;
    freshUntil: number | null;
  };
};
export const directoryProviderLabels: Record<DirectoryProvider, string> = {
  active_directory: "Windows Active Directory",
  entra_id: "Microsoft Entra ID",
  univention: "Univention Corporate Server",
};
export const iamSections = [
  ["overview", "Overview"],
  ["users", "Users"],
  ["roles", "Roles & permissions"],
  ["groups", "Groups & mappings"],
  ["matrix", "Access matrix"],
  ["connections", "Directory connections"],
  ["reviews", "Reviews & audit"],
  ["my-access", "My access"],
] as const;
export type IamSection = (typeof iamSections)[number][0];

export function samePerson(
  actor: { userId: string; iam?: PermissionActor["iam"] },
  userId: unknown,
) {
  return (
    typeof userId === "string" &&
    (actor.userId === userId || !!actor.iam?.personUserIds?.includes(userId))
  );
}
