import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function leaveReviewEvidenceEffects(database:SqlDatabase){return {stageDecideEvidenceManifestStep(value: string, id: string, company_id: string, id2: string, key: string, expectedVersion: number, value2: string | null, userId: string, value3: string, now: number) {
            return database.prepare(`INSERT INTO leave_evidence_manifests(
              id,request_id,company_id,approval_instance_id,decision_command_id,
              request_version,policy_version_id,reviewer_user_id,manifest_json,created_at
            ) VALUES(?,?,?,?,?,?,?,?,?::jsonb,?)`)
                .bind(value, id, company_id, id2, key, expectedVersion, value2, userId, value3, now);
        }};}
