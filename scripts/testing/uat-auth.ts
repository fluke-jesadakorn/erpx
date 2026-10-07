import type { BrowserCase } from "./browser-results";
import type { Suite } from "./catalogue";

export const uatProfileIds = ["owner", "owner-branch", "owner-department", "employee", "finance", "people", "purchasing", "sales", "support", "branch", "department", "hotel-owner", "pms-holding", "pos-holding", "pms", "pos"] as const;
export type UatProfileId = typeof uatProfileIds[number];
export const uatAuthAnnotationType = "erpx-uat-auth";

function validatedProfiles(value: unknown): UatProfileId[] {
  if (!Array.isArray(value) || value.some(id => typeof id !== "string" || !uatProfileIds.includes(id as UatProfileId))) throw new Error("INVALID_UAT_AUTH_PROFILES");
  return [...new Set(value)] as UatProfileId[];
}

/** Explicitly empty means an anonymous case; missing retains standalone full preparation. */
export function parseUatAuthProfiles(value: string | undefined): UatProfileId[] | undefined {
  if (value === undefined) return undefined;
  if (!value.trim()) return [];
  return validatedProfiles(value.split(",").map(id => id.trim()));
}

/** Suite annotations are inherited by Playwright without changing scenario identities. */
export function uatAuthProfiles(...profiles: UatProfileId[]) {
  return { annotation: { type: uatAuthAnnotationType, description: JSON.stringify(validatedProfiles(profiles)) } };
}

export function requiredUatAuthProfiles(suite: Suite, wanted: BrowserCase): UatProfileId[] {
  if (suite.lane !== "uat") throw new Error("UAT_SUITE_REQUIRED");
  if (suite.uatAuth?.mode === "self-authenticated") return [];
  if (suite.uatAuth?.mode !== "case-annotations") throw new Error(`UAT_AUTH_MAPPING_REQUIRED:${suite.id}`);
  const annotations = wanted.annotations?.filter(annotation => annotation.type === uatAuthAnnotationType) ?? [];
  if (annotations.length !== 1 || annotations[0].description === undefined) throw new Error(`UAT_CASE_AUTH_MAPPING_REQUIRED:${wanted.title}`);
  let value: unknown;
  try { value = JSON.parse(annotations[0].description); }
  catch { throw new Error(`INVALID_UAT_CASE_AUTH_MAPPING:${wanted.title}`); }
  return validatedProfiles(value);
}
