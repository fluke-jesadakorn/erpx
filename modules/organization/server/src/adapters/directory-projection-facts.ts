import "server-only";
import {headers} from 'next/headers';
import {companyIdProjection} from './organization-column-alias-families';
import {activeCompanyNodes} from './organization-read-tail-sources-r42';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { type CompanyRow, type NodeRow, type LevelRow, type PositionRow, type DirectorySelectionRecord } from '@erpx/people-payroll-server/public/directory-projection-contract';
import {nativeDirectoryLevels, nativeDirectoryNodes, nativeDirectoryPositions} from './native-lane-23/client';
export function peopleDirectoryHierarchyFacts(database: SqlDatabase) {
    return { readDirectoryFacts2() {
            return database.prepare(activeCompanyNodes('id,slug,display_name', '\n      ', 'display_name,id')).all<CompanyRow>();
        },
        /** Native owner read: the SQL, projection and Holding scope live in
         * `erpx-server-organization::adapters::native_lane_23`; this adapter only
         * transports the validated facts and fails closed when unavailable. */
        async readDirectoryFacts3(): Promise<{ results: NodeRow[] }> {
            return { results: await nativeDirectoryNodes(await headers()) };
        },
        async readDirectoryFacts4(): Promise<{ results: LevelRow[] }> {
            return { results: await nativeDirectoryLevels(await headers()) };
        },
        async readDirectoryFacts5(): Promise<{ results: PositionRow[] }> {
            return { results: await nativeDirectoryPositions(await headers()) };
        },
        readDirectoryFacts10(id: string) {
            return database.prepare(`${companyIdProjection('id', 'organization_nodes')}
      WHERE id=? AND kind='company' AND status='active'`).bind(id).first<DirectorySelectionRecord>();
        },
        readDirectoryFacts11(id: string) {
            return database.prepare(`${companyIdProjection('p.company_id', 'organization_positions p')}
      JOIN organization_nodes c ON c.id=p.company_id AND c.kind='company' AND c.status='active'
      WHERE p.id=? AND p.active=1`).bind(id).first<DirectorySelectionRecord>();
        },
        readDirectoryFacts12(id: string) {
            return database.prepare(`${companyIdProjection('l.company_id', 'staff_levels l')}
      JOIN organization_nodes c ON c.id=l.company_id AND c.kind='company' AND c.status='active'
      WHERE l.id=? AND l.active=1`).bind(id).first<DirectorySelectionRecord>();
        },
        readDirectoryFacts13(id: string, kind: "branch" | "department" | "team", kind2: "branch" | "department" | "team") {
            return database.prepare(`${companyIdProjection('n.company_id', 'organization_nodes n')}
    JOIN organization_nodes c ON c.id=n.company_id AND c.kind='company' AND c.status='active'
    WHERE n.id=? AND n.status='active' AND (n.kind=? OR (?='branch' AND n.kind='headquarters'))`)
                .bind(id, kind, kind2).first<DirectorySelectionRecord>();
        } };
}
