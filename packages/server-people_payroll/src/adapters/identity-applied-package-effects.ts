import { nativePpQuery } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import type {PoolClient} from "pg";
import {lifecycleAppliedPackageAssignmentJoin} from "@erpx/server-identity_access/public/lifecycle-applied-package-projection";
export function stageLifecycleAppliedPackage(client:PoolClient,id:string,now:number){return nativePpQuery<Record<string,unknown>>('pp_identity_applied_package_effects_stmt_1',[id,now]);}
