import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { type CompanyRow, type NodeRow, type LevelRow, type PositionRow, type DirectorySelectionRecord } from '@erpx/server-people_payroll/public/directory-projection-contract';
export function peopleDirectoryHierarchyFacts(database: SqlDatabase) {
    return { readDirectoryFacts2() {
            return database.prepare(`SELECT id,slug,display_name FROM organization_nodes
      WHERE kind='company' AND status='active' ORDER BY display_name,id`).all<CompanyRow>();
        },
        readDirectoryFacts3() {
            return database.prepare(`SELECT n.id,n.company_id,n.kind,n.slug,n.display_name,n.parent_id,n.branch_id
      FROM organization_nodes n JOIN organization_nodes c
        ON c.id=n.company_id AND c.kind='company' AND c.status='active'
      WHERE n.status='active' ORDER BY n.company_id,n.kind,n.display_name,n.id`).all<NodeRow>();
        },
        readDirectoryFacts4() {
            return database.prepare(`SELECT l.id,l.company_id,l.code,l.name,l.level_number
      FROM staff_levels l JOIN organization_nodes c
        ON c.id=l.company_id AND c.kind='company' AND c.status='active'
      WHERE l.active=1 ORDER BY l.company_id,l.level_number NULLS LAST,l.name,l.id`).all<LevelRow>();
        },
        readDirectoryFacts5() {
            return database.prepare(`SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id,
        p.reports_to_position_id,p.is_ceo
      FROM organization_positions p JOIN organization_nodes c
        ON c.id=p.company_id AND c.kind='company' AND c.status='active'
      WHERE p.active=1 ORDER BY p.company_id,p.title,p.id`).all<PositionRow>();
        },
        readDirectoryFacts10(id: string) {
            return database.prepare(`SELECT id AS "companyId" FROM organization_nodes
      WHERE id=? AND kind='company' AND status='active'`).bind(id).first<DirectorySelectionRecord>();
        },
        readDirectoryFacts11(id: string) {
            return database.prepare(`SELECT p.company_id AS "companyId" FROM organization_positions p
      JOIN organization_nodes c ON c.id=p.company_id AND c.kind='company' AND c.status='active'
      WHERE p.id=? AND p.active=1`).bind(id).first<DirectorySelectionRecord>();
        },
        readDirectoryFacts12(id: string) {
            return database.prepare(`SELECT l.company_id AS "companyId" FROM staff_levels l
      JOIN organization_nodes c ON c.id=l.company_id AND c.kind='company' AND c.status='active'
      WHERE l.id=? AND l.active=1`).bind(id).first<DirectorySelectionRecord>();
        },
        readDirectoryFacts13(id: string, kind: "branch" | "department" | "team", kind2: "branch" | "department" | "team") {
            return database.prepare(`SELECT n.company_id AS "companyId" FROM organization_nodes n
    JOIN organization_nodes c ON c.id=n.company_id AND c.kind='company' AND c.status='active'
    WHERE n.id=? AND n.status='active' AND (n.kind=? OR (?='branch' AND n.kind='headquarters'))`)
                .bind(id, kind, kind2).first<DirectorySelectionRecord>();
        } };
}
