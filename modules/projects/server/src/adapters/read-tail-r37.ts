/** Byte-identical SQL frames for the Projects read tail (r37).
 * Only closed relations, aliases, predicates and source names vary; every
 * caller keeps its own binds, locks, result types and trailing clauses. */
export const notExistsRow = (relation: string, alias: string, predicate: string) =>
  `NOT EXISTS(SELECT 1 FROM ${relation} ${alias} WHERE ${predicate})`;

type FactUnionArm = {kind: string; head: string; tail: string};
type FactUnionSources = {readonly tasks: string; readonly templates: string; readonly schedules: string};
/** Closed milestone/task/routine-template/routine-schedule UNION group. The
 * call site supplies the leading newline and space; every later gap, CASE
 * WHEN, NOT IN list, IS DISTINCT FROM and LEFT JOIN stays byte-identical. */
export const projectFactUnionArms = (s: FactUnionSources) => {
  const arms: FactUnionArm[] = [
    {
      kind: 'milestone',
      head: 'm.id,m.version,m.state,',
      tail: `p.id IS NULL OR m.state NOT IN ('completed','cancelled') OR NOT EXISTS
 (SELECT 1 FROM staff_project_milestone_events event WHERE event.milestone_id=m.id AND event.milestone_version=m.version AND event.action=m.state)
 FROM staff_project_milestones m CROSS JOIN scope LEFT JOIN projects p ON p.id=m.project_id WHERE scope.is_base AND m.company_id=scope.company_id`,
    },
    {
      kind: 'task',
      head: `t.id,t.version,t.state,p.id IS NULL OR t.state NOT IN ('completed','cancelled')`,
      tail: `OR (t.state='completed' AND NOT EXISTS(SELECT 1 FROM staff_project_task_packages binding JOIN staff_project_milestones m ON m.id=binding.milestone_id
 WHERE binding.task_id=t.id AND binding.company_id=t.company_id AND binding.project_id=t.project_id AND m.project_id=t.project_id AND m.company_id=t.company_id))
 FROM (${s.tasks}) t CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id WHERE scope.is_base AND t.company_id=scope.company_id AND t.project_id IS NOT NULL`,
    },
    {
      kind: 'routine-template',
      head: `t.id,t.version,CASE WHEN t.published=1 THEN 'published' ELSE 'draft' END,p.id IS NULL OR t.published=1`,
      tail: `FROM (${s.templates}) t CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id WHERE scope.is_base AND t.company_id=scope.company_id AND t.project_id IS NOT NULL`,
    },
    {
      kind: 'routine-schedule',
      head: `s.id,s.version,s.state,p.id IS NULL OR s.state='active' OR s.company_id IS DISTINCT FROM t.company_id`,
      tail: `FROM (${s.schedules}) s JOIN (${s.templates}) t ON t.id=s.template_id CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id
 WHERE scope.is_base AND s.company_id=scope.company_id AND t.project_id IS NOT NULL`,
    },
  ];
  return arms.map(a => `UNION ALL SELECT '${a.kind}',${a.head}\n ${a.tail}`).join('\n ');
};
