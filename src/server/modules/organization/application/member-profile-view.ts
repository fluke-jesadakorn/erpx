import type { WorkContext } from "../../identity_access/public/authorization";
import { memberProfileAccessControls, type MemberLevelHistory, type MemberProfileAccess, type MemberProfileNodes, type MemberProfileOrganization, type MemberProfilePerson, type MemberProfileReference, type MemberProfileSettings } from "../domain/member-profile-view";

export interface MemberProfileViewPorts {
  profile(actor: WorkContext, personId: string | null): Promise<MemberProfilePerson | null>;
  settings(actor: WorkContext): Promise<MemberProfileSettings>;
  canManagePeople(actor: WorkContext): boolean;
  access(actor: WorkContext, personId: string): Promise<MemberProfileAccess>;
  levelHistory(engagementId: string | number): Promise<MemberLevelHistory[]>;
  nodes(actor: WorkContext): Promise<MemberProfileNodes>;
  organization(actor: WorkContext): Promise<MemberProfileOrganization>;
  hasPermission(actor: WorkContext, permission: "iam.manage" | "iam.roles.manage"): boolean;
}

export function createMemberProfileViewApplication(ports: MemberProfileViewPorts) {
  return {
    async read(actor: WorkContext, member: MemberProfileReference) {
      const person = await ports.profile(actor, member.personId);
      const options = await ports.settings(actor);
      const manage = ports.canManagePeople(actor);
      const access = person ? await ports.access(actor, String(person.id)) : null;
      const history = person?.engagement_id ? await ports.levelHistory(person.engagement_id) : [];
      const nodes = manage ? await ports.nodes(actor) : [];
      const organization = manage ? await ports.organization(actor) : undefined;
      const controls = access && person ? memberProfileAccessControls(actor, member.id, {
        manage: ports.hasPermission(actor, "iam.manage"),
        revokeCompany: ports.hasPermission(actor, "iam.roles.manage"),
      }) : { canManageAccess: false, canRevokeCompany: false };
      return { person, options, manage, access, history, nodes, organization, ...controls };
    },
  };
}
