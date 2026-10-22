import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function betaInvitationSources(db:SqlDatabase){return {async acceptedAssignment(value:string,id:string,now:number){return db
        .prepare(
          "SELECT 1 FROM assignments WHERE id=? AND user_id=? AND revoked_at IS NULL AND valid_until>?",
        )
        .bind(value, id, now)
        .first();},
async reviewedInvitation(company_id:string,id:string){return db
    .prepare(
      "SELECT payload FROM iam_reviews WHERE company_id=? AND kind='beta.invitation' AND payload->>'invitationId'=? AND status='approved'",
    )
    .bind(company_id, id)
    .first<{
      payload: {
        parentId: string;
        grants: {
          roleId: string;
          expectedRoleRevision: number;
          scopeKind: string;
          scopeNodeId: string | null;
          validUntil: number;
        }[];
      };
    }>();},
guardInviter(key:string,value:"inviter-still-authorized",invited_by:string,company_id:string,now:number,now2:number){return assertion(
        db,
        key,
        value,
        "EXISTS(SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND iam_role_allows(role,company_id,'company.owner') AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?)",
        [invited_by, company_id, now, now2],
      );},
guardReviewedParent(key:string,value:"reviewed-parent",parentId:string,now:number,id:string,company_id:string,now2:number){return assertion(
        db,
        key,
        value,
        "iam_assignment_active(?,?) AND iam_principal_active(?,?,?)",
        [parentId, now, id, company_id, now2],
      );},
guardReviewedRole(key:string,value:string,roleId:string,company_id:string,expectedRoleRevision:number){return assertion(
          db,
          key,
          value,
          "EXISTS(SELECT 1 FROM iam_roles WHERE id=? AND company_id IN ('*',?) AND published_revision=?)",
          [roleId, company_id, expectedRoleRevision],
        );},
prepareItAssignment(value:string,id:string,company_id:string,now:number,validUntil:number,parentId:string){return db
              .prepare(
                "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,parent_id,source) VALUES(?,?,'it_admin','[\"collaboration\"]',?,?,?,'holding',?,'manual')",
              )
              .bind(
                value,
                id,
                company_id,
                now,
                validUntil,
                parentId,
              );}};}
