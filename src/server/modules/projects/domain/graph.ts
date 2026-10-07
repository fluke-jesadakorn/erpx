import type { ProjectRow } from "./staff-projects";
import type { ProjectMilestoneRow } from "./milestones";
import { BusinessRuleError } from "../../shared/domain/error";
export type Project = ProjectRow & {
    lifecycle_status: 'active' | 'handoff_pending' | 'completed' | 'cancelled';
};
export type GraphTask = {
    id: string;
    title: string;
    state: string;
    version: number;
    assignee_user_id: string;
    milestone_id: string | null;
    binding_version: number | null;
};
export type Actual = {
    id: string;
    milestone_id: string;
    source_kind: 'work_log' | 'cost_allocation';
    source_id: string;
    source_version: number;
    minutes: number | null;
    amount_minor: number | null;
    currency: string | null;
    user_id: string | null;
    current_source_version: number;
    source_state: string;
    journal_status: string | null;
    journal_id: string | null;
};
export type GraphPackage = ProjectMilestoneRow & {
    wbs: string;
    depth: number;
    tasks: GraphTask[];
    approvedMinutes: number;
    costByCurrency: Record<string, string>;
};
export type ProjectGraph = {
    project: Project;
    packages: GraphPackage[];
    tasks: GraphTask[];
    actuals: Actual[];
    edges: {
        from: string;
        to: string;
        kind: 'contains' | 'requires' | 'task';
    }[];
    unassignedTasks: GraphTask[];
    canReadCosts: boolean;
    isProjectOwner: boolean;
    totals: {
        approvedMinutes: number;
        costByCurrency: Record<string, string>;
    };
};
export type ProjectGraphSource = {
    project: Project;
    milestones: ProjectMilestoneRow[];
    tasks: GraphTask[];
    actuals: Actual[];
};
export type ProjectGraphViewer = {
    isOwner: boolean;
    canReadCosts: boolean;
    isSamePerson(userId: string | null): boolean;
};
export function sourceCurrent(actual: Actual) { return actual.source_version === actual.current_source_version && actual.source_state === 'approved' && (actual.source_kind === 'work_log' || actual.journal_status === 'posted'); }
export function addCost(result: Record<string, string>, currency: string, minor: number) { result[currency] = (BigInt(result[currency] ?? '0') + BigInt(minor)).toString(); }
export function buildProjectGraph(rows: ProjectGraphSource, viewer: ProjectGraphViewer, includeAll = false): ProjectGraph {
    const { project: p, milestones, tasks } = rows, isOwner = viewer.isOwner, canReadCosts = includeAll || viewer.canReadCosts, actuals = rows.actuals.filter(source => includeAll || (source.source_kind === 'cost_allocation' ? canReadCosts : isOwner || viewer.isSamePerson(source.user_id))), packages: GraphPackage[] = [], seen = new Set<string>(), edges: ProjectGraph['edges'] = [], totalCosts: Record<string, string> = {};
    let totalMinutes = 0;
    const walk = (parent: string | null, prefix: string, depth: number) => {
        const children = milestones.filter(m => (m.parent_milestone_id ?? null) === parent).sort((left, right) => (left.ordinal_position ?? 0) - (right.ordinal_position ?? 0) || left.id.localeCompare(right.id));
        for (const [index, m] of children.entries()) {
            if (seen.has(m.id))
                throw new BusinessRuleError(409, 'PROJECT_PACKAGE_CYCLE', 'The project structure requires recovery before it can be read.');
            seen.add(m.id);
            const wbs = prefix ? `${prefix}.${index + 1}` : String(index + 1), costs: Record<string, string> = {};
            let minutes = 0;
            for (const source of actuals.filter(s => s.milestone_id === m.id && sourceCurrent(s))) {
                if (source.source_kind === 'work_log')
                    minutes += Number(source.minutes);
                else
                    addCost(costs, source.currency!, Number(source.amount_minor));
            }
            totalMinutes += minutes;
            for (const [currency, amount] of Object.entries(costs))
                totalCosts[currency] = (BigInt(totalCosts[currency] ?? '0') + BigInt(amount)).toString();
            packages.push({ ...m, wbs, depth, tasks: tasks.filter(task => task.milestone_id === m.id), approvedMinutes: minutes, costByCurrency: costs });
            if (parent)
                edges.push({ from: parent, to: m.id, kind: 'contains' });
            for (const predecessor of m.depends_on_ids)
                edges.push({ from: predecessor, to: m.id, kind: 'requires' });
            for (const task of tasks.filter(t => t.milestone_id === m.id))
                edges.push({ from: m.id, to: task.id, kind: 'task' });
            walk(m.id, wbs, depth + 1);
        }
    };
    walk(null, '', 0);
    if (seen.size !== milestones.length)
        throw new BusinessRuleError(409, 'PROJECT_PACKAGE_CYCLE', 'The project structure requires recovery before it can be read.');
    return { project: p, packages, tasks, actuals, edges, unassignedTasks: tasks.filter(t => !t.milestone_id), canReadCosts, isProjectOwner: isOwner, totals: { approvedMinutes: totalMinutes, costByCurrency: totalCosts } };
}
