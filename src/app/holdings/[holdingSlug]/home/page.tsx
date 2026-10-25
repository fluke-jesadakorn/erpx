import { workspaceNavigation } from "@/server/modules/work_projection/public/navigation-snapshot";
import { SocialPage } from "@/components/beta/social-page";
export default async function Home({params,searchParams}:PageProps<"/holdings/[holdingSlug]/home">) {
  const { context: actor, initialWork } = await workspaceNavigation(params);
  const query = await searchParams;
  return <SocialPage actor={actor} initialWork={initialWork} tag={typeof query.tag === "string" ? query.tag.normalize("NFC").toLowerCase() : undefined}/>;
}
