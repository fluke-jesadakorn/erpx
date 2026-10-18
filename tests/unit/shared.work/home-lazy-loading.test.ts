import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";
import { HomeDay } from "../../../src/components/work_projection/home/home-day";
import { HandoverLauncher, PlanLauncher } from "../../../src/components/work_projection/home/tool-launchers";
import { DeferredRegion } from "../../../src/components/work_projection/home/deferred-region";
import { createSocialPageQuery } from "../../../src/server/modules/collaboration/application/social-page";
vi.stubGlobal("React", React);
const imports = vi.hoisted(() => ({ day: vi.fn(), handover: vi.fn(), plan: vi.fn() }));
vi.mock("../../../src/components/people_payroll/self-service/home", () => { imports.day(); return { MyDayCard: () => "Eager day" }; });
vi.mock("../../../src/components/work_projection/today/handover-composer", () => { imports.handover(); return { HandoverComposer: () => "Eager handover" }; });
vi.mock("../../../src/components/work_projection/today/daily-plan", () => { imports.plan(); return { DailyPlan: () => "Eager plan" }; });
it("server renders launchers and reserved space without invoking lazy imports or reading secondary data", () => {
  const secondary = vi.fn(() => React.createElement("p", null, "Eager milestones"));
  const Region = DeferredRegion as React.ComponentType<Omit<React.ComponentProps<typeof DeferredRegion>, "children">>;
  const markup = renderToStaticMarkup(React.createElement(React.Fragment, null,
    React.createElement(HomeDay, { base: "/holdings/acme", enabled: true }),
    React.createElement(HandoverLauncher, { base: "/holdings/acme", onPublished: async () => {} }),
    React.createElement(PlanLauncher, { base: "/holdings/acme" }),
    React.createElement(Region, { label: "Recent work milestones" }, React.createElement(secondary))));
  expect(markup).toContain("My day"); expect(markup).toContain("Draft handover");
  expect(markup).toContain('data-active="false"'); expect(markup).toContain("min-height:260px");
  expect(markup).not.toContain("Eager"); expect(secondary).not.toHaveBeenCalled();
  for (const imported of Object.values(imports)) expect(imported).not.toHaveBeenCalled();
});
it("starts the first feed and audience read together while Home defers My day and milestones", async () => {
  const feed = vi.fn(async () => []), audience = vi.fn(async () => ({ branches: [], projects: [], audiences: [] }));
  const seed = vi.fn(), post = vi.fn();
  const query = createSocialPageQuery({ prepare: async () => {}, feed, audience, seed, post, moderate: async () => false });
  expect(await query({ beta: false }, { tag: "team", skipDay: true, skipMilestones: true })).toMatchObject({ posts: [], milestones: [] });
  expect(feed).toHaveBeenCalledExactlyOnceWith({ limit: 8, stream: "conversations", tag: "team" });
  expect(audience).toHaveBeenCalledTimes(1); expect(seed).not.toHaveBeenCalled(); expect(post).not.toHaveBeenCalled();
});
it('renders the exact included conversation shell with Apps recovery and no Work discovery or queue chrome',async()=>{
 const {SocialHomeShell}=await import('../../../src/components/collaboration/social-home-shell');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_ENVIRONMENT','local');
 try{const props:React.ComponentProps<typeof SocialHomeShell>={base:'/holdings/acme',name:'Actual colleague',single:true,sourceOnly:true,calendarEnabled:true,canRequestLegal:false,contextKey:'opaque-context',children:React.createElement('p',null,'Actual included post')};const html=renderToStaticMarkup(React.createElement(SocialHomeShell,props));expect(html).toContain('Actual included post');expect(html).toContain('Included conversation');expect(html).toContain('Back to Apps');expect(html).toContain('https://apps-dev-erpx.jesadakorn.com/sso/start');expect(html).not.toContain('Back to Today');expect(html).not.toContain('Needs you');expect(html).not.toContain('Draft handover');expect(html).not.toContain('/calendar');expect(html).not.toContain('opaque-context');}finally{vi.unstubAllEnvs();}
});
