/** Byte-identical SQL witnesses for the Projects read tail (r38).
 * Only closed gaps, aliases, id columns and binds vary. The relation, equality
 * order, projection columns and bind order stay byte-identical to the former
 * inline literals; no caller-supplied SQL or free column list enters either
 * helper. Callers keep their own locks, binds, result types and trailing SQL. */

/** Closed member-EXISTS witness. `staff_project_members` is fixed; only the
 * leading gap, member/project aliases, id column and user bind vary. */
export const memberExistsSql = (gap: string, member: string, project: string, id: string, user: string) =>
  `EXISTS(${gap}SELECT 1 FROM staff_project_members ${member} WHERE ${member}.project_id=${project}.${id} AND ${member}.user_id=${user})`;

/** The four fixed executive evidence projection heads. The union key is
 * closed, so no free column list can change a projection. */
export type EvidenceProjectionKind = 'milestone' | 'dependency' | 'actual-hours' | 'posted-cost';

const evidenceHeads: Record<EvidenceProjectionKind, { alias: string; columns: string }> = {
  milestone: {
    alias: 'm',
    columns: "m.title,m.due_at,m.owner_user_id,\n      COALESCE(h.legal_name,m.owner_user_id) owner_name,m.state,m.version",
  },
  dependency: {
    alias: 'm',
    columns: "m.title,m.due_at,m.state,m.version,\n      COALESCE(blockers.items,'[]'::jsonb)::text blockers_json",
  },
  'actual-hours': {
    alias: 'l',
    columns: 'l.work_date::text work_date,l.work_date::text sort_date,\n        l.minutes,l.user_id,l.state,l.version',
  },
  'posted-cost': {
    alias: 'a',
    columns: 'j.entry_date::text entry_date,j.entry_date::text sort_date,\n        j.id journal_id,j.journal_number,j.currency,',
  },
};

export const evidenceProjectionHead = (kind: EvidenceProjectionKind) => {
  const head = evidenceHeads[kind];
  if (!head) throw new Error(`Unknown Projects evidence projection: ${kind}`);
  return `SELECT ${head.alias}.id,${head.alias}.project_id,p.title project_title,${head.columns}`;
};
