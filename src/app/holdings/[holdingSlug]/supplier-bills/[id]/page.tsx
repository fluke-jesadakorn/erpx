import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {supplierBillDetail} from '@/server/supplier-bill-matching';
import {SupplierBillConsole} from '@/components/modules/supplier-bill-console';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await supplierBillDetail(a,id).catch(e=>pageError(e,holdingSlug));return <div className="content-stack"><Link href={`${base}/work`}>Back to Work</Link><Link href={`${base}/supplier-bills`}>All supplier invoice reviews</Link><h1>Supplier invoice matching</h1><SupplierBillConsole key={String(data.bill.version)} base={base} data={data}/></div>;}
