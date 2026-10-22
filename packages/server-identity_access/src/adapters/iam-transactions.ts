import 'server-only';
import {createHash} from 'node:crypto';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import {ApiError} from "@erpx/server-runtime/public/http";
import {requireFreshMfa} from "@erpx/server-identity_access/public/iam-mfa";
import {iamDb} from "@erpx/server-identity_access/public/iam-db";
import type {IamActor} from "@erpx/server-identity_access/domain/iam-actor";
import {createIamCommand} from "@erpx/server-identity_access/application/iam-command";
/** Owner extensions are registered only on this command's original client.
 * They run after the complete IAM decision, receipt and audit, before commit. */
const ownerChecks = new WeakMap<PoolClient, (() => Promise<void>)[]>();
export function assertIamOwnerTransaction(client: PoolClient) {
 if (!ownerChecks.has(client)) throw new ApiError(503, "IAM_OWNER_COMPOSITION_REQUIRED", "This operation requires its complete owned IAM transaction.");
}
export function deferIamOwnerCheck(client: PoolClient, check: () => Promise<void>) {
 assertIamOwnerTransaction(client); ownerChecks.get(client)!.push(check);
}
export type IamOwnerPreparation = (client: PoolClient) => Promise<(() => Promise<void>) | undefined>;
export async function iamCommand<T>(db:PostgresDatabase,actor:IamActor,key:string,operation:string,payload:unknown,permission:string,action:(client:PoolClient)=>Promise<T>,prepareOwner?:IamOwnerPreparation):Promise<T>{db=iamDb(db);return createIamCommand<T>({error:(status,code,message)=>new ApiError(status,code,message),now:Date.now,digest:value=>createHash('sha256').update(value).digest('hex'),transaction:async(actor,run)=>{const client=await db.pool.connect();try{await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE');ownerChecks.set(client,[]);const capture=await prepareOwner?.(client);const table=actor.platform?'platform_assignments':'assignments',recoveryQuery=`SELECT 1 FROM iam_members m JOIN ${table} a ON a.user_id=m.user_id ${actor.platform ? "" : "AND a.company_id=m.company_id"} WHERE m.company_id=$1 AND m.status='active' AND m.login_policy='emergency' AND a.role=$2 AND iam_assignment_active(a.id,$3) LIMIT 1`;const result=await run({assignment:async actor=>(
      await client.query(
        `SELECT * FROM ${table} WHERE id=$1 AND user_id=$2 ${actor.platform ? "" : "AND company_id=$3"} FOR SHARE`,
        actor.platform
          ? [actor.assignmentId, actor.userId]
          : [actor.assignmentId, actor.userId, actor.companyId],
      )
    ).rows[0],active:async(assignmentId,now)=>Boolean((await client.query("SELECT iam_assignment_active($1,$2) ok", [
          assignmentId,
          now,
        ])).rows[0].ok),allowed:async(role,companyId,permission)=>Boolean((await client.query("SELECT iam_role_allows($1,$2,$3) ok", [
          role,
          companyId,
          permission,
        ])).rows[0].ok),rolePolicy:async(role,companyId)=>(
      await client.query(
        "SELECT r.published_revision,v.grantable_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2) AND r.status='active' FOR SHARE OF r",
        [role, companyId],
      )
    ).rows[0],member:async actor=>(
      await client.query(
        "SELECT * FROM iam_members WHERE company_id=$1 AND user_id=$2 FOR SHARE",
        [actor.companyId, actor.userId],
      )
    ).rows[0],directoryIdentity:async(actor,member)=>(
        await client.query(
          "SELECT i.enabled,i.observed_at,c.status FROM iam_identities i JOIN iam_connections c ON c.id=i.connection_id WHERE i.id=$1 AND i.user_id=$2 AND i.company_id=$3",
          [member.identity_id, actor.userId, actor.companyId],
        )
      ).rows[0],revoked:async actor=>Boolean((await client.query(
          "SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1 AND company_id=$2 AND user_id=$3",
          [actor.sessionId, actor.companyId, actor.userId],
        )).rowCount),workContext:async actor=>(
        await client.query(
          "SELECT * FROM active_work_contexts WHERE session_id=$1 AND company_id=$2 FOR UPDATE",
          [actor.sessionId, actor.companyId],
        )
      ).rows[0],receipt:async key=>(
      await client.query("SELECT * FROM iam_receipts WHERE id=$1", [key])
    ).rows[0],recovery:async(actor,now)=>Boolean((
      await client.query(recoveryQuery, [
        actor.companyId,
        actor.platform ? "platform_admin" : "holding_owner",
        now,
      ])
    ).rowCount),perform:async()=>{const result=await action(client);await capture?.();return result;},freshMfa:requireFreshMfa,recordReceipt:async(request,result,now)=>{const{actor,key,digest}=request;await client.query("INSERT INTO iam_receipts VALUES($1,$2,$3,$4,$5,$6)", [
      key,
      actor.companyId,
      actor.userId,
      digest,
      JSON.stringify(result),
      now,
    ]);},recordAudit:async(request,result,now)=>{const{actor,key,operation,payload}=request;await client.query(
      "INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
      [
        crypto.randomUUID(),
        actor.companyId,
        actor.userId,
        actor.assignmentId,
        operation,
        key,
        JSON.stringify({
          input: payload,
          result,
          role: actor.role,
          scope: actor.scopeKind,
          roleRevision: actor.iam?.roleRevision,
        }),
        now(),
      ],
    );}});for(const check of ownerChecks.get(client)!)await check();await client.query('COMMIT');return result;}catch(error){await client.query('ROLLBACK');if(['40001','40P01','23505'].includes((error as{code?:string}).code??''))throw new ApiError(409,'IAM_CONFLICT','Access changed. Refresh and review before retrying.');throw error;}finally{ownerChecks.delete(client);client.release();}}})(actor,key,operation,payload,permission);}
