import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function projectStructureSources(db: SqlDatabase) {
    return { async currentProjectAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number) { return db.prepare("SELECT 1").bind().first(); },
        async currentWorkAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number) { return db.prepare("SELECT 1").bind().first(); } };
}
