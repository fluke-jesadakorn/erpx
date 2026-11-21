import { nativeIdentityWrite } from '@erpx/server-identity_access/public/native-identity-write';
import {stageLifecycleAppliedPackage} from "@erpx/server-people_payroll/public/identity-applied-package-effects";
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import type { PoolClient } from 'pg';
export function lifecycleIdentityEffects(client: PoolClient) {
    return { applyEffect(company_id: string, person_id: string, effective_at: number) { return nativeIdentityWrite("w_e02e16d099d9", [company_id, person_id, effective_at]); },
        applyEffect2(company_id: string, person_id: string) { return nativeIdentityWrite("w_39048208fdaf", [company_id, person_id]); },
        readValid(authorizing_assignment_id: string, now: number) { return client.query<Record<string, unknown>>("SELECT 1 FROM assignments a WHERE a.id=$1 AND iam_assignment_active(a.id,$2) AND iam_role_allows(a.role,a.company_id,'people.manage')", [authorizing_assignment_id, now]); },
        applyEffect7(company_id: string, person_id: string, effective_at: number, id: string) { return nativeIdentityWrite("w_worker_effect_revoke", [company_id, person_id, effective_at, id]); },
        applyEffect8(company_id: string, person_id: string, effective_at: number, id: string) { return nativeIdentityWrite("w_worker_effect_revoke_cutoff", [company_id, person_id, effective_at, id]); },
        applyEffect11(id: string, now: number) { return stageLifecycleAppliedPackage(client,id,now); },
        applyEffect22(value: string, company_id: string, id: string, value2: string, now: number) { return nativeIdentityWrite("w_3ee473c1b14f", [value, company_id, id, value2, now]); } };
}
