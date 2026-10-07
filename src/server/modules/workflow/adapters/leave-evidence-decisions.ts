import type {SqlDatabase} from '../../runtime/public/runtime/types';export function leaveEvidenceDecisions(database:SqlDatabase){return {recordedDecision(companyId:string,leaveRequestId:string,userId:string){return database.prepare(
        `SELECT 1
         FROM approval_instances instance
         JOIN approval_decisions decision ON decision.instance_id=instance.id
         WHERE instance.company_id=? AND instance.record_type='leave'
           AND instance.record_id=?
           AND decision.action IN ('approve','reject','override')
           AND iam_same_person(decision.actor_user_id,?,instance.company_id)
         LIMIT 1`,
      )
      .bind(companyId, leaveRequestId, userId)
      .first();}};}
