import {z} from 'zod';
import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';
import type {StaffActor} from '../../operations/public/staff-policy';
import type {IamActor} from '../../identity_access/public/iam-context';
import type {proposeIamReview} from '../../identity_access/public/iam-service';
import type {GrantInput} from '../../identity_access/public/beta-invitation-contract';
import {requireVerified,type BetaUser} from '../domain/onboarding';
import {invitationSchema} from '../domain/team';
export interface BetaInvitationRepository{capacity(companyId:string,now:number):Promise<{ok:boolean}|null>;
activeBranch(branchId:string,companyId:string):Promise<boolean>;
existingMember(companyId:string,email:string):Promise<boolean>;
insertInvitation(id:string,email:string,branchId:string,userId:string,value:string,value2:number,now:number,role:"employee" | "it_admin"):Promise<void>;validateGrant(actor:IamActor,grant:GrantInput):Promise<void>;review(actor:IamActor,kind:string,payload:unknown,reason:string):Promise<Awaited<ReturnType<typeof proposeIamReview>>>;}
export interface BetaTeamRepository<Effect>{invitation(value:string):Promise<{
      id: string;
      email: string;
      branch_id: string;
      invited_by: string;
      expires_at: number;
      accepted_user_id: string | null;
      revoked_at: number | null;
      role: "employee" | "it_admin";
    }|null>;
acceptedAssignment(value:string,id:string,now:number):Promise<Record<string,unknown>|null>;
invitationBranch(branch_id:string):Promise<{ company_id: string }|null>;
reviewedInvitation(company_id:string,id:string):Promise<{
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
    }|null>;
guardInviter(key:string,value:"inviter-still-authorized",invited_by:string,company_id:string,now:number,now2:number):Effect;
guardInvitation(key:string,value:"invite",id:string,value2:string,now:number):Effect;
guardCapacity(key:string,value:"capacity",company_id:string,company_id2:string):Effect;
guardReviewedParent(key:string,value:"reviewed-parent",parentId:string,now:number,id:string,company_id:string,now2:number):Effect;
guardReviewedRole(key:string,value:string,roleId:string,company_id:string,expectedRoleRevision:number):Effect;
prepareItAssignment(value:string,id:string,company_id:string,now:number,validUntil:number,parentId:string):Effect;
prepareAcceptedInvitation(id:string,now:number,id2:string):Effect;
prepareAcceptanceAudit(value:string,key:string,id:string,value2:string,id2:string,now:number):Effect;
clearAcceptanceAssertions(key:string):Effect;
acceptedUser(id:string):Promise<{ accepted_user_id: string }|null>;
guardBranchCapacity(key:string,value:"branch-capacity",companyId:string,companyId2:string):Effect;batch(effects:Effect[]):Promise<void>;commit<Result>(actor:StaffActor,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[]):Promise<Result>;authorityGuard(key:string,actor:StaffActor,roles:string[]):Effect;memberStatements(company:string,branch:string,user:BetaUser,owner:boolean,invitation:string|null,grant?:{parentId:string;validUntil:number}):Effect[];branchStatements(company:string,id:string,name:string,slug:string):Effect[];}
export interface BetaTeamPorts<Effect>{parseGrant(input:unknown):GrantInput;repository:BetaTeamRepository<Effect>;clock:Clock;identifiers:IdentifierSource;hash(value:string):Promise<string>;requireCompanyAuthority(actor:StaffActor,roles:string[]):Promise<void>;requireFreshMfa(actor:IamActor):Promise<void>;invitationCommand<Result>(actor:IamActor,key:string,operation:string,input:unknown,permission:string,action:(repository:BetaInvitationRepository)=>Promise<Result>):Promise<Result>;}
export function createBetaTeamApplication<Effect>(ports:BetaTeamPorts<Effect>){const r=ports.repository;async function inviteMember(
  actor: StaffActor,
  rawInput: z.input<typeof invitationSchema>,
  key: string,
  token: string,
) {
  const input = invitationSchema.parse(rawInput);
  await ports.requireCompanyAuthority(actor, ["holding_owner"]);
  if (!actor.authority)
    throw new BusinessRuleError(
      403,
      "FRESH_MFA_REQUIRED",
      "Sign in and verify MFA before inviting a member.",
    );
  const iam = {
    ...actor,
    sessionId: actor.authority.sessionId,
    holdingId: null,
    scopeKind: "holding",
    scopeNodeId: null,
    platform: false,
  } satisfies IamActor;
  await ports.requireFreshMfa(iam);
  return ports.invitationCommand(
    
    iam,
    key,
    "beta.invite",
    input,
    "iam.manage",
    async (t) => {
      const now = ports.clock.now(),
        id = `invite-${key}`;
      const capacity = (await t.capacity(actor.companyId,now));
      if (!capacity?.ok)
        throw new BusinessRuleError(
          409,
          "MEMBER_CAPACITY",
          "This company has reached its member limit.",
        );
      if (
        !(await t.activeBranch(input.branchId,actor.companyId))
      )
        throw new BusinessRuleError(
          409,
          "BRANCH_UNAVAILABLE",
          "Choose an active company branch.",
        );
      if (
        (await t.existingMember(actor.companyId,input.email))
      )
        throw new BusinessRuleError(
          409,
          "MEMBER_EXISTS",
          "This person already has access. Use IAM to change assignments.",
        );
      const grants = [
        ports.parseGrant({
          userId: `invite:${id}`,
          roleId: "employee",
          scopeKind: "branch",
          scopeNodeId: input.branchId,
          reason: "Reviewed employee invitation",
        }),
      ];
      if (input.role === "it_admin")
        grants.push(
          ports.parseGrant({
            userId: `invite:${id}`,
            roleId: "it_admin",
            scopeKind: "holding",
            reason: "Reviewed IT administration invitation",
          }),
        );
      for (const grant of grants) await t.validateGrant(iam,grant);
      await t.insertInvitation(id,input.email,input.branchId,actor.userId,await ports.hash(token),now + 7 * 86400000,now,input.role);
      return {
        ...(await t.review(
          iam,
          "beta.invitation",
          { invitationId: id, grants, parentId: actor.assignmentId },
          "Authorize initial employee access.",
        )),
        invitationId: id,
      };
    },
  );
}
async function acceptMember(
  user: BetaUser,
  token: string,
) {
  
  requireVerified(user);
  const row = await r.invitation(await ports.hash(token));
  if (
    !row ||
    row.email !== user.email.toLowerCase() ||
    row.revoked_at ||
    row.expires_at <= ports.clock.now()
  )
    throw new BusinessRuleError(
      404,
      "INVITE_UNAVAILABLE",
      "Sign in with the verified email that was invited, or request a new invitation.",
    );
  if (row.accepted_user_id) {
    if (row.accepted_user_id !== user.id)
      throw new BusinessRuleError(409, "INVITE_USED", "Invitation already used.");
    if (
      !(await r.acceptedAssignment(`beta-assignment-${user.id}`,user.id,ports.clock.now()))
    )
      throw new BusinessRuleError(
        403,
        "ACCESS_REVOKED",
        "This company access was revoked. Ask the owner for a new invitation.",
      );
    return { joined: true };
  }
  const branch = await r.invitationBranch(row.branch_id);
  if (!branch)
    throw new BusinessRuleError(409, "BRANCH_UNAVAILABLE", "Branch unavailable.");
  const reviewed = await r.reviewedInvitation(branch.company_id,row.id);
  if (!reviewed)
    throw new BusinessRuleError(
      409,
      "INVITATION_REVIEW_REQUIRED",
      "An independent administrator must approve the initial grants before joining.",
    );
  const authorization = reviewed.payload;
  const key = `accept-${row.id}`,
    now = ports.clock.now();
  try {
    await r.batch([
      r.guardInviter(key,"inviter-still-authorized",row.invited_by,branch.company_id,now,now),
      r.guardInvitation(key,"invite",row.id,user.email.toLowerCase(),now),
      r.guardCapacity(key,"capacity",branch.company_id,branch.company_id),
      r.guardReviewedParent(key,"reviewed-parent",authorization.parentId,now,user.id,branch.company_id,now),
      ...authorization.grants.map((grant, index) =>
        r.guardReviewedRole(key,"reviewed-role-" + index,grant.roleId,branch.company_id,grant.expectedRoleRevision),
      ),
      ...r.memberStatements(branch.company_id,
        row.branch_id,
        user,
        false,
        row.id,
        {
          parentId: authorization.parentId,
          validUntil: authorization.grants[0].validUntil,
        },
      ),
      ...(row.role === "it_admin"
        ? [
            r.prepareItAssignment(`beta-it-${row.id}`,user.id,branch.company_id,now,authorization.grants.find((g) => g.roleId === "it_admin")!
                  .validUntil,authorization.parentId),
          ]
        : []),
      r.prepareAcceptedInvitation(user.id,now,row.id),
      r.prepareAcceptanceAudit(ports.identifiers.next(),key,user.id,`beta-assignment-${user.id}`,user.id,now),
      r.clearAcceptanceAssertions(key),
    ]);
  } catch {
    if (
      (
        await r.acceptedUser(row.id)
      )?.accepted_user_id !== user.id
    )
      throw new BusinessRuleError(
        409,
        "INVITE_CHANGED",
        "The invitation or company access changed. Request a new invitation.",
      );
  }
  return { joined: true };
}
async function addBetaBranch(
  actor: StaffActor,
  name: string,
  key: string,
) {
  await ports.requireCompanyAuthority(actor, ["holding_owner"]);
  const id = `branch-${key}`;
  return r.commit(actor,
    key,
    "beta.branch.create",
    { name },
    id,
    1,
    { id },
    [
      r.authorityGuard(key, actor, ["holding_owner"]),
      r.guardBranchCapacity(key,"branch-capacity",actor.companyId,actor.companyId),
      ...r.branchStatements(actor.companyId,
        id,
        z.string().trim().min(2).max(100).parse(name),
        id,
      ),
    ],
  );
}
return {inviteMember,acceptMember,addBetaBranch};}
