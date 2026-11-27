import { SocialPage } from "@erpx/feature-collaboration/components/social-page";
import { oasisDemoStagingEnabled } from "@erpx/contracts/holding-hosts";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { OasisFeed } from "./oasis-feed";
export default async function Home({params,searchParams}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }) {
  const actor = await requireWorkContext((await params).holdingSlug);
  if (oasisDemoStagingEnabled(process.env)) {
    return <OasisFeed api={`/api/v1${workspaceBase(publicScope(actor))}`} />;
  }
  const query = await searchParams;
  return <SocialPage actor={actor} tag={typeof query.tag === "string" ? query.tag.normalize("NFC").toLowerCase() : undefined}/>;
}
