import type { OrganizationNode, OrganizationPerson, OrganizationPosition } from "@/lib/organization/organization";

/** The organization projection for HR/IT collaboration; no compensation or access secrets. */
export type OrganizationAccessData = {
  positions: OrganizationPosition[];
  people: OrganizationPerson[];
  nodes: OrganizationNode[];
  levels: { id: string; name: string; level_number: number }[];
  canManage: boolean;
  canMovePeople: boolean;
  actorId: string;
};
