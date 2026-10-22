import { RecordLink } from "@erpx/feature-records/components/record-link";
import { recordCanonicalHref, type RecordKind } from "@erpx/feature-records/lib/record-drawer";
import { Card } from "@erpx/ui/components/foundation";
export function RecordCatalog({base,kind,title,rows,label}:{base:string;kind:RecordKind;title:string;rows:Record<string,unknown>[];label:string}) {
 return <Card as="section" appearance="standard" className="ui-panel"><h2>{title}</h2>{!rows.length&&<p>No records in this work scope.</p>}{rows.map(row=><p key={String(row.id)}><RecordLink record={{kind,id:String(row.id)}} href={recordCanonicalHref(base,{kind,id:String(row.id)})}>{String(row[label]??row.id)}</RecordLink> · {String(row.status??row.state??'')}</p>)}</Card>;
}
