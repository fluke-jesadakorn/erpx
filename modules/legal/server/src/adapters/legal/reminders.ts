import {legalQueuedPurchasePredicate} from '@erpx/legal-server/adapters/queued-purchases';
import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {featureSpecs} from "@erpx/legal-ui/lib/legal/features";
import {legalFeatureNotices,legalSlaNotices,legalEscalationNotices,legalDeadlineNotice,legalEnvelopeExpire} from "@erpx/collaboration-server/adapters/native-legal-reminder-notifications";
import {legalRuntimePredicate} from "@erpx/legal-server/adapters/legal/runtime-gate";
import {createLegalReminderWorker} from "@erpx/legal-server/application/reminders";
import {legalWave3Cte} from '../native-legal-wave3';
export function legalReminders(db:SqlDatabase,holdingId:string){const scope={kind:'holding' as const,id:holdingId};return createLegalReminderWorker({clock:{now:()=>Date.now()},repository:{
 deadlines:async()=>{const predicate=`${legalRuntimePredicate('i.company_id')} AND ${await legalQueuedPurchasePredicate(db,'i','operations')}`;const cte=await legalWave3Cte(scope,'legal_deadline_cte',predicate);await legalDeadlineNotice(db,holdingId,cte);},
 feature:async kind=>legalFeatureNotices(db,holdingId,featureSpecs[kind].table,`${legalRuntimePredicate("i.company_id")} AND ${await legalQueuedPurchasePredicate(db,"i","feature")}`),
 sla:async now=>legalSlaNotices(db,holdingId,`${legalRuntimePredicate("i.company_id")} AND ${await legalQueuedPurchasePredicate(db,"i","operations")}`,now),
 escalations:async()=>legalEscalationNotices(db,holdingId,`${legalRuntimePredicate("i.company_id")} AND ${await legalQueuedPurchasePredicate(db,"i","operations")}`),
 expire:async times=>{const cte=await legalWave3Cte(scope,'legal_envelope_cte',legalRuntimePredicate('i.company_id'),[times.expires,times.updated]);await legalEnvelopeExpire(db,holdingId,cte,times.expires,times.updated,times.event);}
}});}
