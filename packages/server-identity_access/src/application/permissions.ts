import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { PermissionActor } from "@erpx/feature-identity_access/lib/iam";
import type { IdentityError } from "@erpx/server-identity_access/application/authorization";
export function createRequirePermission(ports: {
    error: IdentityError;
    forbidden(): never;
}) { return <T extends PermissionActor>(actor: T, permission: string): T => { if (!hasPermission(actor, permission)) {
    if ((actor as PermissionActor & {
        pageRequest?: true;
    }).pageRequest)
        ports.forbidden();
    throw ports.error(403, 'PERMISSION_REQUIRED', 'Your selected role does not grant this permission.');
} return actor; }; }
