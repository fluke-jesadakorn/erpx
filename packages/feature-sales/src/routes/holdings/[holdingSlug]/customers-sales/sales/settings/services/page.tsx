import {SalesServiceProducts} from '@erpx/feature-sales/components/service-products';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {salesServiceProducts} from '@erpx/server-commerce/public/sales-service-products';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),initial=await salesServiceProducts(actor).catch(error=>pageError(error,holdingSlug));return <SalesServiceProducts base={workspaceBase(publicScope(actor))} initial={initial}/>;}
