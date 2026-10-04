import Link from "next/link";
import {requireWorkContext,publicScope} from "@/server/authorization";
import {workspaceBase} from "@/lib/workspace";
import {pageError} from "@/server/page-error";
import {brandIdentityWorkspace} from "@/server/brand-identity";
import {BrandIdentityConsole} from "@/components/modules/brand-identity-console";
import { PageContent } from "@/components/ui/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await brandIdentityWorkspace(a).catch(e=>pageError(e,holdingSlug));
 return <PageContent as="div" className="ui-page-content"><h1>Brand identity</h1><p>Review a draft, protect approved categories and publish a version for the company&apos;s customer-facing apps.</p><Link href={`${base}/settings?section=organization`}>Logos and approved media</Link><BrandIdentityConsole base={base} data={data}/></PageContent>;
}
