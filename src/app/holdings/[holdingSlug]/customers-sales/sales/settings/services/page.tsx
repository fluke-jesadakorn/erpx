import {SalesServiceProducts} from '@/components/sales/service-products';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {salesServiceProducts} from '@/server/modules/commerce/public/sales-service-products';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),initial=await salesServiceProducts(actor).catch(error=>pageError(error,holdingSlug));return <SalesServiceProducts base={workspaceBase(publicScope(actor))} initial={initial}/>;}
