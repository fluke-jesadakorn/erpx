import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {matchingRoleOptions} from "../../support/helpers/context-switching-browser";

test("workspace header keeps context, navigation and role actions usable across widths", async ({ page, context }, testInfo) => {
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Use an exact isolated Demo run; this test changes only its role/workspace preferences.");
  test.setTimeout(240_000);
  const credentials=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`),"utf8")) as {password:string;users:{email:string;role:string;roles?:string[]}[]};
  const user=credentials.users.find(u=>u.roles?.includes("hr_admin"));
  expect(user).toBeTruthy();
  const headers={Origin:(process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102")};
  const login=await context.request.post("/api/auth/sign-in/email",{headers,data:{email:user!.email,password:credentials.password}});
  expect(login.ok(),await login.text()).toBe(true);
  const activeEndpoint="/api/v1/holdings/simulation/assignments/active";
  const active=(await (await context.request.get(activeEndpoint)).json()).data;
  const hr=active.options.find((a:{role:string})=>a.role==="hr_admin");
  expect(hr).toBeTruthy();
  const selection=await context.request.post(activeEndpoint,{headers,data:{assignmentId:hr.id,revision:active.revision}});
  expect(selection.ok(),await selection.text()).toBe(true);
  const path="/holdings/simulation/people";
  const chrome=page.locator("header[data-app-chrome]");
  const ready=async()=>{
    await expect(chrome.getByRole("button",{name:"Switch workspace",exact:true})).toBeEnabled();
    await expect(chrome.getByRole("button",{name:/Account menu for/})).toBeEnabled();
    await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale","false");
  };
  const observations:unknown[]=[];
  async function geometry(target:Page){
    return target.locator("header[data-app-chrome]").evaluate(header=>{
      const bounds=header.getBoundingClientRect();
      const elements=[...header.querySelectorAll('button,nav[aria-label="Breadcrumb"] [aria-current="page"],.active-work-role')]
        .filter(element=>element.getClientRects().length>0);
      const boxes=elements.map(element=>({label:element.getAttribute("aria-label")??element.textContent?.trim()??"",...element.getBoundingClientRect().toJSON(),button:element.tagName==="BUTTON",hit:element.contains(document.elementFromPoint(element.getBoundingClientRect().x+element.getBoundingClientRect().width/2,element.getBoundingClientRect().y+element.getBoundingClientRect().height/2))}));
      const controls=boxes.filter(box=>box.button);
      return {width:innerWidth,header:bounds.toJSON(),boxes,
        outside:boxes.filter(box=>box.left<bounds.left-1||box.right>bounds.right+1||box.top<bounds.top-1||box.bottom>bounds.bottom+1).map(box=>box.label),
        small:controls.filter(box=>box.width<43.5||box.height<43.5).map(box=>box.label),
        obstructed:controls.filter(box=>!box.hit).map(box=>box.label),
        overlaps:controls.flatMap((a,i)=>controls.slice(i+1).filter(b=>Math.min(a.right,b.right)-Math.max(a.left,b.left)>1&&Math.min(a.bottom,b.bottom)-Math.max(a.top,b.top)>1).map(b=>[a.label,b.label]))};
    });
  }
  for(const theme of ["light","dark"]){
    await context.addCookies([{name:"erpx_appearance",value:theme,url:(process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102")}]);
    await page.goto(path);await ready();
    await expect(page.locator("html")).toHaveAttribute("data-theme",theme);
    for(const width of [320,393,650,651,768,1000,1001,1100,1440]){
      await page.setViewportSize({width,height:900});
      const result=await geometry(page);observations.push({theme,...result});
      expect(result.header.right,`${theme} ${width}px header within viewport`).toBeLessThanOrEqual(width+1);
      expect(result.outside,`${theme} ${width}px context within header`).toEqual([]);
      expect(result.small,`${theme} ${width}px touch targets`).toEqual([]);
      expect(result.obstructed,`${theme} ${width}px hit targets`).toEqual([]);
      expect(result.overlaps,`${theme} ${width}px controls`).toEqual([]);
      if([320,768,1440].includes(width))await page.screenshot({path:testInfo.outputPath(`header-${theme}-${width}.png`)});
    }
  }
  await page.setViewportSize({width:320,height:900});
  const breadcrumb=chrome.getByRole("button",{name:"Show breadcrumb path",exact:true});
  await breadcrumb.click();await expect(chrome.getByRole("group",{name:"Breadcrumb path"})).toBeVisible();
  await page.keyboard.press("Escape");await expect(breadcrumb).toBeFocused();
  await chrome.getByRole("button",{name:"Search workspace",exact:true}).click();
  const search=page.getByRole("dialog",{name:"Go to…",exact:true});
  await expect(search).toBeVisible();await search.getByRole("textbox",{name:"Search workspace destinations"}).fill("Today");
  const destination=search.locator('a[href="/holdings/simulation/home"]');
  await expect(destination).toHaveAccessibleName(/Today/);
  await destination.click();await expect(search).not.toBeVisible();
  await expect(page).toHaveURL(/\/holdings\/simulation\/home$/);
  await ready();
  const switcher=chrome.getByRole("button",{name:"Switch workspace",exact:true});
  await switcher.click();
  const workspaces=page.getByRole("dialog",{name:"Switch workspace",exact:true});
  await workspaces.getByRole("link",{name:"Branch",exact:true}).click();
  await expect(page).toHaveURL(/\/branches\//);await ready();
  await expect(chrome.getByRole("button",{name:"Switch workspace",exact:true})).toHaveAttribute("title",/currently Isolated browser simulation · Branch/);
  await chrome.getByRole("button",{name:"Switch workspace",exact:true}).click();
  await page.getByRole("dialog",{name:"Switch workspace",exact:true}).getByRole("button",{name:/Isolated browser simulation/}).click();
  await expect(page).not.toHaveURL(/\/branches\//);await ready();
  await chrome.getByRole("button",{name:/Account menu for/}).click();
  const treasuryRoles=await matchingRoleOptions(page,"treasury");
  await treasuryRoles.click();
  await ready();await expect(chrome.locator(".active-work-role")).toContainText("treasury");
  await page.reload();await ready();await expect(chrome.locator(".active-work-role")).toContainText("treasury");
  await chrome.getByRole("button",{name:/Account menu for/}).click();
  const hrRoles=await matchingRoleOptions(page,"hr admin");
  await hrRoles.click();
  await ready();await expect(chrome.locator(".active-work-role")).toContainText("hr admin");
  // Company changes require this account's independently published administrator role.
  await chrome.getByRole("button",{name:/Account menu for/}).click();
  const ownerRoles=await matchingRoleOptions(page,"holding owner");
  await ownerRoles.click();
  await ready();await expect(chrome.locator(".active-work-role")).toContainText("holding owner");
  await page.goto("/holdings/simulation/people/organization?changeState=all#organization-change-queue");await ready();
  const detail=page.getByTestId("people-detail");
  await expect(detail).toBeVisible();
  const queue=detail.locator("#organization-change-queue");
  await expect(queue).toBeVisible();
  const queueTop=await queue.evaluate(element=>element.getBoundingClientRect().top);
  const detailHeaderBottom=await detail.getByRole("button",{name:"Close details",exact:true}).locator("..").evaluate(element=>element.getBoundingClientRect().bottom);
  expect(queueTop,"linked queue stays below the current Company detail header").toBeGreaterThanOrEqual(detailHeaderBottom);
  expect(queueTop,"linked queue is visible in the current viewport").toBeLessThan(900);
  await testInfo.attach("workspace-header-result",{contentType:"application/json",body:JSON.stringify({run,observations,interactions:{breadcrumbEscape:true,search:true,branchAndCompanySwitch:true,roleSwitchAndReload:true,linkedQueueVisible:true}},null,2)});
});
