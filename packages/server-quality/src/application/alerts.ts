import {BusinessRuleError} from '@erpx/server-shared/index';
import type {Clock} from '@erpx/server-shared/index';
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import {acknowledgeSchema,qualityTerminalStates,qualityAlertDate,type QualityAlert,type QualityAlertAcknowledgement,type QualityAlertResult} from '@erpx/server-quality/domain/alerts';
export interface QualityAlertRepository{
 alerts():Promise<QualityAlert[]>;replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;
 acknowledge(id:string,key:string,input:QualityAlertAcknowledgement,alert:QualityAlert,now:number):Promise<Record<string,unknown>>;
}
export interface QualityAlertSweepRepository{dueSoon(now:number,today:string,horizon:string):Promise<number>;overdue(now:number,today:string):Promise<number>;notify(now:number):Promise<void>;}
/** Each sweep stage is recoverable independently; unique source keys make a
 * restarted sweep continue routing alerts created before a previous failure. */
export async function sweepQualityAlerts(repository:QualityAlertSweepRepository,now:number,warningWindowMs:number):Promise<QualityAlertResult>{const today=qualityAlertDate(now),horizon=qualityAlertDate(now+warningWindowMs),dueSoon=await repository.dueSoon(now,today,horizon),overdue=await repository.overdue(now,today);await repository.notify(now);return {dueSoon,overdue};}
export function createQualityAlertApplication(ports:{departmentScope:boolean;userId:string;authorizeRead():void;can(permission:string):boolean;clock:Clock;repository():Promise<QualityAlertRepository>}){
 function access(){ports.authorizeRead();if(ports.departmentScope)throw new BusinessRuleError(403,'QUALITY_SCOPE_REQUIRED','Use a Company or Branch quality assignment.');}
 async function qualityAlerts(){access();return (await ports.repository()).alerts();}
 async function getQualityAlert(id:string){const row=(await qualityAlerts()).find(alert=>alert.id===id);if(!row)throw new BusinessRuleError(404,'QUALITY_ALERT_NOT_FOUND','Quality escalation is not available in this scope.');return row;}
 async function acknowledgeQualityAlert(id:string,raw:unknown,key:string){const input=acknowledgeSchema.parse(raw);if(!ports.can('quality.inspect')&&!ports.can('quality.release'))throw new BusinessRuleError(403,'QUALITY_ALERT_PERMISSION_REQUIRED','A quality inspector or manager must acknowledge this escalation.');const r=await ports.repository(),operation='quality.alert.acknowledge',prior=await r.replay(key,operation,id,input);if(prior)return prior;const alert=await getQualityAlert(id);if(alert.acknowledged_by)throw new BusinessRuleError(409,'QUALITY_ALERT_ACKNOWLEDGED','This escalation has already been acknowledged.');return r.acknowledge(id,key,input,alert,ports.clock.now());}
 async function qualityAlertWork():Promise<WorkItem[]>{if(!ports.can('quality.read'))return [];const alerts=await qualityAlerts();return alerts.map((alert) => {
    const completed = Boolean(alert.acknowledged_by) || qualityTerminalStates.includes(alert.status);
    const canAct = !completed && Boolean(alert.can_ack) &&
      (ports.can( "quality.inspect") || ports.can( "quality.release"));
    const overdue = alert.phase === "overdue";
    return {
      id: String(alert.id),
      type: "quality_alert" as const,
      actionKind: "work" as const,
      source: { kind: "quality_hold", id: String(alert.hold_id), href: `/quality/${encodeURIComponent(alert.hold_id)}`, label: "Quality hold", version: Number(alert.hold_version) },
      title: `${overdue ? "Overdue quality hold" : "Quality hold due soon"} · ${alert.product_name}`,
      detail: `${alert.warehouse_name} · due ${alert.due_date}`,
      href: `/quality/${encodeURIComponent(alert.hold_id)}`,
      status: completed ? "acknowledged" : alert.phase,
      updatedAt: Number(alert.acknowledged_at ?? alert.detected_at),
      queue: completed ? "completed" : canAct ? "todo" : "waiting",
      canAct,
      dueAt: Date.parse(`${alert.due_date}T00:00:00Z`),
      responsibleQueue: "Eligible Quality inspector or manager",
      nextAction: completed
        ? "Follow the recorded quality recovery plan"
        : overdue
          ? "Take responsibility and record the overdue follow-up plan"
          : "Prepare the next inspection or release action",
      blocker:
        !completed && !Number(alert.recipients)
          ? "No eligible Quality recipient; Company administrator must assign quality authority"
          : null,
      relationships: [canAct ? "assigned" : "team"],
    };
  });
}
 return {qualityAlerts,getQualityAlert,acknowledgeQualityAlert,qualityAlertWork};
}
