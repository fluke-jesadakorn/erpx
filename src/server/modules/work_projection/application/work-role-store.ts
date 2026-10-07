import type { ActiveWorkSelection, SelectWorkRole } from "../domain/work-role-store";
import type { Clock, IdentifierSource } from "../../shared/application/ports";
export type WorkRoleUnit = {
    current(sessionId: string, userId: string, companyId: string): Promise<ActiveWorkSelection | null>;
    preference(userId: string, companyId: string): Promise<{
        assignment_id: string;
        assignment_version: number;
    } | null>;
    select(input: SelectWorkRole, now: number, key: string, auditId: string): Promise<void>;
};
export function createWorkRoleStore(ports: {
    clock: Clock;
    identifiers: IdentifierSource;
    error(status: number, code: string, message: string): Error;
    errorCode(error: unknown): string | undefined;
}) {
    async function readWorkSelection(db: WorkRoleUnit, sessionId: string, userId: string, companyId: string) {
        return db.current(sessionId, userId, companyId);
    }
    async function selectWorkRole(db: WorkRoleUnit, input: {
        sessionId: string;
        userId: string;
        companyId: string;
        assignmentId: string;
        assignmentVersion: number;
        revision: number;
        expiresAt: number;
        restoring?: boolean;
        rememberPreference?: boolean;
    }) {
        const { sessionId, userId, companyId } = input, now = ports.clock.now();
        const key = ports.identifiers.next();
        try {
            await db.select(input, now, key, ports.identifiers.next());
        }
        catch (error) {
            if ((error as {
                code?: string;
            }).code === '23514')
                throw ports.error(409, 'WORK_CONTEXT_CHANGED', 'Your access or work role changed. Review and choose again.');
            throw error;
        }
        return (await readWorkSelection(db, sessionId, userId, companyId))!;
    }
    // Saved preferences initialize a new session; they never replace a live session's
    // role or revision, so another device cannot change an open form's authority.
    async function restoreWorkSelection(db: WorkRoleUnit, input: {
        sessionId: string;
        userId: string;
        companyId: string;
        expiresAt: number;
    }, assignments: {
        id: string;
        version?: number;
    }[]) {
        const current = await readWorkSelection(db, input.sessionId, input.userId, input.companyId);
        if (current)
            return current;
        const preference = await db.preference(input.userId, input.companyId);
        const assignment = preference && assignments.find(a => a.id === preference.assignment_id && a.version === preference.assignment_version);
        if (!assignment)
            return null;
        try {
            return await selectWorkRole(db, { ...input, assignmentId: assignment.id, assignmentVersion: assignment.version!, revision: 0, restoring: true });
        }
        catch (error) {
            if (ports.errorCode(error) === 'WORK_CONTEXT_CHANGED')
                return readWorkSelection(db, input.sessionId, input.userId, input.companyId);
            throw error;
        }
    }
    /** Initialize only the requested company's context; never move to another workspace. */
    async function initializeWorkSelection(db: WorkRoleUnit, input: {
        sessionId: string;
        userId: string;
        companyId: string;
        expiresAt: number;
    }, assignments: {
        id: string;
        version?: number;
    }[], eligible: {
        id: string;
        version?: number;
    }[]) {
        let selection = await restoreWorkSelection(db, input, assignments);
        if (!selection && assignments.length === 1 && eligible.length === 1) {
            const assignment = eligible[0];
            try {
                selection = await selectWorkRole(db, { ...input, assignmentId: assignment.id, assignmentVersion: assignment.version!, revision: 0 });
            }
            catch (error) {
                selection = await readWorkSelection(db, input.sessionId, input.userId, input.companyId);
                if (!selection)
                    throw error;
            }
        }
        return selection;
    }
    return { readWorkSelection, selectWorkRole, restoreWorkSelection, initializeWorkSelection };
}
