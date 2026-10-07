import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { SocialPage } from "@/components/beta/social-page";
export default async function Home({params,searchParams}:PageProps<"/holdings/[holdingSlug]/home">) {
  const query = await searchParams;
  return <SocialPage actor={await requireWorkContext((await params).holdingSlug)} tag={typeof query.tag === "string" ? query.tag.normalize("NFC").toLowerCase() : undefined}/>;
}
