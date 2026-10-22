export function preparedTaskFields(
  draft: Record<string, string>,
  people: readonly { user_id: string; legal_name: string }[],
) {
  const title = draft.title?.trim().slice(0, 200) || null;
  const description = draft.description?.trim().slice(0, 10000) || null;
  const date = draft.dueDate?.trim() ?? "";
  const parsed = /^\d{4}-\d{2}-\d{2}$/.test(date) ? new Date(`${date}T00:00:00.000Z`) : null;
  const dueLocal = parsed && !Number.isNaN(parsed.valueOf()) && parsed.toISOString().slice(0, 10) === date
    ? `${date}T09:00` : null;
  const requested = draft.assignee?.trim().toLocaleLowerCase();
  const matches = requested
    ? [...new Map(people.map(person => [person.user_id, person])).values()]
        .filter(person => person.user_id.toLocaleLowerCase() === requested || person.legal_name.toLocaleLowerCase() === requested)
    : [];
  return { title, description, dueLocal, assigneeId: matches.length === 1 ? matches[0].user_id : null };
}

export type PreparedPostPurpose = "update" | "plan" | "blocker" | "handover" | "announcement";
const purposes = new Set<PreparedPostPurpose>(["update", "plan", "blocker", "handover", "announcement"]);
export function preparedPostFields(draft: Record<string, string>) {
  return {
    body: (draft.body ?? "").slice(0, 3000),
    purpose: purposes.has(draft.purpose as PreparedPostPurpose)
      ? draft.purpose as PreparedPostPurpose : "update" as const,
  };
}
