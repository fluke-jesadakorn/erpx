import { iamContext } from "@/server/iam-context";
import {
  peopleAccessData,
  accessMetadata,
  accessEntries,
} from "@/server/people-access";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { workspaceBase, workspaceApiHref } from "@/lib/workspace";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { UserManagement } from "@/components/people/user-management";
import { readManagement } from "@/server/user-management";
import { PageHeader } from "@/components/ui/primitives";
export default async function PeopleAccessPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  const resolved = await (async () => {
    try {
      const { db, actor } = await iamContext(holdingSlug),
        context = await requireWorkContext(holdingSlug),
        data = await peopleAccessData(db, actor);
      return {
        actor,
        scope: publicScope(context),
        meta: accessMetadata(data),
        entries: accessEntries(data, new URLSearchParams()),
        management: await readManagement(db, actor),
      };
    } catch (error) {
      return pageError(error, holdingSlug);
    }
  })();
  const base = workspaceBase(resolved.scope);
  return (
    <main className="content" data-source="live">
      <PageHeader
        eyebrow="Your team"
        title="Directory & access"
        description="Find people, understand their work context, and manage ERPX access."
      />
      <PeopleNavigation
        base={base}
        actor={resolved.actor}
        scopeKind={resolved.scope.kind}
        active="access"
      />
      <UserManagement
        initial={resolved.management}
        roles={resolved.meta.roles}
        base={base}
        api={workspaceApiHref(resolved.scope, "people/access")}
        accounts={{ meta: resolved.meta, initialEntries: resolved.entries }}
      />
    </main>
  );
}
