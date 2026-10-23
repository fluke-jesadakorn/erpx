import 'server-only';
import type { z } from 'zod';
import type {PoolClient} from 'pg';
import type {grantInput} from '@erpx/server-identity_access/domain/iam-service';
import type {IamActor} from '@erpx/server-identity_access/domain/iam-actor';
import {ApiError} from '@erpx/server-runtime/public/http';
import {createIamGrantPolicy} from '@erpx/server-identity_access/application/iam-grants';
import {grantTransaction} from '@erpx/server-identity_access/adapters/iam-grant-transaction';
const policy=()=>createIamGrantPolicy({error:(...args)=>new ApiError(...args),now:Date.now,id:()=>crypto.randomUUID()});
export async function validateIamGrant(c: PoolClient, actor: IamActor, input: z.infer<typeof grantInput>, parentId = actor.assignmentId) { return policy().validateIamGrant(grantTransaction(c, actor), actor, input, parentId); }
export async function insertIamGrant(c: PoolClient, actor: IamActor, input: z.infer<typeof grantInput>, parentId: string, source = 'manual', sourceId: string | null = null) { return policy().insertIamGrant(grantTransaction(c, actor), actor, input, parentId, source, sourceId); }
