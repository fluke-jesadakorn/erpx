import { evaluateAccess, permissionCatalog } from '@/lib/permissions';
import type { RoleRevision, AccessDecision } from '@/lib/iam';
export function createOwnIamAccess(ports: {
    holdings(): Promise<{
        id: string;
        displayName: string;
    }[]>;
    now(): number;
    assignments(holdingId: string, userId: string, now: number): Promise<Record<string, unknown>[]>;
    platformAssignments(userId: string, now: number): Promise<Record<string, unknown>[]>;
    role(holdingId: string | null, companyId: string, id: string): Promise<RoleRevision | null>;
}) {
    return async (userId: string) => {
        const rows: {
            id: string;
            company: string;
            role: string;
            scope: string;
            expiresAt: number;
            active: boolean;
            identity: string;
            decisions: AccessDecision[];
        }[] = [];
        for (const holding of await ports.holdings()) {
            const assignments = await ports.assignments(holding.id, userId, ports.now());
            for (const a of assignments) {
                const role = await ports.role(holding.id, String(a.company_id), String(a.role));
                rows.push({
                    id: String(a.id),
                    company: `${holding.displayName} · ${a.company_name}`,
                    role: role?.name ?? String(a.role),
                    scope: String(a.scope_name ?? "Company"),
                    expiresAt: Number(a.valid_until),
                    active: a.active === true,
                    identity: String(a.identity_status === "superseded"
                        ? "Directory required"
                        : (a.login_policy ?? "local")),
                    decisions: permissionCatalog
                        .filter((p) => p.namespace === "customer")
                        .map((p) => evaluateAccess({
                        userId,
                        role: String(a.role),
                        permissions: role?.permissions ?? [],
                        policyRevision: role?.revision,
                        assignmentId: String(a.id),
                        assignmentScope: String(a.scope_kind),
                    }, p.id, {
                        inspect: true,
                        valid: a.active === true,
                        expiresAt: Number(a.valid_until),
                        source: String(a.source),
                    })),
                });
            }
        }
        const platform = await ports.platformAssignments(userId, ports.now());
        for (const a of platform) {
            const role = await ports.role(null, "platform", String(a.role));
            rows.push({
                id: String(a.id),
                company: "Platform",
                role: role?.name ?? String(a.role),
                scope: "Platform",
                expiresAt: Number(a.valid_until),
                active: a.active === true,
                identity: "local",
                decisions: permissionCatalog
                    .filter((p) => p.namespace === "platform")
                    .map((p) => evaluateAccess({
                    userId,
                    role: String(a.role),
                    permissions: role?.permissions ?? [],
                    policyRevision: role?.revision,
                }, p.id, {
                    inspect: true,
                    valid: a.active === true,
                    expiresAt: Number(a.valid_until),
                })),
            });
        }
        return rows;
    };
}
