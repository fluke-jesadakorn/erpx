import type { WorkContext } from '../../identity_access/public/authorization';
import type { HoldingPeopleDirectory, HoldingDirectoryPerson } from '../../organization/public/holding-people-directory';
import { type CompanyRow, type NodeRow, type LevelRow, type PositionRow, type PersonRow, type DirectorySelection, type DirectorySelectionRecord,type SelectedPersonRow, archivedScope } from '../domain/directory-projection';
import type { Clock } from '../../shared';
export interface DirectoryProjectionRepository {
    readDirectoryFacts1(assignmentId: string, userId: string, role: string, companyId: string, assignmentScope: WorkContext["assignmentScope"], now: number, now2: number): Promise<Record<string, unknown> | null>;
    readDirectoryFacts2(): Promise<{
        results: (CompanyRow)[];
    }>;
    readDirectoryFacts3(): Promise<{
        results: (NodeRow)[];
    }>;
    readDirectoryFacts4(): Promise<{
        results: (LevelRow)[];
    }>;
    readDirectoryFacts5(): Promise<{
        results: (PositionRow)[];
    }>;
    readDirectoryFacts6(): Promise<{
        results: (PersonRow)[];
    }>;
    readDirectoryFacts7(value: (string | null)[]): Promise<{
        results: (PersonRow)[];
    }>;
    readDirectoryFacts8(id: string): Promise<SelectedPersonRow | null>;
    readDirectoryFacts9(value: (string | null)[], id: string): Promise<SelectedPersonRow | null>;
    readDirectoryFacts10(id: string): Promise<DirectorySelectionRecord | null>;
    readDirectoryFacts11(id: string): Promise<DirectorySelectionRecord | null>;
    readDirectoryFacts12(id: string): Promise<DirectorySelectionRecord | null>;
    readDirectoryFacts13(id: string, kind: "branch" | "department" | "team", kind2: "branch" | "department" | "team"): Promise<DirectorySelectionRecord | null>;
}
export function createDirectoryProjectionApplication(ports: {
    repository: DirectoryProjectionRepository;
    clock: Clock;
}) {
    const repository = ports.repository;
    async function directoryAssignmentActive(actor: WorkContext) {
        const now = ports.clock.now();
        return !!await repository.readDirectoryFacts1(actor.assignmentId, actor.userId, actor.role, actor.companyId, actor.assignmentScope, now, now);
    }
    async function readHoldingPeopleDirectory(): Promise<HoldingPeopleDirectory> {
        // Every query runs against the resource bound to actor.holdingId. JOINs to
        // active Company roots prevent malformed cross-company references leaking in.
        const [companiesResult, nodesResult, levelsResult, positionsResult, peopleResult] = await Promise.all([
            repository.readDirectoryFacts2(),
            repository.readDirectoryFacts3(),
            repository.readDirectoryFacts4(),
            repository.readDirectoryFacts5(),
            repository.readDirectoryFacts6(),
        ]);
        return {
            companies: companiesResult.results.map(row => ({ id: row.id, slug: row.slug, name: row.display_name })),
            nodes: nodesResult.results.map(row => ({
                id: row.id, companyId: row.company_id, kind: row.kind, slug: row.slug,
                name: row.display_name, parentId: row.parent_id, branchId: row.branch_id,
            })),
            levels: levelsResult.results.map(row => ({
                id: row.id, companyId: row.company_id, code: row.code, name: row.name, number: row.level_number,
            })),
            positions: positionsResult.results.map(row => ({
                id: row.id, companyId: row.company_id, branchId: row.branch_id,
                departmentId: row.department_id, title: row.title, levelId: row.level_id,
                reportsToPositionId: row.reports_to_position_id, isCeo: row.is_ceo === 1,
            })),
            people: peopleResult.results.map(row => ({
                id: row.engagement_id, personId: row.person_id, userId: row.user_id, engagementId: row.engagement_id,
                companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id,
                positionId: row.position_id, legalName: row.legal_name, jobTitle: row.job_title,
                workEmail: row.work_email, workPhone: row.work_phone,
            })),
        };
    }
    async function readArchivedPeopleDirectory(actor: WorkContext): Promise<HoldingDirectoryPerson[]> {
        const result = await repository.readDirectoryFacts7(archivedScope(actor));
        return result.results.map(row => ({
            id: row.engagement_id || `archived:${row.person_id}`, userId: row.user_id,
            personId: row.person_id, engagementId: row.engagement_id,
            companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id,
            positionId: row.position_id, legalName: row.legal_name, jobTitle: row.job_title,
            workEmail: row.work_email, workPhone: row.work_phone,
        }));
    }
    async function readDirectorySelection(actor: WorkContext, selection: DirectorySelection, includeArchived: boolean): Promise<DirectorySelectionRecord | null> {
        if (selection.kind === "person") {
            let person = await repository.readDirectoryFacts8(selection.id);
            if (!person && includeArchived) {
                person = await repository.readDirectoryFacts9(archivedScope(actor), selection.id);
            }
            return person ? { companyId: person.company_id, person: {
                    id: person.engagement_id, personId: person.person_id,
                    engagementId: person.engagement_id, companyId: person.company_id,
                } } : null;
        }
        if (selection.kind === "company" || selection.kind === "unassigned") {
            return repository.readDirectoryFacts10(selection.id);
        }
        if (selection.kind === "position") {
            return repository.readDirectoryFacts11(selection.id);
        }
        if (selection.kind === "level") {
            return repository.readDirectoryFacts12(selection.id);
        }
        return repository.readDirectoryFacts13(selection.id, selection.kind, selection.kind);
    }
    return { directoryAssignmentActive, readHoldingPeopleDirectory, readArchivedPeopleDirectory, readDirectorySelection };
}
