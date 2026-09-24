import { expect, test, type BrowserContext, type Page, type Locator } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = {
  email: string;
  role: string;
  roles?: string[];
  label: string;
  userId: string;
};
type Credentials = { password: string; users: DemoUser[] };

async function selectRole(
  context: BrowserContext,
  credentials: Credentials,
  role: string,
) {
  const user = credentials.users.find(
    (item) => item.roles?.includes(role) || item.role === role,
  );
  expect(user, `${role} demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: "http://127.0.0.1:3102" },
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find(
    (item: { role: string }) => item.role === role,
  );
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: "http://127.0.0.1:3102" },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  return user!;
}

async function signInRole(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  role: string,
  path: string,
) {
  const user = await selectRole(context, credentials, role);
  await page.goto(`${path}${path.includes("?") ? "&" : "?"}refresh=${Date.now()}`, {
    waitUntil: "commit",
  });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
  return user!;
}

test("isolated organization setup supports reviewed correction, safe suspension and retained history", async ({ browser }, testInfo) => {
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(180_000);
  const credentials=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`),"utf8")) as Credentials;
  const contexts=await Promise.all(Array.from({length:3},()=>browser.newContext(testInfo.project.use)));
  contexts.forEach(context=>context.setDefaultTimeout(15_000));
  const [ownerContext,reviewerContext,staffContext]=contexts;
  const path="/holdings/simulation/people/organization";
  const main=(page:Page)=>page.locator("#workspace-content");
  const queue=(page:Page)=>main(page).getByRole("region",{name:"Organization change requests",exact:true});
  const card=(page:Page,label:string)=>queue(page).getByRole("article",{name:label,exact:true});
  const unit=(page:Page,name:string)=>main(page).locator("section.card").filter({has:page.getByRole("heading",{name,exact:true})});
  const recordIds:string[]=[];
  async function command(page:Page,button:Locator,suffix:string,status=200){
    const pending=page.waitForResponse(response=>response.request().method()==="POST"&&new URL(response.url()).pathname.endsWith(`/people/organization/changes/${suffix}`));
    await button.click();const response=await pending;
    expect(response.status(),await response.text()).toBe(status);
    return (await response.json()).data as {id:string;nodeId?:string;state:string;version:number}|undefined;
  }
  async function review(page:Page,label:string,id:string,decision:"approve"|"reject",reason:string,status=200){
    if(new URL(page.url()).searchParams.get("changeId")===id)await page.reload();
    else await page.goto(`${path}?changeId=${encodeURIComponent(id)}#organization-change-${encodeURIComponent(id)}`);
    const request=card(page,label);await expect(request).toBeVisible();
    await request.getByLabel("Review reason",{exact:true}).fill(reason);
    return command(page,request.getByRole("button",{name:decision==="approve"?"Approve and apply":"Return for correction",exact:true}),`${id}/review`,status);
  }
  try{
    const ownerPage=await ownerContext.newPage(),reviewerPage=await reviewerContext.newPage();
    const owner=await signInRole(ownerContext,ownerPage,credentials,"holding_owner",path);
    const reviewer=await signInRole(reviewerContext,reviewerPage,credentials,"hr_admin",path);
    expect(reviewer.userId).not.toBe(owner.userId);
    const suffix=Date.now(),originalName=`Regional service ${suffix}`,branchName=`Reviewed service ${suffix}`,departmentName=`Repair team ${suffix}`;
    const proposal=main(ownerPage).locator("details").filter({has:ownerPage.getByText("Propose a new branch or department",{exact:true})});
    await proposal.getByText("Propose a new branch or department",{exact:true}).click();
    await proposal.getByLabel("Stable URL code",{exact:true}).fill(`reviewed-${suffix}`);
    await proposal.getByLabel("Display name",{exact:true}).fill(originalName);
    await proposal.getByLabel("Business reason",{exact:true}).fill("Create regional service capacity after review.");
    const draft=(await command(ownerPage,proposal.getByRole("button",{name:"Save draft",exact:true}),"draft"))!;
    recordIds.push(draft.id);expect(draft.state).toBe("draft");
    await ownerPage.reload();
    let request=card(ownerPage,`create branch: ${originalName}`);
    await request.getByText("Correct and resubmit",{exact:true}).click();
    await command(ownerPage,request.getByRole("button",{name:"Submit for review",exact:true}),`${draft.id}/submit`);
    await expect(request.getByText(/Awaiting an independent Company HR/)).toBeVisible();
    await expect(request.getByRole("button",{name:"Approve and apply",exact:true})).toHaveCount(0);
    await reviewerPage.goto("/holdings/simulation/work?type=organization_change&view=actions");
    const workLink=main(reviewerPage).getByRole("link").filter({hasText:`create branch: ${originalName}`});
    await expect(workLink).toBeVisible();await workLink.click();
    await expect(reviewerPage).toHaveURL(new RegExp(`changeId=${draft.id}`));
    await expect(card(reviewerPage,`create branch: ${originalName}`).getByRole("button",{name:"Approve and apply",exact:true})).toBeVisible();
    await review(reviewerPage,`create branch: ${originalName}`,draft.id,"reject","Clarify the operating branch name before opening.");
    await ownerPage.goto("/holdings/simulation/inbox?tab=updates");
    const returnedLink=main(ownerPage).getByRole("link",{name:`Organization change returned for correction: ${originalName}`,exact:true}).first();
    await expect(returnedLink).toBeVisible();await returnedLink.click();
    await expect(ownerPage).toHaveURL(new RegExp(`changeId=${draft.id}`));
    request=card(ownerPage,`create branch: ${originalName}`);
    await request.getByText("Correct and resubmit",{exact:true}).click();
    await request.getByLabel("Display name",{exact:true}).fill(branchName);
    await request.getByLabel("Business reason",{exact:true}).fill("Reviewed service region and its operating name.");
    await command(ownerPage,request.getByRole("button",{name:"Submit for review",exact:true}),`${draft.id}/submit`);
    const applied=(await review(reviewerPage,`create branch: ${branchName}`,draft.id,"approve","Operating scope and business reason verified independently."))!;
    expect(applied.version).toBe(5);expect(applied.nodeId).toBeTruthy();
    await ownerPage.reload();await expect(unit(ownerPage,branchName)).toContainText("branch · active · revision 1");
    await queue(ownerPage).getByRole("combobox",{name:"Change status",exact:true}).selectOption("applied");
    await queue(ownerPage).getByRole("button",{name:"Filter changes",exact:true}).click();
    const history=card(ownerPage,`create branch: ${branchName}`);
    await history.getByText("Decision history",{exact:true}).click();
    await expect(history.locator("ol li")).toHaveCount(5);
    await expect(history.locator("ol")).toContainText("Clarify the operating branch name before opening.");
    await expect(history.locator("ol")).toContainText("Operating scope and business reason verified independently.");
    await ownerPage.goto(path); // New proposals are shown in the default open queue.

    const proposeUnit=async(name:string,kind:"branch"|"department",action:"rename"|"suspend"|"reactivate",newName=name)=>{
      await ownerPage.reload();const target=unit(ownerPage,name);
      await target.getByText("Propose a change",{exact:true}).click();
      await target.getByRole("combobox",{name:"Change",exact:true}).selectOption(action);
      await target.getByLabel("Display name",{exact:true}).fill(newName);
      await target.getByLabel("Business reason",{exact:true}).fill(`${action} ${kind} after checking business dependencies.`);
      const result=(await command(ownerPage,target.getByRole("button",{name:"Submit for review",exact:true}),"submit"))!;
      recordIds.push(result.id);await expect(card(ownerPage,`${action} ${kind}: ${newName}`)).toBeVisible();return result;
    };
    const renamedName=`${branchName} hub`;
    const rename=await proposeUnit(branchName,"branch","rename",renamedName);
    await review(reviewerPage,`rename branch: ${renamedName}`,rename.id,"approve","The public operating name matches the reviewed location.");
    await ownerPage.reload();await expect(unit(ownerPage,renamedName)).toContainText("revision 2");
    await proposal.getByText("Propose a new branch or department",{exact:true}).click();
    await proposal.getByRole("combobox",{name:"Unit type",exact:true}).selectOption("department");
    await proposal.getByLabel("Stable URL code",{exact:true}).fill(`repair-${suffix}`);
    await proposal.getByLabel("Display name",{exact:true}).fill(departmentName);
    await proposal.getByRole("combobox",{name:"Branch",exact:true}).selectOption(applied.nodeId!);
    await proposal.getByLabel("Business reason",{exact:true}).fill("Place the repair team under its reviewed branch.");
    const department=(await command(ownerPage,proposal.getByRole("button",{name:"Submit for review",exact:true}),"submit"))!;
    recordIds.push(department.id);
    await review(reviewerPage,`create department: ${departmentName}`,department.id,"approve","Reviewed branch placement and team responsibility.");

    const blocked=await proposeUnit(renamedName,"branch","suspend");
    await review(reviewerPage,`suspend branch: ${renamedName}`,blocked.id,"approve","Attempt to suspend while the child department is active.",409);
    await expect(queue(reviewerPage).getByRole("alert")).toContainText("Move or close active dependencies before suspend");
    await expect(card(reviewerPage,`suspend branch: ${renamedName}`)).toContainText("submitted");
    await ownerPage.reload();await expect(unit(ownerPage,renamedName)).toContainText("branch · active");
    await command(ownerPage,card(ownerPage,`suspend branch: ${renamedName}`).getByRole("button",{name:"Cancel request",exact:true}),`${blocked.id}/cancel`);
    await ownerPage.goto(`${path}?changeState=cancelled`);
    await expect(card(ownerPage,`suspend branch: ${renamedName}`)).toContainText("cancelled");
    await ownerPage.goto(path);

    for(const [name,kind,action] of [[departmentName,"department","suspend"],[renamedName,"branch","suspend"],[renamedName,"branch","reactivate"],[departmentName,"department","reactivate"]] as const){
      const change=await proposeUnit(name,kind,action);
      await review(reviewerPage,`${action} ${kind}: ${name}`,change.id,"approve","Reviewed current dependencies and intended operating status.");
      await ownerPage.reload();await expect(unit(ownerPage,name)).toContainText(`${kind} · ${action==="suspend"?"suspended":"active"}`);
    }

    const organizationState=async()=>{
      const response=await ownerContext.request.get("/api/v1/holdings/simulation/people/organization?changeState=all");
      expect(response.ok(),await response.text()).toBe(true);
      return (await response.json()).data;
    };
    const destinationName=`Service destination ${suffix}`;
    await ownerPage.reload();
    await proposal.getByText("Propose a new branch or department",{exact:true}).click();
    await proposal.getByRole("combobox",{name:"Unit type",exact:true}).selectOption("branch");
    await proposal.getByLabel("Stable URL code",{exact:true}).fill(`service-destination-${suffix}`);
    await proposal.getByLabel("Display name",{exact:true}).fill(destinationName);
    await proposal.getByLabel("Business reason",{exact:true}).fill("Open an independently reviewed destination for team relocation.");
    const destinationChange=(await command(ownerPage,proposal.getByRole("button",{name:"Submit for review",exact:true}),"submit"))!;
    recordIds.push(destinationChange.id);
    const destinationApplied=(await review(reviewerPage,`create branch: ${destinationName}`,destinationChange.id,"approve","Verified the destination branch and operating purpose."))!;
    expect(destinationApplied.nodeId).toBeTruthy();

    const movableDepartmentName=`Movable service team ${suffix}`;
    await ownerPage.reload();
    await proposal.getByText("Propose a new branch or department",{exact:true}).click();
    await proposal.getByRole("combobox",{name:"Unit type",exact:true}).selectOption("department");
    await proposal.getByLabel("Stable URL code",{exact:true}).fill(`movable-team-${suffix}`);
    await proposal.getByLabel("Display name",{exact:true}).fill(movableDepartmentName);
    await proposal.getByRole("combobox",{name:"Branch",exact:true}).selectOption(applied.nodeId!);
    await proposal.getByLabel("Business reason",{exact:true}).fill("Create a team whose position will follow the approved relocation.");
    const movableChange=(await command(ownerPage,proposal.getByRole("button",{name:"Submit for review",exact:true}),"submit"))!;
    recordIds.push(movableChange.id);
    const movableApplied=(await review(reviewerPage,`create department: ${movableDepartmentName}`,movableChange.id,"approve","Verified the source branch and team placement."))!;
    expect(movableApplied.nodeId).toBeTruthy();

    const sourceState=await organizationState();
    const ceo=sourceState.positions.find((position:{is_ceo:number})=>position.is_ceo===1);
    const levelSix=sourceState.levels.find((level:{level_number:number})=>level.level_number===6);
    expect(ceo,"active reporting position").toBeTruthy();expect(levelSix,"level-six catalog entry").toBeTruthy();
    const positionResponse=await ownerContext.request.post("/api/v1/holdings/simulation/people/organization/positions",{
      headers:{Origin:"http://127.0.0.1:3102","Idempotency-Key":`organization-position-${run}`},
      data:{title:`Movable service position ${suffix}`,branchId:applied.nodeId,departmentId:movableApplied.nodeId,levelId:levelSix.id,reportsToPositionId:ceo.id,isCeo:false,active:true,expectedVersion:0,requirements:"Service coverage in the source team.",reason:"Seed the reviewed relocation with an active position."},
    });
    expect(positionResponse.ok(),await positionResponse.text()).toBe(true);
    const movablePosition=(await positionResponse.json()).data;

    await ownerPage.reload();
    const movableUnit=unit(ownerPage,movableDepartmentName);
    await movableUnit.getByText("Propose a change",{exact:true}).click();
    await movableUnit.getByLabel("Change",{exact:true}).selectOption("relocate");
    await movableUnit.getByLabel("Destination branch",{exact:true}).selectOption(destinationApplied.nodeId!);
    await movableUnit.getByLabel("Display name",{exact:true}).fill(movableDepartmentName);
    await movableUnit.getByLabel("Business reason",{exact:true}).fill("Move the service team's active position with the department.");
    const relocation=(await command(ownerPage,movableUnit.getByRole("button",{name:"Submit for review",exact:true}),"submit"))!;
    recordIds.push(relocation.id);
    const relocationApplied=(await review(reviewerPage,`relocate department: ${movableDepartmentName}`,relocation.id,"approve","Verified the destination; the position and department history must move together."))!;
    expect(relocationApplied.nodeId).toBeTruthy();expect(relocationApplied.nodeId).not.toBe(movableApplied.nodeId);
    const relocatedState=await organizationState();
    expect(relocatedState.nodes.find((node:{id:string})=>node.id===movableApplied.nodeId)).toMatchObject({status:"archived",branch_id:applied.nodeId});
    expect(relocatedState.nodes.find((node:{id:string})=>node.id===relocationApplied.nodeId)).toMatchObject({status:"active",branch_id:destinationApplied.nodeId,display_name:movableDepartmentName});
    expect(relocatedState.positions.find((position:{id:string})=>position.id===movablePosition.id)).toMatchObject({branch_id:destinationApplied.nodeId,department_id:relocationApplied.nodeId});

    const archiveDepartmentName=`Retained service team ${suffix}`;
    await ownerPage.reload();
    await proposal.getByText("Propose a new branch or department",{exact:true}).click();
    await proposal.getByRole("combobox",{name:"Unit type",exact:true}).selectOption("department");
    await proposal.getByLabel("Stable URL code",{exact:true}).fill(`retained-team-${suffix}`);
    await proposal.getByLabel("Display name",{exact:true}).fill(archiveDepartmentName);
    await proposal.getByRole("combobox",{name:"Branch",exact:true}).selectOption(destinationApplied.nodeId!);
    await proposal.getByLabel("Business reason",{exact:true}).fill("Create an unused team for a reviewed archive lifecycle.");
    const archiveSeed=(await command(ownerPage,proposal.getByRole("button",{name:"Submit for review",exact:true}),"submit"))!;
    recordIds.push(archiveSeed.id);
    const archiveSeedApplied=(await review(reviewerPage,`create department: ${archiveDepartmentName}`,archiveSeed.id,"approve","Verified the empty team has no assigned work."))!;
    await ownerPage.reload();
    const archiveUnit=unit(ownerPage,archiveDepartmentName);
    await archiveUnit.getByText("Propose a change",{exact:true}).click();
    await archiveUnit.getByLabel("Change",{exact:true}).selectOption("archive");
    await archiveUnit.getByLabel("Business reason",{exact:true}).fill("Retire the unused team while retaining its history.");
    const archiveChange=(await command(ownerPage,archiveUnit.getByRole("button",{name:"Submit for review",exact:true}),"submit"))!;
    recordIds.push(archiveChange.id);
    await review(reviewerPage,`archive department: ${archiveDepartmentName}`,archiveChange.id,"approve","Confirmed the team has no active dependencies and retained its identity history.");
    const archivedState=await organizationState();
    expect(archivedState.nodes.find((node:{id:string})=>node.id===archiveSeedApplied.nodeId)).toMatchObject({status:"archived"});
    await ownerPage.reload();
    const archivedUnit=unit(ownerPage,archiveDepartmentName);
    await archivedUnit.getByText("Propose a change",{exact:true}).click();
    await archivedUnit.getByLabel("Change",{exact:true}).selectOption("reactivate");
    await archivedUnit.getByLabel("Business reason",{exact:true}).fill("Reopen the retained team after reviewing current staffing needs.");
    const reactivateChange=(await command(ownerPage,archivedUnit.getByRole("button",{name:"Submit for review",exact:true}),"submit"))!;
    recordIds.push(reactivateChange.id);
    await review(reviewerPage,`reactivate department: ${archiveDepartmentName}`,reactivateChange.id,"approve","Reactivation approved with the retained record history intact.");
    expect((await organizationState()).nodes.find((node:{id:string})=>node.id===archiveSeedApplied.nodeId)).toMatchObject({status:"active"});

    const staffPage=await staffContext.newPage();
    await signInRole(staffContext,staffPage,credentials,"employee",path);
    await expect(queue(staffPage)).toHaveCount(0);
    const restricted=await staffContext.request.get("/api/v1/holdings/simulation/people/organization?changeState=all");
    expect(restricted.status()).toBe(200);expect((await restricted.json()).data.nodeChanges).toEqual([]);
    await ownerPage.goto(`${path}?changeState=all&changeQuery=${encodeURIComponent(branchName)}`);
    await expect(queue(ownerPage).getByRole("article")).toHaveCount(5);
    await testInfo.attach("organization-lifecycle-result",{contentType:"application/json",body:JSON.stringify({project:testInfo.project.name,run,recordIds,branchId:applied.nodeId,destinationBranchId:destinationApplied.nodeId,relocatedFromDepartmentId:movableApplied.nodeId,relocatedToDepartmentId:relocationApplied.nodeId,relocatedPositionId:movablePosition.id,archivedAndReactivatedDepartmentId:archiveSeedApplied.nodeId,requester:owner.userId,reviewer:reviewer.userId,finalBranchStatus:"active",finalDepartmentStatus:"active",blockedSuspensionStatus:409,workReviewLink:true,inboxCorrectionLink:true},null,2)});
    await queue(ownerPage).evaluate(element=>element.scrollIntoView({block:"start"}));
    await ownerPage.screenshot({path:testInfo.outputPath("organization-change-history.png")});
  }finally{await Promise.allSettled(contexts.map(context=>context.close()));}
});
