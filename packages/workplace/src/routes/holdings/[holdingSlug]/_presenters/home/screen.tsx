import { workspaceNavigation } from "@erpx/server-work_projection/public/navigation-snapshot";
import { SocialPage } from "@erpx/feature-collaboration/components/social-page";
export default async function Home({params,searchParams}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }) {
  const { context: actor, initialWork } = await workspaceNavigation(params);
  const query = await searchParams;
  return <SocialPage actor={actor} initialWork={initialWork} tag={typeof query.tag === "string" ? query.tag.normalize("NFC").toLowerCase() : undefined}/>;
}
