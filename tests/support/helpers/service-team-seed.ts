import {expenseDb as db} from "./expense-fixture";
import type {ServiceTeamInput} from "@erpx/server-support/public/service-teams";

/**
 * Seed an active or inactive service-team owner row set. Native Support owns the
 * command invariants; retained TypeScript read/routing tests only need the
 * resulting rows.
 */
export async function seedServiceTeam(companyId:string,createdBy:string,input:ServiceTeamInput,id:string,now=Date.now()){
  await db.batch([
    db.prepare("INSERT INTO service_teams(id,company_id,name,active,version,configuration_json,created_by,created_at,updated_at) VALUES(?,?,?,?,1,?,?,?,?)").bind(id,companyId,input.name,input.active?1:0,JSON.stringify(input),createdBy,now,now),
    db.prepare("INSERT INTO service_team_revisions(company_id,team_id,version,configuration_json,actor_id,created_at) VALUES(?,?,1,?,?,?)").bind(companyId,id,JSON.stringify(input),createdBy,now),
    ...input.members.map(member=>db.prepare("INSERT INTO service_team_members(company_id,team_id,user_id,responsibility) VALUES(?,?,?,?)").bind(companyId,id,member.userId,member.responsibility)),
    ...(input.active?input.coverage:[]).map(coverage=>db.prepare("INSERT INTO service_team_coverage(company_id,team_id,category,scope_kind,scope_node_id) VALUES(?,?,?,?,?)").bind(companyId,id,coverage.category,coverage.scopeKind,coverage.scopeNodeId)),
  ]);
  return {id,version:1,input};
}

/** Seed a second recorded configuration for the same team. */
export async function reviseServiceTeam(companyId:string,teamId:string,version:number,input:ServiceTeamInput,actorId:string,now=Date.now()){
  await db.batch([
    db.prepare("UPDATE service_teams SET name=?,active=?,version=?,configuration_json=?,updated_at=? WHERE id=? AND company_id=?").bind(input.name,input.active?1:0,version,JSON.stringify(input),now,teamId,companyId),
    db.prepare("INSERT INTO service_team_revisions(company_id,team_id,version,configuration_json,actor_id,created_at) VALUES(?,?,?,?,?,?)").bind(companyId,teamId,version,JSON.stringify(input),actorId,now),
  ]);
  return {id:teamId,version,input};
}
