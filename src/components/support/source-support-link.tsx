import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordLink as Link } from "@/components/records/record-link";
import { type UiAppearance } from "@/components/ui/foundation";
export function SourceSupportLink({base,kind,id,version,label="Report an issue with this record"}:({base:string;kind:"pos_sale"|"pms_record"|"asset";id:string;version:number;label?:string}) & { appearance?: UiAppearance }){
 const query=new URLSearchParams({sourceKind:kind,sourceId:id,sourceVersion:String(version),new:"1"});
 return <Link href={canonicalUiHref(`${base}/support?${query}`)}>{label}</Link>;
}
