import { nativePpIdentityAppliedPackageEffectsSql } from '@erpx/server-collaboration/adapters/native-pp-identity_applied_package_effects-sql';
import type {PoolClient} from "pg";
import {lifecycleAppliedPackageAssignmentJoin} from "@erpx/server-identity_access/public/lifecycle-applied-package-projection";
export function stageLifecycleAppliedPackage(client:PoolClient,id:string,now:number){return client.query<Record<string,unknown>>(nativePpIdentityAppliedPackageEffectsSql('pp_identity_applied_package_effects_stmt_1'),[id,now]);}
