import type { PermissionActor } from '@/lib/iam';
import type { IdentityError } from './authorization';
export type JobActor = PermissionActor & {
    userId: string;
    assignmentId: string;
    companyId: string;
    authority?: {
        sessionId: string;
        assignmentVersion: number;
    };
};
export function createCaptureJobAuthority(ports: {
    grant(actor: JobActor, now: number): Promise<{
        version: number;
    } | null>;
    authority(actor: JobActor): Promise<NonNullable<PermissionActor['iam']>>;
    now(): number;
    error: IdentityError;
}) {
    return async <T extends JobActor>(actor: T): Promise<T> => {
        const grant = await ports.grant(actor, ports.now());
        if (!grant)
            throw ports.error(403, "ASSIGNMENT_UNAVAILABLE", "The authorizing assignment is unavailable.");
        return {
            ...actor,
            iam: {
                ...(actor.iam ??
                    (await ports.authority(actor))),
                queuedAssignmentVersion: actor.authority?.assignmentVersion ?? grant.version,
            },
        };
    };
}
export function createRequireJobAuthority(ports: {
    error: IdentityError;
}) {
    return (actor: JobActor | null) => {
        if (!actor?.iam?.queuedAssignmentVersion)
            throw ports.error(409, "IAM_JOB_STALE", "This job predates the current access policy. Review and submit it again.");
    };
}
