import type { OrganizationOverview, OrganizationPerson } from "./organization";

export type ChartEntry = {
  person: OrganizationPerson;
  title: string;
  placement: string;
  isCeo: boolean;
  issues: string[];
  boundary: string | null;
  searchText: string;
};
export type OrganizationGraph = {
  entries: Map<string, ChartEntry>;
  parents: Map<string, string>;
  children: Map<string, string[]>;
  roots: string[];
  attention: string[];
};

/** Build only from the authorized overview. Never infer reporting edges from levels. */
export function buildOrganizationGraph(data: Pick<OrganizationOverview, "people" | "positions" | "nodes">): OrganizationGraph {
  const entries = new Map<string, ChartEntry>();
  const people = new Map<string, OrganizationPerson[]>();
  const positions = new Map(data.positions.map(p => [p.id, p]));
  const nodes = new Map(data.nodes.map(n => [n.id, n.display_name]));
  const parents = new Map<string, string>();
  for (const person of data.people) {
    const key = person.company_id + ":" + person.person_id;
    people.set(key, [...(people.get(key) ?? []), person]);
    const position = positions.get(person.position_id ?? "");
    const title = position?.title ?? person.job_title;
    const placement = [nodes.get(person.branch_id ?? ""), nodes.get(person.department_id ?? "")].filter(Boolean).join(" / ");
    entries.set(person.engagement_id, {
      person, title, placement, isCeo: Boolean(position?.is_ceo),
      issues: [...person.issues], boundary: null,
      searchText: [person.legal_name, title, person.job_title, person.level_name, person.level_number ? "Level " + person.level_number : "Level not mapped", placement].join(" ").toLocaleLowerCase(),
    });
  }
  for (const [id, entry] of entries) {
    const person = entry.person;
    if (!person.manager_person_id) continue;
    const matches = people.get(person.company_id + ":" + person.manager_person_id) ?? [];
    if (matches.length === 1) {
      parents.set(id, matches[0].engagement_id);
      entry.searchText += " " + matches[0].legal_name.toLocaleLowerCase();
    } else if (matches.length > 1) {
      entry.issues.push("Multiple visible profiles match this manager. Review the reporting relationship.");
    } else {
      // Absence in a scoped response does not prove that a manager is missing in the company.
      entry.boundary = "Manager is not available in this view.";
    }
  }
  // Iterative cycle detection also handles long/invalid chains without overflowing the stack.
  const done = new Set<string>();
  const cycles = new Set<string>();
  for (const start of entries.keys()) {
    const path: string[] = [], offsets = new Map<string, number>();
    let current: string | undefined = start;
    while (current && !done.has(current) && !offsets.has(current)) {
      offsets.set(current, path.length);
      path.push(current);
      current = parents.get(current);
    }
    if (current && offsets.has(current)) {
      for (const id of path.slice(offsets.get(current))) cycles.add(id);
    }
    path.forEach(id => done.add(id));
  }
  for (const id of cycles) {
    parents.delete(id);
    entries.get(id)!.issues.push("Circular reporting relationship. Open the profile to correct it.");
  }
  const children = new Map<string, string[]>();
  for (const id of entries.keys()) children.set(id, []);
  for (const [id, parent] of parents) children.get(parent)!.push(id);
  const compare = (a: string, b: string) => {
    const left = entries.get(a)!, right = entries.get(b)!;
    return Number(right.isCeo) - Number(left.isCeo)
      || left.person.legal_name.localeCompare(right.person.legal_name)
      || a.localeCompare(b);
  };
  for (const ids of children.values()) ids.sort(compare);
  return {
    entries, parents, children,
    roots: [...entries.keys()].filter(id => !parents.has(id)).sort(compare),
    attention: [...entries.keys()].filter(id => entries.get(id)!.issues.length > 0).sort(compare),
  };
}

export function ancestorPath(graph: OrganizationGraph, id: string): string[] {
  const path: string[] = [], seen = new Set<string>();
  let current: string | undefined = graph.parents.get(id);
  while (current && !seen.has(current)) {
    seen.add(current);
    path.unshift(current);
    current = graph.parents.get(current);
  }
  return path;
}

export function searchOrganization(graph: OrganizationGraph, query: string): string[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return [...graph.entries.keys()].filter(id => terms.every(term => graph.entries.get(id)!.searchText.includes(term)));
}

export const teamLevelTitles = ["CEO", "Executives / Directors", "Branch / Department Managers", "Supervisors", "Senior Staff", "Staff", "Level not assigned"];
export function assignedLevel(value: number | null): number {
  return value !== null && Number.isInteger(value) && value >= 1 && value <= 6 ? value : 7;
}
export function departmentHue(slug: string): number {
  const hues: Record<string, number> = { finance: 174, accounting: 150, operations: 214, people: 275, sales: 340, purchasing: 43, warehouse: 25, it: 240, "customer-service": 190 };
  const normalized = slug.toLowerCase().trim();
  return hues[normalized] ?? [...normalized].reduce((hash, char) => (hash * 31 + char.charCodeAt(0)) % 360, 0);
}
export function teamIds(graph: OrganizationGraph, focusId: string | null): Set<string> {
  if (!focusId) return new Set(graph.entries.keys());
  const ids = new Set<string>(), queue = [focusId];
  for (let i = 0; i < queue.length; i++) {
    const id = queue[i];
    if (ids.has(id) || !graph.entries.has(id)) continue;
    ids.add(id); queue.push(...(graph.children.get(id) ?? []));
  }
  return ids;
}
export function levelGroups(graph: OrganizationGraph, data: Pick<OrganizationOverview, "nodes">, ids: ReadonlySet<string>) {
  const nodes = new Map(data.nodes.map(n => [n.id, n]));
  return teamLevelTitles.map((title, index) => {
    const groups = new Map<string, { key: string; department: string; branch: string; hue: number; ids: string[] }>();
    for (const id of ids) {
      const person = graph.entries.get(id)!.person;
      if (assignedLevel(person.level_number) !== index + 1) continue;
      const key = JSON.stringify([person.company_id, person.branch_id, person.department_id]);
      if (!groups.has(key)) groups.set(key, { key, department: nodes.get(person.department_id ?? "")?.display_name ?? "Department not assigned / not shown", branch: nodes.get(person.branch_id ?? "")?.display_name ?? "Company-wide / branch not shown", hue: departmentHue(nodes.get(person.department_id ?? "")?.slug ?? person.department_id ?? "unassigned"), ids: [] });
      groups.get(key)!.ids.push(id);
    }
    for (const group of groups.values()) group.ids.sort((a, b) => graph.entries.get(a)!.person.legal_name.localeCompare(graph.entries.get(b)!.person.legal_name) || a.localeCompare(b));
    return { level: index + 1, title, groups: [...groups.values()].sort((a, b) => a.department.localeCompare(b.department) || a.branch.localeCompare(b.branch) || a.key.localeCompare(b.key)) };
  });
}
