import 'server-only';
import type { z } from 'zod';
import type {PoolClient} from 'pg';
import type {grantInput} from '@erpx/identity-access-server/domain/iam-service';
import type {IamActor} from '@erpx/identity-access-server/domain/iam-actor';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {createIamGrantPolicy} from '@erpx/identity-access-server/application/iam-grants';
import {grantTransaction} from '@erpx/identity-access-server/adapters/iam-grant-transaction';
const policy=()=>createIamGrantPolicy({error:(...args)=>new ApiError(...args),now:Date.now,id:()=>crypto.randomUUID()});
export async function validateIamGrant(c: PoolClient, actor: IamActor, input: z.infer<typeof grantInput>, parentId = actor.assignmentId) { return policy().validateIamGrant(grantTransaction(c, actor), actor, input, parentId); }
export async function insertIamGrant(c: PoolClient, actor: IamActor, input: z.infer<typeof grantInput>, parentId: string, source = 'manual', sourceId: string | null = null) { return policy().insertIamGrant(grantTransaction(c, actor), actor, input, parentId, source, sourceId); }
