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

export function visibleChartIds(graph: OrganizationGraph, expanded: ReadonlySet<string>): string[] {
  const result: string[] = [], queue = [...graph.roots];
  for (let index = 0; index < queue.length; index++) {
    const id = queue[index];
    result.push(id);
    if (expanded.has(id)) queue.push(...(graph.children.get(id) ?? []));
  }
  return result;
}

export function searchOrganization(graph: OrganizationGraph, query: string): string[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return [...graph.entries.keys()].filter(id => terms.every(term => graph.entries.get(id)!.searchText.includes(term)));
}
