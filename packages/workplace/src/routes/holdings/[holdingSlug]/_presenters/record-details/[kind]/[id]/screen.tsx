import { notFound } from "next/navigation";
import { isRecordKind } from "@erpx/feature-records/lib/record-drawer";
import { RecordSupplement } from "@erpx/feature-records/components/record-supplement";
export const dynamic="force-dynamic";
export default async function RecordDetailsPage({params,searchParams}:{params:Promise<{holdingSlug:string;kind:string;id:string}>;searchParams:Promise<Record<string,string>>}) {
 const {holdingSlug,kind,id}=await params;
 if(!isRecordKind(kind))notFound();
 return <RecordSupplement holdingSlug={holdingSlug} target={{kind,id}} args={await searchParams}/>;
}
