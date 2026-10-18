import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
export const adminSections = [
  ["brand", "Brand & identity"],
  ["people", "People & access"],
  ["organization", "Organization"],
  ["approvals", "Approvals & routines"],
  ["usage", "Usage & billing"],
  ["ai", "AI providers"],
  ["feedback", "Feedback & reports"],
] as const;
export type AdminSection = (typeof adminSections)[number][0];
export type AdminAccess = {
  sections: AdminSection[];
  managePeople: boolean;
  manageOrganization: boolean;
  owner: boolean;
  manageAiProviders: boolean;
};
export function approvalAdminHref(base: string, kind: string, id: string) {
  return canonicalUiHref(`${base}/settings?${new URLSearchParams({ section: "approvals", kind, record: id })}`);
}
