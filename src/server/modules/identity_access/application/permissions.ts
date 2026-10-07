import { hasPermission } from "@/lib/permissions";
import type { PermissionActor } from "@/lib/iam";
import type { IdentityError } from "./authorization";
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
