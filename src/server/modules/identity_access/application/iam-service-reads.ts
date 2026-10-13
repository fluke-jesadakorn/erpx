import { z } from 'zod';
import type { IamActor } from '../domain/iam-actor';
import { iamPermission } from '../domain/iam-actor';
import type { RoleRevision } from "@/lib/identity_access/iam";
import { permissionCatalog, hasPermission, evaluateAccess } from "@/lib/identity_access/permissions";
import type { IdentityError } from './authorization';
export interface IamServiceReadPorts {
    error: IdentityError;
    now(): number;
    companyScope(actor: IamActor): void;
    authorize(actor: IamActor, permission: string): unknown;
    readyControl(): Promise<void>;
    roleIds(companyId: string): Promise<{
        id: string;
    }[]>;
    role(companyId: string, id: string): Promise<RoleRevision | null>;
    auditPage(actor: IamActor, needle: string, after: {
        createdAt: number;
        id: string;
    } | null): Promise<Record<string, unknown>[]>;
    assignments(actor: IamActor, self: boolean): Promise<Record<string, unknown>[]>;
    people(actor: IamActor): Promise<{
        id: string;
        user_id: string | null;
        legal_name: string;
    }[]>;
    nodes(actor: IamActor): Promise<Record<string, unknown>[]>;
    migrations(actor: IamActor): Promise<Record<string, unknown>[]>;
    users(ids: string[]): Promise<{
        id: string;
        name: string;
        email: string;
    }[]>;
    mfaStatus(actor: IamActor): Promise<{
        enabled: boolean;
    } | null>;
    campaigns(actor: IamActor): Promise<Record<string, unknown>[]>;
    campaignItems(actor: IamActor, accessReviewCampaigns: Record<string, unknown>[]): Promise<Record<string, unknown>[]>;
    sessions(actor: IamActor): Promise<Record<string, unknown>[]>;
    connections(actor: IamActor, canManageDirectory: boolean): Promise<Record<string, unknown>[]>;
    groups(actor: IamActor): Promise<Record<string, unknown>[]>;
    previews(actor: IamActor): Promise<Record<string, unknown>[]>;
    mappings(actor: IamActor): Promise<Record<string, unknown>[]>;
    identities(actor: IamActor): Promise<Record<string, unknown>[]>;
    reviews(actor: IamActor): Promise<Record<string, unknown>[]>;
    recentAudit(actor: IamActor): Promise<Record<string, unknown>[]>;
    active(r: Record<string, unknown>, now: number): Promise<{
        active: boolean;
    } | null>;
}
function canReadCompany(actor: IamActor) {
    return (["holding", "platform"].includes(actor.scopeKind) &&
        hasPermission(actor, iamPermission(actor, "iam.manage")));
}
export function createIamServiceReads(ports: IamServiceReadPorts) {
    async function listIamRoles(companyId: string): Promise<RoleRevision[]> {
        const rows = await ports.roleIds(companyId);
        return (await Promise.all(rows.map((r) => ports.role(companyId, r.id)))).filter((r): r is RoleRevision => !!r);
    }
    async function iamAuditHistory(actor: IamActor, search: string, cursor?: string) {
        ports.companyScope(actor);
        ports.authorize(actor, iamPermission(actor, "iam.read"));
        let after: {
            createdAt: number;
            id: string;
        } | null = null;
        if (cursor) {
            try {
                after = z
                    .object({
                    createdAt: z.number().int().nonnegative(),
                    id: z.string().uuid(),
                })
                    .parse(JSON.parse(cursor));
            }
            catch {
                throw ports.error(422, "INVALID_CURSOR", "Refresh the audit history.");
            }
        }
        const needle = `%${search.slice(0, 200).replace(/[\\%_]/g, "\\$&")}%`;
        const rows = await ports.auditPage(actor, needle, after);
        const items = rows.slice(0, 100), last = items.at(-1);
        return {
            items,
            nextCursor: rows.length > 100 && last
                ? JSON.stringify({
                    createdAt: Number(last.created_at),
                    id: String(last.id),
                })
                : null,
        };
    }
    async function iamOverview(actor: IamActor, self = false) {
        ports.authorize(actor, iamPermission(actor, self ? "iam.self" : "iam.read"));
        const roles = await listIamRoles(actor.companyId);
        const rows = await ports.assignments(actor, self);
        const people = !actor.platform && !self
            ? await ports.people(actor)
            : [];
        const nodes = !actor.platform
            ? await ports.nodes(actor)
            : [];
        const migrations = !self && canReadCompany(actor)
            ? await ports.migrations(actor)
            : [];
        const ids = [
            ...new Set([
                ...rows.map((r) => String(r.user_id)),
                ...people.flatMap((p) => (p.user_id ? [p.user_id] : [])),
            ]),
        ];
        if (self && !ids.includes(actor.userId))
            ids.push(actor.userId);
        await ports.readyControl();
        const users = ids.length
            ? await ports.users(ids)
            : [];
        const ownStatus = await ports.mfaStatus(actor);
        const accessReviewCampaigns = !actor.platform && !self && hasPermission(actor, iamPermission(actor, "iam.review")) ? await ports.campaigns(actor) : [];
        const accessReviewItems = accessReviewCampaigns.length ? await ports.campaignItems(actor, accessReviewCampaigns) : [];
        const ownSessions = await ports.sessions(actor);
        const canManageDirectory = hasPermission(actor, iamPermission(actor, "iam.directory.manage")) &&
            ["holding", "platform"].includes(actor.scopeKind);
        const connections = !self
            ? await ports.connections(actor, canManageDirectory)
            : [];
        const canManageMappings = hasPermission(actor, iamPermission(actor, "iam.manage"));
        const groups: Record<string, unknown>[] = !self && (canManageDirectory || canManageMappings)
            ? await ports.groups(actor)
            : [];
        const previews = !self && canManageDirectory
            ? await ports.previews(actor)
            : [];
        for (const preview of previews)
            for (const group of (preview.snapshot_json as {
                groups: {
                    externalId: string;
                    name: string;
                }[];
            }).groups)
                if (!groups.some((g) => g.connection_id === preview.connection_id &&
                    g.external_id === group.externalId))
                    groups.push({
                        connection_id: preview.connection_id,
                        external_id: group.externalId,
                        name: group.name,
                        observed_at: preview.created_at,
                        preview: true,
                    });
        const mappings = !self
            ? await ports.mappings(actor)
            : [];
        const identities = !self && canManageDirectory
            ? await ports.identities(actor)
            : [];
        const reviews = !self
            ? await ports.reviews(actor)
            : [];
        const audit = !self && ["holding", "platform"].includes(actor.scopeKind)
            ? await ports.recentAudit(actor)
            : [];
        const catalog = permissionCatalog.filter((p) => (p.namespace === "platform") === actor.platform);
        const validRows: (Record<string, unknown> & {
            active: boolean;
        })[] = await Promise.all(rows.map(async (r) => {
            const active = await ports.active(r, ports.now());
            return {
                ...r,
                active: !!active?.active &&
                    (!r.identity_status || r.identity_status === "active") &&
                    (r.login_policy !== "directory" ||
                        (r.directory_enabled === true &&
                            r.connection_status === "active" &&
                            Number(r.observed_at) > ports.now() - 300000)),
            };
        }));
        const decisions = validRows.map((r) => {
            const role = roles.find((v) => v.id === r.role);
            return {
                assignmentId: String(r.id),
                userId: String(r.user_id),
                decisions: catalog.map((p) => ({
                    ...evaluateAccess({
                        userId: String(r.user_id),
                        role: String(r.role),
                        permissions: role?.permissions ?? [],
                        policyRevision: role?.revision,
                        assignmentId: String(r.id),
                        assignmentScope: String(r.scope_kind),
                    }, p.id, {
                        inspect: true,
                        valid: r.active,
                        expiresAt: Number(r.valid_until),
                        source: r.source === "directory"
                            ? `Directory group: ${r.group_name ?? r.group_id ?? "Unavailable"} · mapping ${r.source_id}`
                            : r.source === "policy"
                                ? `Reviewed access policy · evidence ${r.source_id}`
                                : `Manual assignment${r.parent_id ? " · parent " + r.parent_id : ""}`,
                    }),
                    scope: `${r.scope_kind}${r.scope_node_id ? " · " + (nodes.find((n) => n.id === r.scope_node_id)?.display_name ?? r.scope_node_id) : " · " + actor.companyId}`,
                })),
            };
        });
        return {
            evaluatedAt: ports.now(),
            previews,
            people,
            nodes,
            migrations,
            companyId: actor.companyId,
            scopeKind: actor.scopeKind,
            validUntil: Number(rows.find((r) => r.id === actor.assignmentId)?.valid_until ?? ports.now()),
            roles,
            users,
            assignments: validRows,
            connections,
            groups,
            mappings,
            identities,
            reviews,
            accessReviewCampaigns,
            accessReviewItems,
            audit,
            catalog,
            decisions,
            ownSessions,
            currentSessionId: actor.sessionId,
            mfaEnabled: !!ownStatus?.enabled,
            currentUserId: actor.userId,
            currentAssignmentId: actor.assignmentId,
            permissions: actor.iam?.permissions ?? [],
            platform: actor.platform,
            grantablePermissions: actor.iam?.grantablePermissions ?? [],
        };
    }
    return { listIamRoles, iamAuditHistory, iamOverview };
}
