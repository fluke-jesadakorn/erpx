import { requireWorkContext } from "@/server/authorization";
import { SocialPage } from "@/components/beta/social-page";
export default async function Home({params}:{params:Promise<{holdingSlug:string}>}) { return <SocialPage actor={await requireWorkContext((await params).holdingSlug)}/>; }
