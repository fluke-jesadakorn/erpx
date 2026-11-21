import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import { nativePeopleBatchB } from '@erpx/server-collaboration/adapters/native-people-batch-b';
export function leaveReviewEvidenceEffects(database:SqlDatabase){return {stageDecideEvidenceManifestStep(value: string, id: string, company_id: string, id2: string, key: string, expectedVersion: number, value2: string | null, userId: string, value3: string, now: number) {
            return nativePeopleBatchB(database).leaveEvidence(company_id, [value, id, company_id, id2, key, expectedVersion, value2, userId, value3, now]);
        }};}
