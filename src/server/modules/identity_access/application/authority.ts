import type { PermissionActor, RoleRevision } from "@/lib/identity_access/iam";
import type { IdentityError } from "./authorization";
export type IamAuthorityMember = {
    status: string;
    login_policy: string;
    version: number;
    person_id: string | null;
    principal_id: string;
    enabled: boolean;
    observed_at: number;
    connection_status: string;
};
export interface IamAuthorityPorts {
    role(companyId: string, id: string): Promise<RoleRevision | null>;
    member(companyId: string, userId: string): Promise<IamAuthorityMember | null>;
    person(companyId: string, userId: string): Promise<string | null>;
    aliases(companyId: string, personId: string): Promise<string[]>;
    emergency(companyId: string, userId: string, sessionId: string, now: number): Promise<{
        expires_at: number;
    } | null>;
    hrAppointment(companyId: string, userId: string, now: number): Promise<{
        version: number;
    } | null>;
    now(): number;
    error: IdentityError;
}
export function createIamAuthority(ports: IamAuthorityPorts) { return async (companyId: string, userId: string, roleId: string, sessionId?: string): Promise<NonNullable<PermissionActor['iam']>> => { const [role, member] = await Promise.all([ports.role(companyId, roleId), ports.member(companyId, userId)]); if (!role)
    throw ports.error(403, 'ROLE_UNAVAILABLE', 'This role is unavailable.'); if (member && member.status !== 'active')
    throw ports.error(403, 'IDENTITY_SUSPENDED', 'This identity no longer has access to this company.'); if (member?.login_policy === 'directory' && (!member.enabled || member.connection_status !== 'active' || member.observed_at <= ports.now() - 300000))
    throw ports.error(403, 'DIRECTORY_ACCESS_UNAVAILABLE', 'Your directory access needs a successful refresh. Contact your administrator.'); const personId = member?.person_id ?? (companyId === 'platform' ? null : await ports.person(companyId, userId)), aliases = personId ? await ports.aliases(companyId, personId) : []; let emergencyUntil: number | null = null; if (member?.login_policy === 'emergency') {
    const elevation = await ports.emergency(companyId, userId, sessionId ?? '', ports.now());
    if (!elevation)
        throw ports.error(403, 'EMERGENCY_ACCESS_REQUIRED', 'Verify MFA and activate a temporary emergency session in account security.');
    emergencyUntil = Number(elevation.expires_at);
} const hrAppointment = companyId === 'platform' ? null : await ports.hrAppointment(companyId, userId, ports.now()), permissions = effectiveAuthorityPermissions(roleId, role.permissions, !!hrAppointment); return { personUserIds: [...new Set([userId, ...aliases])], emergencyUntil, roleRevision: role.revision, identityVersion: member?.version ?? 0, permissions, hrAppointmentVersion: hrAppointment?.version, grantablePermissions: role.grantablePermissions, principalId: member?.principal_id ?? `local:${userId}`, personId, freshUntil: member?.login_policy === 'directory' ? Number(member.observed_at) + 300000 : null }; }; }
export function createPersonIdentityIds(ports: {
    aliases(companyId: string, ids: readonly string[]): Promise<{
        user_id: string;
    }[]>;
}) { return async (companyId: string, ids: readonly string[]) => { if (!ids.length)
    return []; return [...new Set([...ids, ...(await ports.aliases(companyId, ids)).map(row => row.user_id)])]; }; }
export function createCurrentPersonPrincipals(ports: {
    successors(companyId: string, ids: readonly string[]): Promise<{
        original_id: string;
        current_id: string;
    }[]>;
}) { return async (companyId: string, ids: readonly string[]) => { if (!ids.length)
    return new Map<string, string>(); const rows = await ports.successors(companyId, ids), map = new Map(ids.map(id => [id, id])); for (const id of ids) {
    const matches = rows.filter(row => row.original_id === id);
    if (matches.length === 1)
        map.set(id, matches[0].current_id);
} return map; }; }

export function effectiveAuthorityPermissions(roleId: string, permissions: readonly string[], hasHrAppointment: boolean) {
  const appointment = ["people.read", "people.manage", "leave.policy.manage", "module.people_payroll.read"];
  const base = roleId === "hr_admin" && !hasHrAppointment ? permissions.filter(permission => !appointment.includes(permission)) : [...permissions];
  return hasHrAppointment ? [...new Set([...base, ...appointment])] : base;
}
