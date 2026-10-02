import { z } from "zod";
import type { OrganizationAccessData } from "./organization-access";
import { reportingIssues } from "./organization";

const id = z.string().min(1).max(180);
export const draftItemSchema = z.object({
  kind: z.enum(["person", "position", "department"]), id,
  parked: z.boolean(), positionId: id.optional(), departmentId: id.optional(), levelId: id.optional(), branchId: id.optional(),
  managerPersonId: id.nullable().optional(), reportsToPositionId: id.nullable().optional(),
  acceptRecommendedLeave: z.boolean().optional(),
  supersedeLegacyCase: z.object({ id, expectedVersion: z.number().int().positive() }).optional(),
}).strict();
export const draftDefinitionSchema = z.object({
  items: z.array(draftItemSchema).max(20), reason: z.string().max(1000),
  effectiveAt: z.number().int().positive(), timezone: z.string().max(80).refine(value => { try { new Intl.DateTimeFormat("en", { timeZone: value }); return true; } catch { return false; } }),
  managers: z.record(z.string(), id.nullable()).default({}), leave: z.record(z.string(), z.boolean()).default({}),
}).strict().superRefine((value, ctx) => {
  const keys = value.items.map(item => `${item.kind}:${item.id}`);
  if (new Set(keys).size !== keys.length) ctx.addIssue({ code: "custom", message: "Each item can appear only once." });
  for (const item of value.items) {
    if (item.kind !== "person" && (item.positionId || item.managerPersonId !== undefined || item.supersedeLegacyCase)) ctx.addIssue({ code: "custom", message: "Invalid person fields." });
    if (item.kind !== "position" && (item.departmentId || item.levelId || item.reportsToPositionId !== undefined)) ctx.addIssue({ code: "custom", message: "Invalid position fields." });
    if (item.kind !== "department" && item.branchId) ctx.addIssue({ code: "custom", message: "Invalid department fields." });
  }
});
export type DraftItem = z.infer<typeof draftItemSchema>;
export type DraftDefinition = z.infer<typeof draftDefinitionSchema>;
export type DraftState = "draft" | "submitted" | "discarded";
export type OrganizationDraft = { id: string; version: number; state: DraftState; definition: DraftDefinition; updated_at: number };
export type DraftRequest = { id: string; item_keys: string[]; state: "scheduled" | "waiting_approval" | "waiting_dependency" | "applied" | "needs_review"; last_error: string | null; change_id: string | null };
export type DraftDiff = { key: string; kind: DraftItem["kind"]; id: string; label: string; before: string; after: string; affected: string[]; blockers: string[] };
export const draftKey = (item: Pick<DraftItem, "kind" | "id">) => `${item.kind}:${item.id}`;
export function emptyDraft(): DraftDefinition { return { items: [], reason: "", effectiveAt: Date.now() + 3600000, timezone: "Asia/Bangkok", managers: {}, leave: {} }; }

