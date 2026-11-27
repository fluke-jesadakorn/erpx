import { workspaceNavigation } from "@erpx/server-work_projection/public/navigation-snapshot";
import { SocialPage } from "@erpx/feature-collaboration/components/social-page";
import { oasisDemoStagingEnabled } from "@erpx/contracts/holding-hosts";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { OasisFeed } from "./oasis-feed";
export default async function Home({params,searchParams}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }) {
  if (oasisDemoStagingEnabled(process.env)) {
    const actor = await requireWorkContext((await params).holdingSlug);
    return <OasisFeed api={`/api/v1${workspaceBase(publicScope(actor))}`} />;
  }
  const { context: actor, initialWork } = await workspaceNavigation(params);
  const query = await searchParams;
  return <SocialPage actor={actor} initialWork={initialWork} tag={typeof query.tag === "string" ? query.tag.normalize("NFC").toLowerCase() : undefined}/>;
}
