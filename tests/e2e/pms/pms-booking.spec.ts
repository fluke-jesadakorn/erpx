import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
import { createEngineeringBrowserContext } from "../../support/browser/engineering-browser-evidence";
import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
test.use({actionTimeout:20_000});
const isolated =
  process.env.ERPX_BOOKING_E2E === "1" &&
  ["3102", "3104"].includes(new URL(process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100").port);
test.beforeAll(async () => {
  if (!isolated) return;
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "An exact owned booking simulation is required").toMatch(/^[a-f0-9]{12}$/);
  if (!run) throw new Error("EXACT_BOOKING_SIMULATION_REQUIRED");
  const state = JSON.parse(await readFile(resolve(".runtime/browser", run, "state.json"), "utf8"));
  expect(state).toMatchObject({ run, prefix: `erpx_browser_${run}`, profile: "booking", ready: true,
    origin: new URL(process.env.ERPX_E2E_BASE_URL!).origin });
});
test("guest portal is public without exposing internal PMS records", async ({
  page,
  request,
}) => {
  const slug = isolated ? "booking-test" : "acme";
  await page.goto(`/book/${slug}`);
  await expect(
    page.getByRole("heading", { name: "Find a stay that feels right." }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Manage booking" }),
  ).toBeVisible();
  const response = await request.post(`/api/public/booking/${slug}/manage`, {
    headers: {
      Origin: new URL(page.url()).origin,
      Authorization: "Bearer invalid",
    },
    data: {},
  });
  expect(response.status()).toBe(401);
  expect(JSON.stringify(await response.json())).not.toContain("guest@");
});
test("searches, quotes and confirms a family stay in an isolated hotel", async ({
  page,
}) => {
  test.skip(
    !isolated,
    "Requires an exact owned booking fixture on port 3102 or 3104; never creates a live reservation",
  );
  const arrival = new Date(); arrival.setUTCDate(arrival.getUTCDate() + 7);
  const departure = new Date(arrival); departure.setUTCDate(departure.getUTCDate() + 2);
  const cancellation = new Date(arrival); cancellation.setUTCDate(cancellation.getUTCDate() - 1);
  const arrivalDate = arrival.toISOString().slice(0, 10), departureDate = departure.toISOString().slice(0, 10), cancellationDate = cancellation.toISOString().slice(0, 10);
  await page.goto("/book/booking-test");
  await page.getByLabel("Check-in", { exact: true }).fill(arrivalDate);
  await page.getByLabel("Check-out", { exact: true }).fill(departureDate);
  await page.getByLabel("Child ages per room").fill("7");
  await page.getByRole("button", { name: "Search availability" }).click();
  await expect(page.getByText("THB 7,062.00", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Add rooms" }).click();
  await page.getByRole("button", { name: "Continue to guest details" }).click();
  await page.getByLabel("Full name").fill("Isolated browser guest");
  await page
    .getByLabel("Email", { exact: true })
    .fill("browser-qa@example.test");
  await page.getByLabel("Phone", { exact: true }).fill("020000000");
  await page.getByRole("button", { name: "Review price & policy" }).click();
  await expect(
    page.getByRole("heading", { name: "Review your stay" }),
  ).toBeVisible();
  await expect(
    page.getByText(`Free cancellation until ${cancellationDate} 18:00 (Asia/Bangkok). After this deadline, contact the hotel. Pay at the hotel.`, { exact: true }),
  ).toBeVisible();
  await page
    .getByLabel("I have reviewed the dates, full price and cancellation terms.")
    .check();
  await page.getByRole("button", { name: "Confirm reservation", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: /Booking confirmed · PMS-/ }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.locator('a[href*="/manage#token="]').click();
  await expect(page.getByText("Isolated browser guest", { exact: true })).toBeVisible();
  expect(new URL(page.url()).hash).toBe("");
  const selection = page.getByRole("checkbox", { name: `Select Family room ${arrivalDate}`, exact: true });
  await selection.check();
  await page.getByRole("button", { name: "Change selected rooms", exact: true }).click();
  const change = page.getByRole("dialog", { name: "Change your stay", exact: true });
  const extendedDeparture = new Date(departure); extendedDeparture.setUTCDate(extendedDeparture.getUTCDate() + 1);
  const extendedDate = extendedDeparture.toISOString().slice(0, 10);
  await change.getByLabel("Check-out", { exact: true }).fill(extendedDate);
  await change.getByRole("button", { name: "Review price and terms", exact: true }).click();
  await expect(change.getByText("THB 10,593.00", { exact: true }).first()).toBeVisible();
  await change.getByRole("button", { name: "Confirm changes", exact: true }).click();
  await expect(change).toBeHidden();
  await expect(page.getByRole("status")).toContainText("Your booking has been updated.");
  await page.reload();
  await expect(page.getByText(`${arrivalDate} → ${extendedDate}`, { exact: true })).toBeVisible();
  await selection.check();
  await page.getByRole("button", { name: "Cancel selected rooms", exact: true }).click();
  const cancellationReview = page.getByRole("dialog", { name: "Review cancellation", exact: true });
  await cancellationReview.getByRole("button", { name: "Review price and terms", exact: true }).click();
  await expect(cancellationReview.getByText("Cancellation fee: THB 0.00", { exact: true })).toBeVisible();
  await cancellationReview.getByRole("button", { name: "Confirm changes", exact: true }).click();
  await expect(cancellationReview).toBeHidden();
  await expect(selection).toBeDisabled();
  await page.reload();
  await expect(selection).toBeDisabled();
  await expect(page.getByText("2 adult(s) · 1 child(ren) · cancelled", { exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});


test("reviewed hotel and room galleries publish sanitized images and disappear after revocation", async ({ browser },testInfo) => {
  test.skip(!isolated,"Requires the exact owned booking/scanner/jobs cohort; never publishes live hotel images");
  test.setTimeout(240_000);
  const run=process.env.ERPX_DEMO_RUN!,directory=resolve('.runtime/browser',run);
  const {fixture}=JSON.parse(await readFile(resolve(directory,'state.json'),'utf8'));
  const credentials=JSON.parse(await readFile(resolve(directory,'credentials.json'),'utf8')) as {password:string;users:Array<{userId:string;email:string;assignmentId:string}>};
  expect(fixture.staff.authorId).not.toBe(fixture.staff.reviewerId);
  const origin=process.env.ERPX_E2E_BASE_URL!,settings=`/holdings/booking-test/pms/settings?property=${fixture.propertyId}`;
  const author=await createEngineeringBrowserContext(browser,testInfo,{baseURL:origin}),reviewer=await createEngineeringBrowserContext(browser,testInfo,{baseURL:origin}),guest=await createEngineeringBrowserContext(browser,testInfo,{baseURL:origin});
  try{
    const health=await (await author.request.get('/api/health')).json();
    expect(health.attachments.enabled).toBe(true);
    for(const [context,userId] of [[author,fixture.staff.authorId],[reviewer,fixture.staff.reviewerId]] as const){
      const user=credentials.users.find(u=>u.userId===userId)!;expect(user).toBeTruthy();
      const login=await context.request.post('/api/auth/sign-in/email',{headers:{Origin:origin},data:{email:user.email,password:credentials.password}});expect(login.status()).toBe(200);
      await selectFixtureAssignment(context,fixture.companyId,user.assignmentId,'booking-test');
    }
    const page=await author.newPage(),review=await reviewer.newPage(),publicPage=await guest.newPage();
    async function gallery(page:Page,title:string){
      await page.goto(settings);await page.locator('summary:visible').filter({hasText:title}).filter({hasText:'Gallery'}).click();
      const drawer=page.getByRole('dialog',{name:`Gallery · ${title}`,exact:true});await expect(drawer).toBeVisible();return drawer;
    }
    const items=[{title:'Isolated Family Hotel',file:'lobby.webp',alt:'Fictional hotel lobby sample — replace before live sale',targetType:'pms_property',targetId:fixture.propertyId},
      {title:'Family room',file:'suite-main.webp',alt:'Fictional family suite sample — replace before live sale',targetType:'pms_room_type',targetId:fixture.typeId}];
    const publications:Array<{id:string;url:string;alt:string;title:string}>=[];
    for(const item of items){
      let drawer=await gallery(page,item.title);
      await drawer.locator('input[type="file"]:not([capture])').setInputFiles(resolve('public/pms-demo/rooms',item.file));
      await expect(drawer.getByText('Ready to attach',{exact:true})).toBeVisible({timeout:120_000});
      await drawer.getByRole('button',{name:'Attach clean files to this record',exact:true}).click();
      await expect(drawer.getByText('Attached and locked',{exact:false}).first()).toBeVisible();
      drawer=await gallery(page,item.title);
      await expect(drawer.getByLabel('Bound private source')).toContainText(item.file);
      await drawer.getByLabel('Public alt text').fill(item.alt);
      const created=page.waitForResponse(r=>r.url().endsWith('/media-publications')&&r.request().method()==='POST');
      await drawer.getByRole('button',{name:'Create publication revision',exact:true}).click();
      const response=await created;expect(response.ok(),await response.text()).toBe(true);const publication=(await response.json()).data;
      const url=`/api/public/holdings/booking-test/media-publications/${publication.id}/content`;
      expect((await guest.request.get(url)).status()).toBe(404);
      const denied=page.waitForResponse(r=>r.url().endsWith(`/media-publications/${publication.id}/approve`));
      await drawer.getByRole('button',{name:'Approve',exact:true}).click();
      const own=await denied;expect(own.status()).toBe(403);expect((await own.json()).error.code).toBe('INDEPENDENT_PUBLICATION_REVIEW');
      const reviewDrawer=await gallery(review,item.title);
      await reviewDrawer.getByRole('button',{name:'Approve',exact:true}).click();
      await expect(reviewDrawer.getByText('Revision 1 · approved',{exact:true})).toBeVisible();
      const image=await guest.request.get(url);expect(image.status()).toBe(200);expect(image.headers()['content-type']).toBe('image/webp');expect(image.headers()['x-content-type-options']).toBe('nosniff');
      expect(image.headers()['cache-control']).toContain('no-store');
      publications.push({id:publication.id,url,alt:item.alt,title:item.title});
    }
    await page.goto(settings);await page.locator('summary:visible').filter({hasText:'Guest booking website · content and publishing'}).click();
    const content=page.getByRole('dialog',{name:'Guest booking website · content and publishing',exact:true});
    await content.locator(`input[name="hotel-images"][value="${publications[0].id}"]`).check();
    await content.locator(`input[name="images-${fixture.typeId}"][value="${publications[1].id}"]`).check();
    await content.getByRole('button',{name:'Preview and check readiness',exact:true}).click();
    await content.getByRole('button',{name:'Save booking website settings',exact:true}).click();
    await expect(content.getByRole('status')).toHaveText('Booking website settings saved');
    await publicPage.goto('/book/booking-test');
    await expect(publicPage.getByRole('img',{name:publications[0].alt,exact:true})).toBeVisible();
    const date=new Date();date.setUTCDate(date.getUTCDate()+14);const arrival=date.toISOString().slice(0,10);date.setUTCDate(date.getUTCDate()+2);
    await publicPage.getByLabel('Check-in',{exact:true}).fill(arrival);await publicPage.getByLabel('Check-out',{exact:true}).fill(date.toISOString().slice(0,10));
    await publicPage.getByRole('button',{name:'Search availability',exact:true}).click();
    await expect(publicPage.getByRole('img',{name:publications[1].alt,exact:true})).toBeVisible();
    expect(publicPage.viewportSize()).toEqual(testInfo.project.use.viewport);
    expect(await publicPage.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await publicPage.screenshot({path:testInfo.outputPath('reviewed-sample-galleries.png'),fullPage:true});
    for(const publication of publications){
      const drawer=await gallery(review,publication.title);await drawer.getByRole('button',{name:'Revoke',exact:true}).click();
      await drawer.getByLabel('Revocation reason').fill('Fictional pilot image withdrawn by independent reviewer');
      await drawer.getByRole('button',{name:'Confirm revocation',exact:true}).click();
      await expect(drawer.getByText('Revision 1 · revoked',{exact:true})).toBeVisible();
      expect((await guest.request.get(publication.url)).status()).toBe(404);
    }
    await publicPage.reload();
    for(const publication of publications)await expect(publicPage.getByRole('img',{name:publication.alt,exact:true})).toHaveCount(0);
    expect(await publicPage.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await publicPage.screenshot({path:testInfo.outputPath('revoked-sample-galleries.png'),fullPage:true});
    await page.goto(settings);await page.locator('summary:visible').filter({hasText:'Gallery'}).filter({hasText:'Isolated Family Hotel'}).click();
    await expect(page.getByRole('dialog',{name:'Gallery · Isolated Family Hotel',exact:true}).getByText('Revision 1 · revoked',{exact:true})).toBeVisible();
  }finally{
    const closed=await Promise.allSettled([author.close(),reviewer.close(),guest.close()]);
    const failure=closed.find(result=>result.status==='rejected');
    if(failure?.status==='rejected')throw failure.reason;
  }
});

test("cancels one of two booked rooms and preserves the other room and accepted price", async ({browser,page},testInfo)=>{
  test.skip(!isolated,"Requires an exact owned fictional booking cohort");
  const directory=resolve('.runtime/browser',process.env.ERPX_DEMO_RUN!),{fixture}=JSON.parse(await readFile(resolve(directory,'state.json'),'utf8'));
  const credentials=JSON.parse(await readFile(resolve(directory,'credentials.json'),'utf8')) as {password:string;users:Array<{userId:string;email:string;assignmentId:string}>};
  const origin=process.env.ERPX_E2E_BASE_URL!,staff=await createEngineeringBrowserContext(browser,testInfo,{baseURL:origin});
  try{
    const user=credentials.users.find(u=>u.userId===fixture.staff.authorId)!;
    const login=await staff.request.post('/api/auth/sign-in/email',{headers:{Origin:origin},data:{email:user.email,password:credentials.password}});expect(login.status()).toBe(200);
    await selectFixtureAssignment(staff,fixture.companyId,user.assignmentId,'booking-test');
    const staffPage=await staff.newPage();await staffPage.goto('/holdings/booking-test/home');
    const scope=await staffPage.locator('[data-work-revision]').first().evaluate(node=>({revision:(node as HTMLElement).dataset.workRevision!,company:(node as HTMLElement).dataset.workCompany!,assignment:(node as HTMLElement).dataset.workAssignment!}));
    expect(scope.company).toBe(fixture.companyId);expect(scope.assignment).toBe(user.assignmentId);
    const base='/api/v1/holdings/booking-test/pms',snapshotResponse=await staff.request.get(`${base}?property=${fixture.propertyId}`);expect(snapshotResponse.ok()).toBe(true);
    const snapshot=(await snapshotResponse.json()).data.snapshot;
    expect(snapshot.property.id).toBe(fixture.propertyId);
    const created=await staff.request.post(base,{headers:{Origin:origin,'Idempotency-Key':crypto.randomUUID(),'x-erpx-context-revision':scope.revision,'x-erpx-context-company':scope.company,'x-erpx-context-assignment':scope.assignment},data:{operation:'room.save',propertyId:fixture.propertyId,expectedVersion:snapshot.property.version,input:{number:'102',typeId:fixture.typeId,active:true}}});expect(created.ok(),await created.text()).toBe(true);
    const date=new Date();date.setUTCDate(date.getUTCDate()+21);const arrival=date.toISOString().slice(0,10);date.setUTCDate(date.getUTCDate()+2);const departure=date.toISOString().slice(0,10);
    await page.goto('/book/booking-test');await page.getByLabel('Check-in',{exact:true}).fill(arrival);await page.getByLabel('Check-out',{exact:true}).fill(departure);await page.getByLabel('Child ages per room').fill('7');await page.getByLabel('Rooms per selection').fill('2');
    await page.getByRole('button',{name:'Search availability',exact:true}).click();await expect(page.getByText('THB 14,124.00',{exact:true})).toBeVisible();
    await page.getByRole('button',{name:'Add rooms',exact:true}).click();await page.getByRole('button',{name:'Continue to guest details',exact:true}).click();
    await page.getByLabel('Full name').fill('Fictional two-room guest');await page.getByLabel('Email',{exact:true}).fill('two-room@example.test');await page.getByLabel('Phone',{exact:true}).fill('020000000');
    await page.getByRole('button',{name:'Review price & policy',exact:true}).click();await page.getByLabel('I have reviewed the dates, full price and cancellation terms.').check();await page.getByRole('button',{name:'Confirm reservation',exact:true}).click();
    await expect(page.getByRole('heading',{name:/Booking confirmed · PMS-/})).toBeVisible();
    const initialResponse=page.waitForResponse(r=>r.url().endsWith('/manage')&&r.request().method()==='POST');await page.locator('a[href*="/manage#token="]').click();
    const initial=(await(await initialResponse).json()).data;expect(initial.rooms).toHaveLength(2);expect(initial.total).toBe(1412400);expect(initial.dueAtHotel).toBe(1412400);
    const selection=page.getByRole('checkbox',{name:`Select Family room ${arrival}`,exact:true});await expect(selection).toHaveCount(2);await selection.nth(0).check();
    await page.getByRole('button',{name:'Cancel selected rooms',exact:true}).click();const drawer=page.getByRole('dialog',{name:'Review cancellation',exact:true});
    await drawer.getByRole('button',{name:'Review price and terms',exact:true}).click();await expect(drawer.getByText('Cancellation fee: THB 0.00',{exact:true})).toBeVisible();await expect(drawer.getByText('Refund to request')).toContainText('THB 0.00');
    await drawer.getByRole('button',{name:'Confirm changes',exact:true}).click();await expect(drawer).toBeHidden();await expect(selection.nth(0)).toBeDisabled();await expect(selection.nth(1)).toBeEnabled();
    const reloadResponse=page.waitForResponse(r=>r.url().endsWith('/manage')&&r.request().method()==='POST');await page.reload();const persisted=(await(await reloadResponse).json()).data;
    expect(persisted.total).toBe(706200);expect(persisted.dueAtHotel).toBe(706200);expect(persisted.refunds).toHaveLength(0);
    expect(persisted.rooms[0]).toEqual({...initial.rooms[0],state:'cancelled'});expect(persisted.rooms[1]).toEqual(initial.rooms[1]);
    await expect(selection.nth(0)).toBeDisabled();await expect(selection.nth(1)).toBeEnabled();expect(new URL(page.url()).hash).toBe('');expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
    await page.screenshot({path:testInfo.outputPath('partial-cancellation-persisted.png'),fullPage:true});
  }finally{await staff.close();}
});
