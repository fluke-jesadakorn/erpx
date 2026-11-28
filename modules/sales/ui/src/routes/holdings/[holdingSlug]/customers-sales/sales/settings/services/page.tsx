import {SalesServiceProducts} from '@erpx/sales-ui/components/service-products';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {salesServiceProducts} from '@erpx/commerce-server/public/sales-service-products';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),initial=await salesServiceProducts(actor).catch(error=>pageError(error,holdingSlug));return <SalesServiceProducts base={workspaceBase(publicScope(actor))} initial={initial}/>;}
