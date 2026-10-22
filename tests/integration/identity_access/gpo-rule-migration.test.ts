import { expect, it, vi } from "vitest";
import { mkdtemp, readdir, copyFile, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { env } from "../../support/runtime-env";
import { migrateDatabase } from "@erpx/server-runtime/adapters/runtime/migrations";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { mapSpendingOrganization } from "../../support/helpers/spending-organization";

const provider=vi.hoisted(()=>({apply:vi.fn()}));
vi.mock("@erpx/server-identity_access/public/directory-provider",()=>({applyDirectoryAction:provider.apply,readAutomationInventory:vi.fn()}));

it("bridges direct mappings and every owned legacy claim without issuing Directory writes",async()=>{
  const root=await mkdtemp(join(tmpdir(),"erpx-gpo-migration-")),track=join(root,"holding-write"),db=env.H002_DB;
  try{
    await mkdir(track);
    const source=join(process.cwd(),"migrations/postgres/holding-write");
    for(const name of (await readdir(source)).filter(name=>name.endsWith(".sql")&&name<"0320_"))await copyFile(join(source,name),join(track,name));
    await migrateDatabase(db,"holding-write",root);
    // The organization fixture uses the current Position writer while this test
    // deliberately holds back the legacy GPO bridge migration.
    await copyFile(join(source,"0347_position_roster_required.sql"),join(track,"0347_position_roster_required.sql"));
    await migrateDatabase(db,"holding-write",root);
    const f=await seedExpenseCompany(db,env.H002_FILES,false);await mapSpendingOrganization(f);
    const person=`person-${f.head.userId}`,engagement=await db.prepare("SELECT id,position_id FROM hr_engagements WHERE person_id=?").bind(person).first<{id:string;position_id:string}>();
    const connection=`migration-connection-${crypto.randomUUID()}`;
    await db.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'Migration fixture','active_directory','{}','active',?,1)").bind(connection,f.company,f.it.userId).run();
    await db.prepare("INSERT INTO position_gpo_policies(company_id,position_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,updated_at) VALUES(?,?,2,2,?,'gpo','group','ou','directory_applied',1)").bind(f.company,engagement!.position_id,connection).run();
    await db.prepare("INSERT INTO position_gpo_revisions(id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,reason,command_id,created_at) VALUES(?,?,?,2,?,'gpo','group','ou','v2','[]',?,'Historic mapping',?,1)")
      .bind(crypto.randomUUID(),f.company,engagement!.position_id,connection,f.it.userId,crypto.randomUUID()).run();
    for(const group of ["group","orphan-group"])await db.prepare("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES(?,?,'account',?,1)").bind(connection,group,`position-gpo:${engagement!.id}`).run();
    await db.prepare(`INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,engagement_id,account_id,revision,engagement_version,connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,status,created_at,updated_at)
      VALUES(?,?,?,?,?,'account',2,1,?,'gpo','group','ou','add','directory_applied',1,1)`)
      .bind("historic-job",f.company,engagement!.position_id,person,engagement!.id,connection).run();
    // Ownership must survive even when the old employee is no longer assigned.
    await db.prepare("UPDATE hr_engagements SET position_id=NULL,status='ended',version=version+1 WHERE id=?").bind(engagement!.id).run();
    const before=(await db.prepare("SELECT * FROM people_directory_membership_sources WHERE connection_id=? ORDER BY group_id").bind(connection).all()).results;
    await copyFile(join(source,"0320_organization_gpo_rules.sql"),join(track,"0320_organization_gpo_rules.sql"));
    await migrateDatabase(db,"holding-write",root);
    expect(provider.apply).not.toHaveBeenCalled();
    expect((await db.prepare("SELECT * FROM people_directory_membership_sources WHERE connection_id=? ORDER BY group_id").bind(connection).all()).results).toEqual(before);
    const rules=(await db.prepare("SELECT id,target_kind,target_id,version,revision,directory_config_status FROM organization_gpo_rules WHERE company_id=?").bind(f.company).all()).results;
    expect(rules).toEqual([{id:`legacy-position:${engagement!.position_id}`,target_kind:"position",target_id:engagement!.position_id,version:2,revision:2,directory_config_status:"directory_applied"}]);
    const claims=(await db.prepare("SELECT group_id,position_id,rule_id FROM organization_gpo_membership_claims WHERE connection_id=? ORDER BY group_id").bind(connection).all()).results;
    expect(claims).toEqual([
      {group_id:"group",position_id:engagement!.position_id,rule_id:`legacy-position:${engagement!.position_id}`},
      {group_id:"orphan-group",position_id:"unassigned",rule_id:`legacy-orphan:${engagement!.id}`},
    ]);
    expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE id='historic-job'").first("status")).toBe("directory_applied");
    expect(await db.prepare("SELECT count(*)::int count FROM position_gpo_config_ownership WHERE connection_id=?").bind(connection).first("count")).toBe(0);
    const levelId=await db.prepare("SELECT level_id FROM organization_positions WHERE id=?").bind(engagement!.position_id).first<string>("level_id");
    await db.prepare(`INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,last_actor_id,last_reason,updated_at)
      VALUES('inherited-rule',?,'level',?,1,1,?,'gpo','level-group','ou','directory_applied',?,'Historical level rule',1)`)
      .bind(f.company,levelId,connection,f.it.userId).run();
    const covered=(await db.prepare("SELECT id FROM organization_positions WHERE company_id=? AND level_id=? AND active=1 ORDER BY id").bind(f.company,levelId).all<{id:string}>()).results.map(row=>row.id);
    const oldSource=`gpo-rule:${JSON.stringify(["inherited-rule",engagement!.id])}`;
    await db.prepare("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES(?,'level-group','account',?,1)").bind(connection,oldSource).run();
    await db.prepare(`INSERT INTO organization_gpo_membership_claims(connection_id,group_id,member_id,source_id,rule_id,company_id,position_id,engagement_id,person_id,revision,engagement_version,gpo_id,computer_target_ou_id)
      VALUES(?,'level-group','account',?,'inherited-rule',?,?,?,?,1,1,'gpo','ou')`)
      .bind(connection,oldSource,f.company,engagement!.position_id,engagement!.id,person).run();
    await copyFile(join(source,"0360_position_gpo_tasks.sql"),join(track,"0360_position_gpo_tasks.sql"));
    await migrateDatabase(db,"holding-write",root);
    expect(provider.apply).not.toHaveBeenCalled();
    expect((await db.prepare("SELECT target_id FROM organization_gpo_rules WHERE company_id=? AND target_kind='position' AND id LIKE 'positionized:%' ORDER BY target_id").bind(f.company).all<{target_id:string}>()).results.map(row=>row.target_id)).toEqual(covered);
    expect(await db.prepare("SELECT active FROM organization_gpo_rules WHERE id='inherited-rule'").first("active")).toBe(false);
    const claim=await db.prepare("SELECT rule_id,source_id FROM organization_gpo_membership_claims WHERE connection_id=? AND group_id='level-group'").bind(connection).first<{rule_id:string;source_id:string}>();
    expect(claim?.rule_id).toBe(`positionized:${(await import("node:crypto")).createHash("md5").update(`inherited-rule:${engagement!.position_id}`).digest("hex")}`);
    expect(claim?.source_id).toBe(`gpo-rule:${JSON.stringify([claim!.rule_id,engagement!.id])}`);
    expect(await db.prepare("SELECT count(*)::int count FROM people_directory_membership_sources WHERE connection_id=? AND group_id='level-group' AND source_id=?").bind(connection,claim!.source_id).first("count")).toBe(1);
    expect(await db.prepare("SELECT count(*)::int count FROM position_gpo_tasks WHERE company_id=?").bind(f.company).first("count")).toBeGreaterThanOrEqual(covered.length);
  }finally{await rm(root,{recursive:true,force:true});}
});
