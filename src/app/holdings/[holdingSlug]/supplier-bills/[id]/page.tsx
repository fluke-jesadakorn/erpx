import { RecordLink as Link } from "@/components/records/record-link";
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {supplierBillDetail} from '@/server/supplier-bill-matching';
import {SupplierBillConsole} from '@/components/modules/supplier-bill-console';
import {RecordDocumentPanel} from '@/components/attachments/record-document-panel';
import {hasPermission} from '@/lib/permissions';
import { PageContent } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await supplierBillDetail(a,id).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={`${base}/work`}>Back to Work</Link><Link href={`${base}/supplier-bills`}>All supplier invoice reviews</Link><h1>Supplier invoice matching</h1><SupplierBillConsole key={String(data.bill.version)} base={base} data={data}/><RecordDocumentPanel appearance="standard" base={base} targetType="supplier_bill_match" targetId={String(data.bill.id)} expectedTargetVersion={Number(data.bill.version)} editable={hasPermission(a,'finance.write')} title="Supplier invoice and matching documents" purposes={[{key:'invoice_document',label:'Supplier invoice',description:'Attach the supplier invoice for this exact matching revision.',visibility:'Visible only to authorized Procurement and Finance colleagues in this Company scope.'},{key:'tax_document',label:'Supplier tax document',description:'Attach the tax invoice or related statutory supplier document for this matching revision.',visibility:'Visible only to authorized Procurement and Finance colleagues in this Company scope.'},{key:'bill_match_support',label:'Matching support',description:'Attach price, quantity, or correction support reviewed for this bill match.',visibility:'Visible only to authorized Procurement and Finance colleagues in this Company scope.'}]}/></PageContent>;}
