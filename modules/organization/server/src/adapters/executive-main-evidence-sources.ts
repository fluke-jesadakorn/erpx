import "server-only";
import {headers} from 'next/headers';
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { OrganizationNodeEvidenceRow, PendingStructureEvidenceRow } from '@erpx/platform-runtime-server/public/executive/evidence-contract';
import {nativeExecutiveEvidence} from './native-lane-23/client';

type Cursor={name:string|undefined;at:number|undefined;id:string};
type EvidenceInput={query:string;limit:number};

/** Native owner reads: the SQL, cursor binds and projection live in the Rust
 * organization crate; these adapters transport validated facts only. */
export function executiveMainEvidenceSources(_db: SqlDatabase) {
    return {
        async factsSearchOrganizationNodeEvidence(cursor: Cursor|null, input: EvidenceInput, actor: WorkContext): Promise<{
            rows: OrganizationNodeEvidenceRow[];
        }> {
            if (cursor && (cursor.name === undefined || cursor.at !== undefined))
                throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
            const evidence = await nativeExecutiveEvidence(await headers(), {
                kind: 'node-evidence',
                companyId: actor.companyId,
                values: [actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId],
                query: input.query,
                limit: input.limit + 1,
                ...(cursor ? {cursorName: cursor.name, cursorId: cursor.id} : {}),
            });
            return {rows: evidence.rows as unknown as OrganizationNodeEvidenceRow[]};
        },
        async factsReadOrganizationNodeEvidence(actor: WorkContext, id: string): Promise<{
            row: OrganizationNodeEvidenceRow | null;
        }> {
            const evidence = await nativeExecutiveEvidence(await headers(), {
                kind: 'node-evidence',
                companyId: actor.companyId,
                values: [actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId],
                nodeId: id,
            });
            return {row: (evidence.rows[0] as unknown as OrganizationNodeEvidenceRow) ?? null};
        },
        async factsSearchPendingStructureEvidence(cursor: Cursor|null, input: EvidenceInput, actor: WorkContext): Promise<{
            rows: PendingStructureEvidenceRow[];
        }> {
            if (cursor && (cursor.at === undefined || cursor.name !== undefined))
                throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
            const evidence = await nativeExecutiveEvidence(await headers(), {
                kind: 'pending-evidence',
                companyId: actor.companyId,
                values: [actor.branchId, actor.branchId],
                query: input.query,
                limit: input.limit + 1,
                ...(cursor ? {cursorAt: cursor.at, cursorId: cursor.id} : {}),
            });
            return {rows: evidence.rows as unknown as PendingStructureEvidenceRow[]};
        },
        async factsReadPendingStructureEvidence(actor: WorkContext, id: string): Promise<{
            row: PendingStructureEvidenceRow | null;
        }> {
            const evidence = await nativeExecutiveEvidence(await headers(), {
                kind: 'pending-evidence',
                companyId: actor.companyId,
                values: [actor.branchId, actor.branchId],
                nodeId: id,
            });
            return {row: (evidence.rows[0] as unknown as PendingStructureEvidenceRow) ?? null};
        },
    };
}
