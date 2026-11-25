import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {
  nativeProjectHandoffProject,
  nativeProjectHandoffWork,
  nativeProjectHandoffs,
  type NativeHandoffRow,
  type NativeHandoffWorkRow,
  type ProjectActualsActor,
} from './native-lane-27/client';

/**
 * Projects-owned handoff reads are executed by the lane 27 native axum + sqlx
 * owner. The retained repository methods carry only bounded identities; there
 * is no TypeScript handoff SQL and no fallback reader.
 */
export function projectStructureSources(db: SqlDatabase, caller: ProjectActualsActor) {
    void db; // retained repository signature; no SQL is issued here
    return {
        /** Graph actuals are owned by the mounted native Projects graph read;
         * there is no TypeScript SQL reader. Kept only to satisfy the shared
         * repository port used by the retained handoff reads. */
        async graphActuals() { throw new Error('PROJECT_GRAPH_NATIVE_ONLY'); },
        async handoffs(companyId: string, id: string): Promise<{ results: NativeHandoffRow[] }> {
            return { results: await nativeProjectHandoffs({ ...caller, companyId }, id) };
        },
        async handoffProject(id: string, companyId: string) {
            return nativeProjectHandoffProject({ ...caller, companyId }, id);
        },
        async handoffWork(companyId: string, userId: string, _userId2: string, _userId3: string, _userId4: string): Promise<{ results: NativeHandoffWorkRow[] }> {
            return { results: await nativeProjectHandoffWork({ ...caller, companyId, userId }) };
        },
    };
}