/** The same projected graph drives the editor and the authoritative server review. */
export function projectOrganization(data: OrganizationAccessData, definition: DraftDefinition) {
  const nodes = data.nodes.map(node => {
    const move = definition.items.find(item => item.kind === "department" && item.id === node.id && !item.parked);
    return move?.branchId ? { ...node, branch_id: move.branchId } : { ...node };
  });
  const positions = data.positions.map(position => {
    const move = definition.items.find(item => item.kind === "position" && item.id === position.id && !item.parked);
    const department = nodes.find(node => node.id === (move?.departmentId ?? position.department_id));
    const level = data.levels.find(level => level.id === (move?.levelId ?? position.level_id));
    return { ...position, department_id: department?.id ?? position.department_id, branch_id: department?.branch_id ?? position.branch_id,
      level_id: level?.id ?? position.level_id, level_number: level?.level_number ?? position.level_number,
      reports_to_position_id: move?.reportsToPositionId === undefined ? position.reports_to_position_id : move.reportsToPositionId };
  });
  const people = data.people.map(person => {
    const move = definition.items.find(item => item.kind === "person" && item.id === person.engagement_id && !item.parked);
    const position = positions.find(position => position.id === (move?.positionId ?? person.position_id));
    const positionMove = definition.items.find(item => item.kind === "position" && item.id === person.position_id && !item.parked);
    const changedPosition = !!move?.positionId || !!positionMove;
    const department = nodes.find(node => node.id === person.department_id);
    return { ...person, ...(changedPosition && position ? { position_id: position.id, branch_id: position.branch_id ?? person.branch_id, department_id: position.department_id ?? person.department_id, ...(move?.positionId || positionMove?.levelId ? { level_id: position.level_id, level_number: position.level_number } : {}), job_title: move?.positionId ? position.title : person.job_title } : { branch_id: department?.branch_id ?? person.branch_id }),
      manager_person_id: Object.hasOwn(definition.managers, person.engagement_id) ? definition.managers[person.engagement_id] : move?.managerPersonId === undefined ? person.manager_person_id : move.managerPersonId };
  });
  return { ...data, nodes, positions, people };
}
export const personPlacement = (p: OrganizationAccessData["people"][number]) => ({ positionId: p.position_id, branchId: p.branch_id, departmentId: p.department_id, levelId: p.level_id, managerPersonId: p.manager_person_id });
export const positionPlacement = (p: OrganizationAccessData["positions"][number]) => ({ branchId: p.branch_id, departmentId: p.department_id, levelId: p.level_id, reportsToPositionId: p.reports_to_position_id });
export const changedPeople = (before: OrganizationAccessData, after: OrganizationAccessData) => after.people.filter(p => JSON.stringify(personPlacement(p)) !== JSON.stringify(personPlacement(before.people.find(old => old.engagement_id === p.engagement_id)!)));
export function organizationPath(data: OrganizationAccessData, kind: string, id: string) {
  const person = data.people.find(p => kind === "person" && p.engagement_id === id);
  const position = data.positions.find(p => p.id === (person?.position_id ?? (kind === "position" ? id : "")));
  const department = data.nodes.find(n => n.id === (position?.department_id ?? person?.department_id ?? (kind === "department" ? id : "")));
  const branch = data.nodes.find(n => n.id === (department?.branch_id ?? position?.branch_id ?? person?.branch_id));
  const level = data.levels.find(l => l.id === position?.level_id);
  return [branch?.display_name, department?.display_name, position?.title, level?.name].filter(Boolean).join(" › ") || "Unassigned";
}
export function organizationDraftDiff(data: OrganizationAccessData, definition: DraftDefinition): DraftDiff[] {
  const projected = projectOrganization(data, definition);
  return definition.items.map(item => {
    const person = data.people.find(p => p.engagement_id === item.id);
    const position = data.positions.find(p => p.id === item.id);
    const department = data.nodes.find(n => n.id === item.id && n.kind === "department");
    const blockers: string[] = [];
    if (item.parked) blockers.push("Choose a destination for this parked item.");
    if (item.kind === "person" && (!person || person.status === "ended" || person.person_status !== "active")) blockers.push("Employee is no longer current.");
    if (item.kind === "position" && !position?.active) blockers.push("Position is unavailable.");
    if (item.kind === "department" && department?.status !== "active") blockers.push("Department is unavailable.");
    if (!item.parked) {
      if (item.kind === "person" && !data.positions.some(p => p.id === item.positionId && p.active)) blockers.push("Choose an active position in this Company.");
      if (item.kind === "position" && (!item.departmentId && !item.levelId && item.reportsToPositionId === undefined)) blockers.push("Choose a department or Level.");
      if (item.departmentId && !data.nodes.some(n => n.id === item.departmentId && n.kind === "department" && n.status === "active" && data.nodes.some(b => b.id === n.branch_id && b.kind === "branch" && b.status === "active"))) blockers.push("Choose an active department in an active branch.");
      if (item.levelId && !data.levels.some(l => l.id === item.levelId && l.level_number >= 1 && l.level_number <= 6)) blockers.push("Choose one of the six Levels.");
      if (item.kind === "department" && !data.nodes.some(n => n.id === item.branchId && n.kind === "branch" && n.status === "active")) blockers.push("Choose an active branch in this Company.");
    }
    const affected = projected.people.filter(p => item.kind === "person" ? p.engagement_id === item.id : item.kind === "position" ? p.position_id === item.id || data.people.find(old => old.engagement_id === p.engagement_id)?.position_id === item.id : p.department_id === item.id || data.people.find(old => old.engagement_id === p.engagement_id)?.department_id === item.id).map(p => p.engagement_id);
    const before = organizationPath(data, item.kind, item.id), after = item.parked ? "Move tray" : organizationPath(projected, item.kind, item.id);
    if (!item.parked && (item.kind === "person" ? item.positionId === person?.position_id && item.managerPersonId === undefined : item.kind === "position" ? (!item.departmentId || item.departmentId === position?.department_id) && (!item.levelId || item.levelId === position?.level_id) && item.reportsToPositionId === undefined : item.branchId === department?.branch_id)) blockers.push("Choose a different destination.");
    return { key: draftKey(item), kind: item.kind, id: item.id, label: item.kind === "person" ? person?.legal_name ?? "Unavailable employee" : item.kind === "position" ? position?.title ?? "Unavailable position" : department?.display_name ?? "Unavailable department", before, after, affected, blockers };
  });
}
export function draftGraphBlockers(data: OrganizationAccessData, definition: DraftDefinition) {
  const next = projectOrganization(data, definition), changed = changedPeople(data, next), blockers: string[] = [];
  const date = new Intl.DateTimeFormat("en-CA", { timeZone: definition.timezone, year: "numeric", month: "2-digit", day: "2-digit" }).format(definition.effectiveAt);
  for (const person of next.people) {
    const before = data.people.find(p => p.engagement_id === person.engagement_id)!;
    const prior = reportingIssues(before, data.people, data.positions, date);
    for (const issue of reportingIssues(person, next.people, next.positions, date)) if (changed.includes(person) || !prior.includes(issue)) blockers.push(`${person.legal_name}: ${issue}`);
  }
  for (const position of next.positions) {
    const moved = definition.items.some(i => i.kind === "position" && i.id === position.id && !i.parked);
    const parentMoved = definition.items.some(i => i.kind === "position" && i.id === position.reports_to_position_id && !i.parked);
    if (!moved && !parentMoved) continue;
    const parent = next.positions.find(p => p.id === position.reports_to_position_id);
    if (position.is_ceo ? position.level_number !== 1 || !!parent : !parent || parent.level_number >= position.level_number) blockers.push(`${position.title}: choose a valid reporting position at a more senior Level.`);
    const seen = new Set([position.id]); let current = parent;
    while (current) { if (seen.has(current.id)) { blockers.push("Circular position reporting relationship."); break; } seen.add(current.id); current = next.positions.find(p => p.id === current!.reports_to_position_id); }
  }
  return [...new Set(blockers)];
}
