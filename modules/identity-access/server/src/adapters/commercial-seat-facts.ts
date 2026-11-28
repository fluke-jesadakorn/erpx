import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';

export function commercialSeatCandidates(db:SqlDatabase,asOf:number){return db.prepare(`SELECT DISTINCT a.user_id
    FROM assignments a JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id)
      AND r.status='active'
    JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id
      AND v.revision=r.published_revision
    WHERE a.scope_kind IN ('holding','branch','department')
      AND a.user_id !~* '^(system:|service:|platform:|bot:)'
      AND jsonb_array_length(v.permissions_json)>0
      AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)`)
    .bind(asOf,asOf).all<{user_id:string}>();}
/** One owner-local projection for the platform-identity seat account read. Both
 * exported callers previously repeated this literal; a single parameterized
 * statement now serves them so the projection cannot drift. Only the
 * already-authorized id list is accepted from a caller; the SQL is fixed here. */
const seatAccountsSql = (ids:string[]) => `SELECT u.id user_id,p.status,
      EXISTS(SELECT 1 FROM platform_assignments x WHERE x.user_id=u.id) platform_identity
      FROM "user" u LEFT JOIN principals p ON p.user_id=u.id
      WHERE u.id IN (${ids.map(()=>"?").join(",")})`;
export function commercialSeatAccounts(db:SqlDatabase,ids:string[]){return db.prepare(seatAccountsSql(ids))
      .bind(...ids).all<{user_id:string;status:string|null;platform_identity:boolean}>();}
export function commercialSeatProjectionAccounts(db:SqlDatabase,ids:string[]){return db.prepare(seatAccountsSql(ids))
    .bind(...ids)
    .all<{user_id:string;status:string|null;platform_identity:boolean}>();}
