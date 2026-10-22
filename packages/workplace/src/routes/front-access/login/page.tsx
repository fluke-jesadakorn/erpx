import { headers } from "next/headers";
import { frontAppForHost, safeFrontReturnPath } from "@erpx/feature-shared/lib/front-host";
import { StandardFrontSignIn } from "@erpx/front/components/standard-front-access";
import { currentSession } from "@erpx/server-identity_access/public/authorization";
import { workspaceChoices } from "@erpx/server-organization/public/workspace-choices";
import { appsEntryUrl } from "@erpx/feature-runtime/lib/application-origins";

export default async function FrontLogin({ searchParams }: { searchParams: Promise<{ next?: string; reason?: string }> }) {
  const app = frontAppForHost((await headers()).get("host"));
  const query = await searchParams;
  const next = safeFrontReturnPath(query.next);
  const session = await currentSession().catch(() => null);
  const hasErpRole = session ? (await workspaceChoices().catch(() => [])).length > 0 : false;
  const erpHref = hasErpRole ? appsEntryUrl() : null;
  const isPos = app === "pos";
  return <StandardFrontSignIn isPos={isPos} next={next} reason={query.reason} erpHref={erpHref} />;
}
