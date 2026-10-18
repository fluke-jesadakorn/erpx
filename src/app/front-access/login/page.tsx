import { headers } from "next/headers";
import { frontAppForHost, safeFrontReturnPath } from "@/lib/shared/front-host";
import { StandardFrontSignIn } from "@/components/front/standard-front-access";
import { currentSession } from "@/server/modules/identity_access/public/authorization";
import { workspaceChoices } from "@/server/modules/organization/public/workspace-choices";
import { appsEntryUrl } from "@/lib/runtime/application-origins";

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
