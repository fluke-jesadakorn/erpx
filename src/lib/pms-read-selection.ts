import { z } from "zod";
import { dateField, idField } from "./pms";

export const pmsReadSelectionSchema = z.discriminatedUnion("view", [
  z.object({ view: z.literal("dashboard") }),
  z.object({ view: z.literal("calendar"), from: dateField, to: dateField }).refine(value =>
    value.from < value.to && Date.parse(value.to) - Date.parse(value.from) <= 93 * 86_400_000,
    { message: "Choose a calendar range of at most 93 days." }),
  z.object({ view: z.literal("detail"), id: idField }),
]);
export type PmsReadSelection = z.infer<typeof pmsReadSelectionSchema>;

export function pmsReadSelectionFromSearch(search: URLSearchParams) {
  if (!search.has("view")) return undefined;
  return pmsReadSelectionSchema.parse({ view: search.get("view"), from: search.get("from"),
    to: search.get("to"), id: search.get("id") });
}
