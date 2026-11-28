import type { OrganizationAccessData } from "@erpx/organization-ui/lib/organization-access";
import type { Clock } from "@erpx/platform-shared-server/index";
import { assertOrganizationAccess, projectOrganizationAccess, type OrganizationAccessActor, type OrganizationAccessSnapshot } from "@erpx/organization-server/domain/access";

export interface OrganizationAccessRepository {
  roster(): Promise<Pick<OrganizationAccessSnapshot, "people" | "positions">>;
  nodes(): Promise<OrganizationAccessData["nodes"]>;
  levels(): Promise<OrganizationAccessData["levels"]>;
}
export function createOrganizationAccessApplication(actor: OrganizationAccessActor, repository: OrganizationAccessRepository, clock: Clock) {
  return {
    async read(): Promise<OrganizationAccessData> {
      assertOrganizationAccess(actor);
      const [{ people, positions }, nodes, levels] = await Promise.all([repository.roster(), repository.nodes(), repository.levels()]);
      return projectOrganizationAccess(actor, { people, positions, nodes, levels }, new Date(clock.now()).toISOString().slice(0, 10));
    },
  };
}
