import Link from "@/components/workplace/workspace-link";
import {StatusBadge} from '@/components/ui/primitives';
import type {WorkItem} from "@/lib/work_projection/work-item";
import { Card } from "@/components/ui/foundation";

/** The owner service supplies the same projection used by Work. */
export function TransitNextAction({item,base}:{item:WorkItem;base:string}) {
 const done=item.queue==='completed';
 return <Card as="section" appearance="standard" className="ui-panel record-next-action" aria-label="Transport status, responsible team and next action">
  <div><span className="eyebrow">Transport status</span><StatusBadge appearance="standard" status={item.status}/></div>
  <div><span className="eyebrow">Responsible team</span><strong>{done?'Transport resolved':item.responsibleQueue}</strong></div>
  <div><span className="eyebrow">Next action</span><strong>{done?'Review recorded transport evidence':item.nextAction}</strong><p className="muted">{item.blocker??(done?'All transport quantities are resolved.':item.canAct?'Ready for you to continue in the transport record.':'Waiting for the responsible team.')}</p><Link href={`${base}${item.href}`}>Open transport record</Link></div>
 </Card>;
}
