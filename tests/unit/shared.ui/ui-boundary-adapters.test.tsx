import {routeDependencies,routeClosureIds} from "../../../scripts/ui/inventory.mjs";
import { createElement, createRef, type ComponentProps } from "react";
import type NextLink from "next/link";
import type { Dialog } from "@erpx/ui/components/dialog";
import { ProductNavigationBoundary } from "@erpx/workplace/components/product-navigation-boundary";
import { CalendarDraftBoundary } from "@erpx/feature-work_projection/components/calendar/calendar-draft-boundary";
import { renderToStaticMarkup } from "react-dom/server";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { afterEach, expect, it, vi } from "vitest";
import WorkspaceLink from "@erpx/workplace/components/workspace-link";
import { ProductLinkEnvironment, ProductWorkLinks } from "@erpx/workplace/components/product-link-context";
import { DestinationIndexView } from "@erpx/workplace/components/destination-index-view";
import { scopeFor } from "../../../scripts/ui/standard-scope.mjs";
import { Table } from "@erpx/ui/components/table";
import { ListFilters } from "@erpx/ui/components/list-filters";
import { SetupChecklist } from "@erpx/ui/components/setup-checklist";
import { UiNavigationProvider } from "@erpx/ui/components/navigation-context";
import { RecordLink, RecordScopeProvider } from "@erpx/feature-records/public/client";
import { registerUnsavedChanges } from "@erpx/ui/lib/unsaved-changes";
import { hasUnsavedWork } from "@erpx/feature-work_projection/lib/context-switch";

const roots: string[] = [];
function expandedRouteClosure(manifest: { closurePaths: string[] }, route: { closureIds?: number[]; entryIds?:number[] }) {
  return routeClosureIds(manifest,route).map((index:number) => manifest.closurePaths[index]);
}
const observedLinks = vi.hoisted(() => ({ props: [] as ComponentProps<typeof NextLink>[] }));
const observedNavigation = vi.hoisted(() => ({ dialogs: [] as ComponentProps<typeof Dialog>[], refresh: vi.fn(), pathname: "/holdings/acme/finance/documents/invoices" }));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: observedNavigation.refresh }),
  usePathname: () => observedNavigation.pathname,
  useSearchParams: () => new URLSearchParams("tag=one&tag=two"),
}));
vi.mock("@erpx/ui/components/dialog", async importOriginal => {
  const actual = await importOriginal<typeof import("@erpx/ui/components/dialog")>();
  const { createElement } = await import("react");
  return { ...actual, Dialog: function ObservedDialog(props: ComponentProps<typeof actual.Dialog>) {
    observedNavigation.dialogs.push(props);
    return createElement(actual.Dialog, props);
  } };
});
vi.mock("next/link", async importOriginal => {
  const actual = await importOriginal<typeof import("next/link")>();
  const { createElement } = await import("react");
  return { ...actual, default: function ObservedNextLink(props: ComponentProps<typeof actual.default>) {
    observedLinks.props.push(props);
    return createElement(actual.default, props);
  } };
});
afterEach(() => {
  observedLinks.props.length = 0;
  observedNavigation.dialogs.length = 0;
  observedNavigation.refresh.mockClear();
  observedNavigation.pathname = "/holdings/acme/finance/documents/invoices";
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

it("keeps owner-connected record row actions and empty-row column spans after injecting navigation into primitives", () => {
  const html = renderToStaticMarkup(<ProductLinkEnvironment audience="finance">
    <RecordScopeProvider base="/holdings/acme">
      <Table><thead><tr><th>Record</th><th>Status</th></tr></thead><tbody>
        <tr><td><RecordLink href="/holdings/acme/finance/invoices/fixture-invoice">Fictional invoice</RecordLink></td><td>Draft</td></tr>
        <tr><td colSpan={2}>No other records</td></tr>
      </tbody></Table>
    </RecordScopeProvider>
  </ProductLinkEnvironment>);
  expect(html).toContain('data-slot="table"');
  expect(html).toContain('class="ui-record-open-action"');
  expect(html).toContain('class="sr-only">Details</span>');
  expect(html).toMatch(/col[Ss]pan="3"/);
  const hrefs = [...html.matchAll(/href="([^"]+)"/g)].map(match => new URL(match[1].replaceAll("&amp;", "&"), "https://fixture.invalid"));
  expect(hrefs).toHaveLength(2);
  for (const href of hrefs) {
    expect(href.pathname).toBe("/holdings/acme/finance/documents/invoices/fixture-invoice/overview");
    const back = new URL(href.searchParams.get("returnTo")!, "https://fixture.invalid");
    expect(back.searchParams.getAll("tag")).toEqual(["one", "two"]);
  }
});

it("keeps owned filter actions and repeated retained values through shell-provided filter navigation", () => {
  observedNavigation.pathname = "/holdings/acme/people-organization/people/directory/current";
  const html = renderToStaticMarkup(<ProductLinkEnvironment audience="people">
    <ListFilters appearance="standard" action="/holdings/acme/people" query="Fictional" preserve={{status:"current",tag:["one","two"]}}/>
  </ProductLinkEnvironment>);
  expect(html).toContain('action="/holdings/acme/people-organization/people/directory/current"');
  expect(html.match(/name="tag"/g)).toHaveLength(2);
  expect(html).not.toContain('name="status"');
  const href = /href="([^"]+)"/.exec(html)?.[1];
  expect(href).toBeDefined();
  expect(new URL(href!.replaceAll("&amp;", "&"), "https://fixture.invalid").searchParams.getAll("tag")).toEqual(["one","two"]);
});

it("renders checklist owner labels and a supplied link without reading a Front shell language", () => {
  const Link = ({href,children}:ComponentProps<typeof NextLink>) => <a href={String(href)} data-owner-link>{children}</a>;
  const html = renderToStaticMarkup(<UiNavigationProvider bindings={{Link}}>
    <SetupChecklist title="ความพร้อม" labels={{remaining:count=>`เตรียม ${count} รายการ`,setup:"ตั้งค่า"}} items={[{id:"setup",label:"ตัวอย่าง",ready:false,owner:"ผู้ดูแล",href:"/fixture/setup"}]}/>
  </UiNavigationProvider>);
  expect(html).toContain('class="card setup-checklist"');
  expect(html).toContain("เตรียม 1 รายการ");
  expect(html).toContain('href="/fixture/setup" data-owner-link="true">ตั้งค่า</a>');
});

it("shares primitive dirty registrations with the existing workspace navigation guard", () => {
  vi.stubGlobal("document", {querySelector:()=>null});
  let dirty = true;
  const unregister = registerUnsavedChanges(() => dirty);
  try {
    expect(hasUnsavedWork()).toBe(true);
    dirty = false;
    expect(hasUnsavedWork()).toBe(false);
    dirty = true;
    unregister();
    expect(hasUnsavedWork()).toBe(false);
  } finally { unregister(); vi.unstubAllGlobals(); }
});

it("renders a single real Next anchor with canonical scoped destination and the caller's content and presentation", () => {
  const html = renderToStaticMarkup(createElement(WorkspaceLink, {
    href: "/holdings/acme/branches/main/departments/desk/people?view=internal&status=current&q=Anan#work",
    className: "retained-specialized-link", "aria-label": "Open colleague", target: "_blank", rel: "noopener",
  }, createElement("strong", { "data-caller": "retained" }, "Anan")));
  expect(html).toContain('href="/holdings/acme/branches/main/departments/desk/people-organization/people/directory/current?q=Anan#work"');
  expect(html).toContain('class="retained-specialized-link"');
  expect(html).toContain('aria-label="Open colleague"');
  expect(html).toContain('target="_blank"'); expect(html).toContain('rel="noopener"');
  expect(html).toContain('<strong data-caller="retained">Anan</strong>');
  expect(html.match(/<a\b/g)).toHaveLength(1); expect(html).not.toMatch(/<div|style=|ui-record-item|data-appearance/);
});

it("preserves Next navigation options, ref, events and object hrefs while disabling speculative prefetch by default", () => {
  const href = { pathname: "/holdings/acme/pms/rooms", query: { property: "hotel-1", q: "Room 101" }, hash: "selected" };
  const ref = createRef<HTMLAnchorElement>(), onClick = () => {}, onNavigate = () => {};
  renderToStaticMarkup(createElement(WorkspaceLink, { href, ref, onClick, onNavigate, replace: true, scroll: false }, "Rooms"));
  expect(observedLinks.props).toHaveLength(1);
  const result = observedLinks.props[0];
  expect(result.href).toBe(href); expect(result.ref).toBe(ref);
  expect(result.onClick).toBe(onClick); expect(result.onNavigate).toBe(onNavigate);
  expect(result.replace).toBe(true); expect(result.scroll).toBe(false); expect(result.prefetch).toBe(false);
  renderToStaticMarkup(createElement(WorkspaceLink, { href, prefetch: true }));
  expect(observedLinks.props[1].prefetch).toBe(true);
});

it("retains unchanged product URL objects while mapping actual local Front rewrites and cross-product handoffs", () => {
  const sameProduct = { pathname: "/holdings/acme/services/assets/register", query: { q: "Current assets" } };
  const localFront = { pathname: "/holdings/acme/pos", query: { tab: "history", sale: "sale-1" }, hash: "evidence" };
  const finance = { pathname: "/holdings/acme/finance/invoices/invoice-1", query: { status: ["paid", "posted"] }, hash: "workflow-owners" };
  const render = (audience: "assets" | "pos", href: ComponentProps<typeof WorkspaceLink>["href"]) =>
    renderToStaticMarkup(<ProductLinkEnvironment audience={audience}>
      <ProductWorkLinks companyId="company-two" companySlug="acme-co" workspaceBase="/holdings/acme">
        <WorkspaceLink href={href} prefetch>Record</WorkspaceLink>
      </ProductWorkLinks>
    </ProductLinkEnvironment>);
  render("assets", sameProduct);
  expect(observedLinks.props[0].href).toBe(sameProduct);
  expect(observedLinks.props[0].prefetch).toBe(true);
  render("pos", localFront);
  expect(observedLinks.props[1].href).toBe("/acme/acme-co/holding/history/receipts/sale-1/overview#evidence");
  render("pos", finance);
  const href = observedLinks.props[2].href;
  expect(typeof href).toBe("string");
  if (typeof href !== "string") throw new Error("The cross-product link must use its registry launch URL.");
  const sso = new URL(href), launch = new URL(sso.searchParams.get("next")!, "https://launch.invalid");
  expect(sso.origin).toBe("https://finance-dev-erpx.jesadakorn.com");
  expect(sso.pathname).toBe("/sso/start");
  expect(launch.searchParams.get("companyId")).toBe("company-two");
  expect(launch.searchParams.get("next")).toBe("/holdings/acme/finance/documents/invoices/invoice-1/overview?status=paid&status=posted#workflow-owners");
  expect(observedLinks.props[2].prefetch).toBe(false);
});

it.each(["legacy", "standard"] as const)("renders the %s index with the same links and an explicit presentation boundary", appearance => {
  const html = renderToStaticMarkup(createElement(DestinationIndexView, { appearance, title: "Hotel PMS", links: [
    { label: "Rooms", href: "/holdings/acme/pms/rooms?property=hotel-1#selected" },
    { label: "Reports", href: "/holdings/acme/pms/reports?property=hotel-1" },
  ] }));
  expect(html).toContain(`data-appearance="${appearance}"`); expect(html).toContain("Hotel PMS");
  expect(html).toContain('aria-label="Available pages"'); expect(html.match(/<a\b/g)).toHaveLength(2);
  expect(html).toContain('href="/holdings/acme/pms/rooms?property=hotel-1#selected"');
  expect(html).toContain('href="/holdings/acme/pms/reports?property=hotel-1"');
  if (appearance === "standard") { expect(html).toContain("ui-page-header"); expect(html).toContain("ui-record-item"); }
  else { expect(html).not.toMatch(/ui-page-header|ui-record-item|class="ui-stack"/); expect(html).toContain('class="content-stack"'); }
});

it("preserves the owner-filtered empty state without manufacturing a visible destination", () => {
  for (const appearance of ["legacy", "standard"] as const) {
    const html = renderToStaticMarkup(createElement(DestinationIndexView, { appearance, title: "Rooms", links: [] }));
    expect(html).toContain("No pages are available for your selected work role."); expect(html).not.toContain("<a");
  }
});

it("keeps extracted hospitality presenters in their original boundary and requires adapters for other ordinary descendants", () => {
  for (const owner of ["pms/[[...section]]", "pms/import", "pos", "hospitality/setup"])
    expect(scopeFor(`packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/${owner}/screen.tsx`)).toBe("deferred-pms-pos");
  expect(scopeFor("packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/finance/screen.tsx")).toBe("standardize");
  expect(scopeFor("packages/workplace/src/components/workspace-link.tsx")).toBe("shared-adapter");
  expect(scopeFor("packages/workplace/src/components/destination-index-view.tsx")).toBe("shared-adapter");

  const root = mkdtempSync(join(tmpdir(), "erpx-boundary-adapters-")); roots.push(root);
  for (const directory of ["scripts/ui", "docs/ux-ui", "packages/feature-pms/src/components", "packages/workplace/src/components", "packages/feature-ordinary/src/components", "node_modules"])
    mkdirSync(join(root, directory), { recursive: true });
  for (const file of ["coverage.mjs", "route-topology.mjs", "standard-scope.mjs", "shared-consumers.json", "inventory.mjs"])
    copyFileSync(resolve("scripts/ui", file), join(root, "scripts/ui", file));
  symlinkSync(resolve("node_modules/typescript"), join(root, "node_modules/typescript"), "dir");
  writeFileSync(join(root, "packages/feature-pms/src/components/owner.tsx"), 'import Link from "@erpx/workplace/components/workspace-link"; export const Owner=()=> <Link/>;');
  writeFileSync(join(root, "packages/workplace/src/components/workspace-link.tsx"), 'import {Panel} from "@erpx/feature-ordinary/components/panel"; export default function Link(){return <Panel/>;}');
  writeFileSync(join(root, "packages/feature-ordinary/src/components/panel.tsx"), 'export const Panel=()=> <section/>;');
  const result = spawnSync(process.execPath, [join(root, "scripts/ui/coverage.mjs")], { cwd: root, encoding: "utf8" });
  expect(result.status, result.stdout + result.stderr).toBe(0);
  const manifest = JSON.parse(readFileSync(join(root, "docs/ux-ui/standard-coverage.json"), "utf8"));
  expect(manifest.boundaryConsumers).toEqual([{ owner: "packages/feature-pms/src/components/owner.tsx", component: "packages/feature-ordinary/src/components/panel.tsx",
    reason: "An ordinary migrated component is reachable from an excluded owner; add an explicit legacy adapter or document a narrower boundary." }]);
});


it.each([
  ["legacy", "en"], ["legacy", "th"], ["standard", "en"], ["standard", "th"],
] as const)("keeps the real %s navigation confirmation and buttons in %s", (appearance, language) => {
  const html = renderToStaticMarkup(<ProductNavigationBoundary appearance={appearance} language={language}>
    <a href="/retained-target" data-product-switch>Current destination</a>
  </ProductNavigationBoundary>);
  expect(html).toContain('href="/retained-target"');
  expect(html.match(/<a\b/g)).toHaveLength(1);
  expect(observedNavigation.dialogs).toHaveLength(1);
  const dialog = observedNavigation.dialogs[0];
  expect(dialog.appearance).toBe(appearance);
  expect(dialog.open).toBe(false);
  expect(dialog.title).toBe(language === "th" ? "ออกจากหน้าที่มีงานค้าง?" : "Leave unsaved work?");
  expect(dialog.finalFocus?.()).toBe(null);
  // The real controlled Dialog stays closed on SSR. Render its captured children
  // separately to exercise the real Button and Base UI implementation, not a hook mock.
  const controls = renderToStaticMarkup(dialog.children);
  expect(controls.match(/<button\b/g)).toHaveLength(2);
  expect(controls.match(new RegExp(`data-appearance="${appearance}"`, "g"))).toHaveLength(2);
  expect(controls).toContain('data-autofocus="true"');
  expect(controls).toContain(language === "th" ? "แก้ไขต่อ" : "Keep editing");
  expect(controls).toContain(language === "th" ? "ทิ้งข้อมูลและออกจากหน้า" : "Discard and leave");
  expect(controls.includes("ui-standard-control")).toBe(appearance === "standard");
  expect(observedNavigation.refresh).not.toHaveBeenCalled();
});

it("keeps Office, Account and Admin confirmations standard by default without a mount read", () => {
  const html = renderToStaticMarkup(<ProductNavigationBoundary>Owner server snapshot</ProductNavigationBoundary>);
  expect(html).toContain("Owner server snapshot");
  expect(observedNavigation.dialogs).toHaveLength(1);
  expect(observedNavigation.dialogs[0].appearance).toBe("standard");
  expect(observedNavigation.dialogs[0].title).toBe("Leave unsaved work?");
  expect(observedNavigation.dialogs[0].open).toBe(false);
  expect(observedNavigation.refresh).not.toHaveBeenCalled();
});

it("keeps specialized Calendar server content and the original legacy confirmation controls", () => {
  const child = createElement("section", { className: "retained-calendar-grid", "data-width": "640" }, "Existing Calendar");
  for (const dirty of [false, true]) {
    observedNavigation.dialogs.length = 0;
    expect(renderToStaticMarkup(<CalendarDraftBoundary dirty={dirty}>{child}</CalendarDraftBoundary>)).toBe(renderToStaticMarkup(child));
    expect(observedNavigation.dialogs).toHaveLength(1);
    const dialog = observedNavigation.dialogs[0];
    expect(dialog.open).toBe(false);
    expect(dialog.appearance ?? "legacy").toBe("legacy");
    const controls = renderToStaticMarkup(dialog.children);
    expect(controls.match(/<button\b/g)).toHaveLength(2);
    expect(controls.match(/data-appearance="legacy"/g)).toHaveLength(2);
    expect(controls).toContain("Keep editing");
    expect(controls).toContain("Discard and leave");
    expect(controls).not.toContain("ui-standard-control");
  }
});

it('the actual canonical POS parent selects the standard index without replacing its native scoped reader or link values',async()=>{
 const {createRequire}=await import('node:module'),require=createRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),source=readFileSync(resolve("packages/feature-pos/src/routes/holdings/[holdingSlug]/pos/page.tsx"),'utf8'),compiled=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, jsx:ts.JsxEmit.ReactJSX,module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,loaded={exports:{} as Record<string,unknown>},nativeLinks=[{label:'Register settings',href:'/holdings/acme/pos/settings?register=original#readiness'},{label:'Receipts',href:'/holdings/acme/pos/history?q=Original&status=paid'}],native=vi.fn(async(props:{appearance:'standard'|'legacy';holdingSlug:string;path:string})=>createElement(DestinationIndexView,{appearance:props.appearance,title:'POS',links:nativeLinks}));
 new Function('require','module','exports',compiled)((name:string):unknown=>{if(name==='react/jsx-runtime')return require(name);if(name.endsWith('/destination-index'))return {DestinationIndex:native};throw Error('Unexpected actual POS parent import '+name);},loaded,loaded.exports);
 const Page=loaded.exports.default as(props:{params:Promise<{holdingSlug:string}>})=>Promise<import('react').ReactElement<{appearance:'standard'|'legacy';holdingSlug:string;path:string}>>,tree=await Page({params:Promise.resolve({holdingSlug:'acme'})});expect(tree.type).toBe(native);expect(tree.props).toEqual({appearance:'standard',holdingSlug:'acme',path:'pos'});expect(native).not.toHaveBeenCalled();const html=renderToStaticMarkup(await native(tree.props));expect(native).toHaveBeenCalledExactlyOnceWith(tree.props);expect(html).toContain('data-appearance="standard"');expect(html).toContain('ui-record-item');expect(html).toContain('href="/holdings/acme/pos/settings?register=original#readiness"');expect(html).toContain('href="/holdings/acme/pos/history?q=Original&amp;status=paid"');
});

// Actual shared confirmation preserves the specialized default while the real
// standard Calendar caller opts in; native pending controls still refuse discard.
it.each([{appearance:"legacy",held:false},{appearance:"standard",held:false},{appearance:"legacy",held:true},{appearance:"standard",held:true}] as const)("renders actual Calendar $appearance confirmation with held=$held without losing native guards",({appearance,held})=>{
  const child=createElement("section",{"data-calendar-owner":"retained"},"Current Calendar content");
  expect(renderToStaticMarkup(<CalendarDraftBoundary appearance={appearance} dirty busy={held} pending={held}>{child}</CalendarDraftBoundary>)).toBe(renderToStaticMarkup(child));
  expect(observedNavigation.dialogs).toHaveLength(1);const dialog=observedNavigation.dialogs[0];expect(dialog.appearance).toBe(appearance);expect(dialog.open).toBe(false);
  const controls=renderToStaticMarkup(dialog.children);expect(controls.match(/<button\b/g)).toHaveLength(held?1:2);expect(controls.match(new RegExp(`data-appearance="${appearance}"`,"g"))).toHaveLength(held?1:2);expect(controls).toContain("Keep editing");expect(controls.includes("Discard and leave")).toBe(!held);expect(controls.includes("ui-standard-control")).toBe(appearance==="standard");expect(observedNavigation.refresh).not.toHaveBeenCalled();
});


it("maps all actual named-slot sources to Next public paths without collapsing their source identities", async () => {
  const { appPageContext } = await import("../../../scripts/ui/route-topology.mjs");
  const { readdirSync } = await import("node:fs"), { createRequire } = await import("node:module");
  const require = createRequire(import.meta.url);
  const { normalizeAppPath } = require("next/dist/shared/lib/router/utils/app-paths") as { normalizeAppPath: (source: string) => string };
  const pages = (directory: string): string[] => readdirSync(directory, { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? pages(`${directory}/${entry.name}`) : entry.name === "page.tsx" ? [`${directory}/${entry.name}`] : []);
  const sources = pages("apps/compat/src/app"), slots = sources.filter(source => source.split("/").some(segment => segment.startsWith("@")));
  expect(slots).toHaveLength(295);
  const identities = new Set<string>();
  for (const source of slots) {
    const context = appPageContext(source);
    expect(context).not.toBeNull(); if (!context) throw new Error("The actual named-slot page was omitted.");
    expect(context.route).toBe(normalizeAppPath(source.slice("apps/compat/src/app".length).replace(/\.tsx$/, "")));
    expect(context.route).not.toContain("/@");
    expect(context.routeContext.kind).toBe("parallel-slot");
    expect(context.routeContext.slots.map(slot => slot.name)).toEqual(["record"]);
    identities.add(source);
  }
  expect(identities.size).toBe(slots.length);
  expect(appPageContext("packages/feature-finance/src/routes/holdings/[holdingSlug]/@record/finance/documents/expenses/[id]/page.tsx")).toEqual({
    route: "/holdings/[holdingSlug]/finance/documents/expenses/[id]",
    routeContext: { kind: "parallel-slot", slots: [{ name: "record", source: "packages/feature-finance/src/routes/holdings/[holdingSlug]/@record",
      parentSource: "packages/feature-finance/src/routes/holdings/[holdingSlug]", parentRoute: "/holdings/[holdingSlug]" }] },
  });
});

it("keeps nested slot and route-group ownership explicit and does not expose private implementation pages", async () => {
  const { appPageContext } = await import("../../../scripts/ui/route-topology.mjs");
  expect(appPageContext("packages/workplace/src/routes/page.tsx")).toEqual({ route: "/", routeContext: { kind: "children", slots: [] } });
  expect(appPageContext("src/app/(workspace)/dashboard/@team/(members)/@detail/[id]/page.tsx")).toEqual({
    route: "/dashboard/[id]", routeContext: { kind: "parallel-slot", slots: [
      { name: "team", source: "src/app/(workspace)/dashboard/@team", parentSource: "src/app/(workspace)/dashboard", parentRoute: "/dashboard" },
      { name: "detail", source: "src/app/(workspace)/dashboard/@team/(members)/@detail",
        parentSource: "src/app/(workspace)/dashboard/@team/(members)", parentRoute: "/dashboard" },
    ] },
  });
  expect(appPageContext("src/app/_private/page.tsx")).toBeNull();
  expect(appPageContext("src/app/work/_presenters/nested/page.tsx")).toBeNull();
  expect(appPageContext("src/components/page.tsx")).toBeNull();
  expect(appPageContext("src/app/%5Fpublic/page.tsx")?.route).toBe("/_public");
  expect(() => appPageContext("src/app/feed/@modal/(..)photo/[id]/page.tsx")).toThrow("UI_INTERCEPTED_ROUTE_CONTEXT_REQUIRED");
});

it("finds real ancestor and slot fallback boundaries without promoting private or nested global convention lookalikes", async () => {
  const { appPageBoundaries } = await import("../../../scripts/ui/route-topology.mjs");
  const source = "src/app/holdings/[holdingSlug]/@record/finance/[id]/page.tsx";
  const available = [source, "src/app/layout.tsx", "src/app/global-error.tsx", "src/app/forbidden.tsx",
    "src/app/holdings/forbidden.tsx", "src/app/holdings/[holdingSlug]/layout.tsx", "src/app/holdings/[holdingSlug]/unauthorized.tsx",
    "src/app/holdings/[holdingSlug]/@record/error.tsx", "src/app/holdings/[holdingSlug]/@record/default.tsx",
    "src/app/holdings/[holdingSlug]/default.tsx", "src/app/holdings/[holdingSlug]/@other/default.tsx",
    "src/app/holdings/[holdingSlug]/_presenters/forbidden.tsx", "src/app/unrelated/forbidden.tsx",
    "src/app/holdings/global-error.tsx", "src/app/global-not-found.tsx"];
  expect(appPageBoundaries(source, available)).toEqual([
    "src/app/forbidden.tsx", "src/app/global-error.tsx", "src/app/holdings/[holdingSlug]/@other/default.tsx",
    "src/app/holdings/[holdingSlug]/@record/default.tsx", "src/app/holdings/[holdingSlug]/@record/error.tsx",
    "src/app/holdings/[holdingSlug]/default.tsx", "src/app/holdings/[holdingSlug]/layout.tsx",
    "src/app/holdings/[holdingSlug]/unauthorized.tsx", "src/app/holdings/forbidden.tsx", "src/app/layout.tsx",
  ].sort());
});

it("generates both canonical and drawer cases at their public URL and fingerprints actual Next boundary imports", () => {
  const root = mkdtempSync(join(tmpdir(), "erpx-route-topology-")); roots.push(root);
  for (const directory of ["scripts/ui", "docs/ux-ui", "node_modules"]) mkdirSync(join(root, directory), { recursive: true });
  for (const file of ["coverage.mjs", "route-topology.mjs", "standard-scope.mjs", "shared-consumers.json", "inventory.mjs"])
    copyFileSync(resolve("scripts/ui", file), join(root, "scripts/ui", file));
  symlinkSync(resolve("node_modules/typescript"), join(root, "node_modules/typescript"), "dir");
  const holding = "src/app/(workspace)/holdings/[holdingSlug]", canonical = `${holding}/finance/documents/[id]/page.tsx`,
    drawer = `${holding}/@record/finance/documents/[id]/page.tsx`, fallback = `${holding}/@record/default.tsx`;
  const fixture: Record<string, string> = {
    "src/app/page.tsx": "export default function Page(){return <main/>;}",
    [canonical]: "export default function Document(){return <main/>;}",
    [drawer]: "export default function Record(){return <dialog/>;}",
    [`${holding}/@record/[...path]/page.tsx`]: "export default function ClearRecord(){return null;}",
    [`${holding}/_presenters/page.tsx`]: "export default function Private(){return <aside/>;}",
    "src/app/layout.tsx": "import './root.css';export default function Layout({children}){return <html><body>{children}</body></html>;}",
    "src/app/root.css": "body{margin:0}",
    "src/app/global-error.tsx": "import {GlobalState} from './global-state';export default GlobalState;",
    "src/app/global-state.tsx": "import './global-state.css';export function GlobalState(){return <main/>;}",
    "src/app/global-state.css": ".global-state{color:CanvasText}",
    "src/app/(workspace)/holdings/forbidden.tsx": "import {Denied} from './denied';export default Denied;",
    "src/app/(workspace)/holdings/denied.tsx": "import './denied.css';export function Denied(){return <main/>;}",
    "src/app/(workspace)/holdings/denied.css": ".denied{color:CanvasText}",
    [`${holding}/layout.tsx`]: "export default function Layout({children,record}){return <>{children}{record}</>;}",
    [`${holding}/@record/error.tsx`]: "export default function RecordError(){return <aside/>;}",
    [fallback]: "export default function RecordDefault(){return null;}",
    [`${holding}/_presenters/forbidden.tsx`]: "import './private.css';export default function PrivateDenied(){return <aside/>;}",
    [`${holding}/_presenters/private.css`]: ".private{color:red}",
  };
  for (const [source, contents] of Object.entries(fixture)) {
    mkdirSync(join(root, source.slice(0, source.lastIndexOf("/"))), { recursive: true }); writeFileSync(join(root, source), contents);
  }
  type Route = { source: string; route: string; fingerprint: string; closureIds: number[]; dependencies: string[]; styles: string[];
    boundaries: string[]; routeContext: { kind: string; slots: { name: string; parentRoute: string }[] } };
  const generate = (): { summary: { routes: number }; closurePaths: string[]; routes: Route[]; sources: { source: string; verification: string }[] } => {
    const result = spawnSync(process.execPath, [join(root, "scripts/ui/coverage.mjs")], { cwd: root, encoding: "utf8" });
    expect(result.status, result.stdout + result.stderr).toBe(0);
    return JSON.parse(readFileSync(join(root, "docs/ux-ui/standard-coverage.json"), "utf8"));
  };
  const manifest = generate(); expect(manifest.summary.routes).toBe(4);
  expect(manifest.closurePaths).toEqual([...new Set(manifest.closurePaths)].sort());
  for (const route of manifest.routes) {
    expect(routeClosureIds(manifest,route)).toEqual([...new Set<number>(routeClosureIds(manifest,route))].sort((a,b)=>a-b));
    expect(expandedRouteClosure(manifest, route)).toEqual([...expandedRouteClosure(manifest, route)].sort());
  }
  expect(manifest.routes.map(route => route.source)).toEqual(expect.arrayContaining([canonical, drawer, `${holding}/@record/[...path]/page.tsx`]));
  expect(manifest.sources.find(source => source.source === `${holding}/_presenters/page.tsx`)?.verification).toBe("pending");
  expect(manifest.routes.some(route => route.source.includes("/_presenters/"))).toBe(false);
  const canonicalCase = manifest.routes.find(route => route.source === canonical), drawerCase = manifest.routes.find(route => route.source === drawer);
  if (!canonicalCase || !drawerCase) throw new Error("Canonical or slot source disappeared from the actual inventory.");
  expect(canonicalCase.route).toBe("/holdings/[holdingSlug]/finance/documents/[id]"); expect(drawerCase.route).toBe(canonicalCase.route);
  expect(canonicalCase.routeContext).toEqual({ kind: "children", slots: [] });
  expect(drawerCase.routeContext.kind).toBe("parallel-slot"); expect(drawerCase.routeContext.slots[0]).toMatchObject({ name: "record", parentRoute: "/holdings/[holdingSlug]" });
  for (const route of [canonicalCase, drawerCase]) {
    expect(routeDependencies(manifest,route)).toContain("src/app/global-error.tsx");
    expect(routeDependencies(manifest,route)).toContain("src/app/(workspace)/holdings/forbidden.tsx");
    expect(route.styles).toContain("src/app/global-state.css"); expect(route.styles).toContain("src/app/(workspace)/holdings/denied.css");
    expect(route.boundaries).toContain(fallback); expect(expandedRouteClosure(manifest, route)).not.toContain(`${holding}/_presenters/forbidden.tsx`);
    expect(route.styles).not.toContain(`${holding}/_presenters/private.css`);
  }
  expect(drawerCase.boundaries).toContain(`${holding}/@record/error.tsx`);
  const before = new Map(manifest.routes.map(route => [route.source, route.fingerprint]));
  writeFileSync(join(root, "src/app/(workspace)/holdings/denied.css"), ".denied{color:HighlightText}");
  const changed = generate();
  for (const source of [canonical, drawer]) expect(changed.routes.find(route => route.source === source)?.fingerprint).not.toBe(before.get(source));
  expect(changed.routes.find(route => route.source === "src/app/page.tsx")?.fingerprint).toBe(before.get("src/app/page.tsx"));
});

type RuntimeImportCoverage = {
  summary: { routes: number };
  closurePaths: string[];
  routes: { source: string; closureIds: number[]; dependencies: string[]; fingerprint: string }[];
  sources: { source: string; fingerprint: string }[];
};

function runtimeImportCoverageFixture(fixture: Record<string, string>) {
  const root = mkdtempSync(join(tmpdir(), "erpx-runtime-imports-")); roots.push(root);
  for (const directory of ["scripts/ui", "docs/ux-ui", "node_modules"])
    mkdirSync(join(root, directory), { recursive: true });
  for (const file of ["coverage.mjs", "route-topology.mjs", "standard-scope.mjs", "shared-consumers.json", "inventory.mjs"])
    copyFileSync(resolve("scripts/ui", file), join(root, "scripts/ui", file));
  symlinkSync(resolve("node_modules/typescript"), join(root, "node_modules/typescript"), "dir");
  for (const [source, contents] of Object.entries(fixture)) {
    mkdirSync(join(root, source.slice(0, source.lastIndexOf("/"))), { recursive: true });
    writeFileSync(join(root, source), contents);
  }
  return { root, generate: (): RuntimeImportCoverage => {
    const result = spawnSync(process.execPath, [join(root, "scripts/ui/coverage.mjs")], { cwd: root, encoding: "utf8" });
    expect(result.status, result.stdout + result.stderr).toBe(0);
    return JSON.parse(readFileSync(join(root, "docs/ux-ui/standard-coverage.json"), "utf8"));
  } };
}

it("does not require the actual chart editor on Finance profile through erased FinanceChart imports, while retaining its real chart caller", () => {
  const moves=JSON.parse(readFileSync(resolve("scripts/architecture/workspace-relocations.json"),"utf8")).moves as {from:string;to:string}[];
  const current=(source:string)=>{const moved=moves.find(move=>move.from===source);if(!moved)throw new Error("Actual route relocation missing");return moved.to;};
  const base="src/app/holdings/[holdingSlug]/finance/setup",profile=current(`${base}/profile/page.tsx`),chart=current(`${base}/chart/page.tsx`),
    target="packages/feature-finance/src/components/finance-chart-controls.tsx",controls="packages/feature-finance/src/components/accounting-profile-controls.tsx";
  const files=[profile,current(`${base}/profile/_screen.tsx`),chart,current(`${base}/chart/_screen.tsx`),target,controls];
  const fixture = runtimeImportCoverageFixture(Object.fromEntries(files.map(source => [source, readFileSync(resolve(source), "utf8")])));
  const before = fixture.generate(), profileCase = before.routes.find(route => route.source === profile), chartCase = before.routes.find(route => route.source === chart);
  if (!profileCase || !chartCase) throw new Error("An actual Finance setup route was omitted.");
  expect(before.summary.routes).toBe(2);
  expect(routeDependencies(before,profileCase)).toContain(controls);
  expect(expandedRouteClosure(before, profileCase)).not.toContain(target); expect(routeDependencies(before,profileCase)).not.toContain(target);
  expect(expandedRouteClosure(before, chartCase)).toContain(target); expect(routeDependencies(before,chartCase)).toContain(target);
  expect(before.sources.some(source => source.source === target)).toBe(true);
  writeFileSync(join(fixture.root, target), readFileSync(resolve(target), "utf8") + "\n// A changed real chart editor remains a chart-route fingerprint dependency.\n");
  const after = fixture.generate();
  expect(after.routes.find(route => route.source === profile)?.fingerprint).toBe(profileCase.fingerprint);
  expect(after.routes.find(route => route.source === chart)?.fingerprint).not.toBe(chartCase.fingerprint);
  expect(after.sources.find(source => source.source === target)?.fingerprint).not.toBe(before.sources.find(source => source.source === target)?.fingerprint);
});

it("uses installed TypeScript erasure for explicit type edges in both TSX and TS passes and conservatively retains real value import forms", async () => {
  const { createRequire } = await import("node:module"), require = createRequire(import.meta.url), ts = require("typescript") as typeof import("typescript");
  const configuration = ts.parseConfigFileTextToJson("tsconfig.json", readFileSync(resolve("tsconfig.json"), "utf8"));
  expect(configuration.error).toBeUndefined();
  // A future verbatim-module change needs an explicit graph review: all-type
  // named clauses then emit an empty side-effect import/export instead.
  expect(configuration.config.compilerOptions.verbatimModuleSyntax ?? false).toBe(false);
  const specifier = "@/components/runtime-edge", target = "src/components/runtime-edge.tsx";
  const cases: { name: string; source: string; kept: boolean; currentEmits?: boolean; verbatimEmits?: boolean }[] = [
    { name: "import-type", source: `import type { Shape } from "${specifier}"; export type Captured = Shape;`, kept: false },
    { name: "import-type-default", source: `import type chosen from "${specifier}"; export type Captured = typeof chosen;`, kept: false },
    { name: "import-type-namespace", source: `import type * as choices from "${specifier}"; export type Captured = choices.Shape;`, kept: false },
    { name: "export-type", source: `export type { Shape } from "${specifier}";`, kept: false },
    { name: "export-type-star", source: `export type * from "${specifier}";`, kept: false },
    { name: "all-type-import", source: `import { type Shape } from "${specifier}"; export type Captured = Shape;`, kept: false, verbatimEmits: true },
    { name: "all-type-export", source: `export { type Shape } from "${specifier}";`, kept: false, verbatimEmits: true },
    { name: "mixed-import", source: `import { value, type Shape } from "${specifier}"; export const selected = value;`, kept: true },
    { name: "default-with-types", source: `import chosen, { type Shape } from "${specifier}"; export const selected = chosen;`, kept: true },
    { name: "default-import", source: `import chosen from "${specifier}"; export const selected = chosen;`, kept: true },
    { name: "namespace-import", source: `import * as choices from "${specifier}"; export const selected = choices.value;`, kept: true },
    { name: "type-named-value", source: `import { type as chosen } from "${specifier}"; export const selected = chosen;`, kept: true },
    { name: "deferred-namespace", source: `import defer * as choices from "${specifier}"; export const selected = choices.value;`, kept: true },
    { name: "mixed-export", source: `export { value, type Shape } from "${specifier}";`, kept: true },
    { name: "star-export", source: `export * from "${specifier}";`, kept: true },
    { name: "namespace-export", source: `export * as choices from "${specifier}";`, kept: true },
    { name: "side-effect", source: `import "${specifier}";`, kept: true },
    { name: "empty-value-import", source: `import {} from "${specifier}";`, kept: true, currentEmits: false },
    { name: "empty-value-export", source: `export {} from "${specifier}";`, kept: true, currentEmits: false },
    { name: "literal-dynamic", source: `export const acquire = () => import("${specifier}");`, kept: true },
    { name: "type-query", source: `export type Captured = typeof import("${specifier}");`, kept: false },
    { name: "nonliteral-dynamic", source: `const destination = "${specifier}"; export const acquire = () => import(destination);`, kept: false },
  ];
  const emittedDependency = (source: string, verbatimModuleSyntax: boolean) => {
    const emitted = ts.transpileModule(source, { fileName: "edge.tsx", compilerOptions: { target: ts.ScriptTarget.ESNext,
      module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.Preserve, isolatedModules: true, verbatimModuleSyntax } }).outputText;
    const ast = ts.createSourceFile("edge.js", emitted, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
    let found = false;
    const visit = (node: import("typescript").Node) => {
      const value = ts.isImportDeclaration(node) || ts.isExportDeclaration(node) ? node.moduleSpecifier
        : ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword ? node.arguments[0] : undefined;
      if (value && ts.isStringLiteral(value) && value.text === specifier) found = true;
      ts.forEachChild(node, visit);
    };
    visit(ast); return found;
  };
  const files: Record<string, string> = {
    [target]: "export type Shape = { id: string }; export const value = 1; export const type = 2; export default value; export function Edge(){return <section/>;}",
  };
  for (const item of cases) {
    expect(emittedDependency(item.source, false), `${item.name}: current installed TypeScript emit`).toBe(item.currentEmits ?? item.kept);
    expect(emittedDependency(item.source, true), `${item.name}: verbatim installed TypeScript emit`).toBe(item.verbatimEmits ?? item.kept);
    files[`src/app/tsx-${item.name}/page.tsx`] = `${item.source}\nexport default function Page(){return null;}`;
    files[`src/app/ts-${item.name}/page.tsx`] = 'import { ready } from "./relay"; export default function Page(){return ready;}';
    files[`src/app/ts-${item.name}/relay.ts`] = `${item.source}\nexport const ready = 1;`;
  }
  const manifest = runtimeImportCoverageFixture(files).generate();
  expect(manifest.summary.routes).toBe(cases.length * 2);
  for (const item of cases) for (const lane of ["tsx", "ts"]) {
    const source = `src/app/${lane}-${item.name}/page.tsx`, route = manifest.routes.find(candidate => candidate.source === source);
    if (!route) throw new Error(`The ${lane} import case disappeared: ${item.name}`);
    expect(expandedRouteClosure(manifest, route).includes(target), `${lane} ${item.name}: exact source edge`).toBe(item.kept);
    expect(routeDependencies(manifest,route).includes(target), `${lane} ${item.name}: UI owner requirements`).toBe(item.kept);
  }
  expect(manifest.sources.some(source => source.source === target)).toBe(true);
});

it("resolves the actual public runtime.ts owner ahead of a same-name directory in both TSX and TS caller graphs", () => {
  const publicOwner = "packages/server-runtime/src/public/runtime.ts", adapter = "packages/server-runtime/src/adapters/runtime.ts",
    publicChild = "packages/server-runtime/src/public/runtime/env.ts", adapterChild = "packages/server-runtime/src/adapters/runtime/env.ts";
  const sources = [publicOwner, adapter, publicChild, adapterChild];
  const fixture: Record<string, string> = Object.fromEntries(sources.map(source => [source, readFileSync(resolve(source), "utf8")]));
  const originalImport = 'import { runtimeEnv } from "@erpx/server-runtime/public/runtime";';
  fixture["src/app/runtime-direct/page.tsx"] = `${originalImport} export default function Page(){return typeof runtimeEnv;}`;
  fixture["src/app/runtime-relay/page.tsx"] = 'import { selected } from "./reader"; export default function Page(){return typeof selected;}';
  fixture["src/app/runtime-relay/reader.ts"] = `${originalImport} export const selected = runtimeEnv;`;
  const manifest = runtimeImportCoverageFixture(fixture).generate();
  expect(manifest.summary.routes).toBe(2);
  for (const source of ["src/app/runtime-direct/page.tsx", "src/app/runtime-relay/page.tsx"]) {
    const route = manifest.routes.find(candidate => candidate.source === source);
    if (!route) throw new Error(`The actual runtime owner caller was omitted: ${source}`);
    const closure = expandedRouteClosure(manifest, route);
    expect(closure).toContain(publicOwner); expect(closure).toContain(adapter); expect(closure).toContain(adapterChild);
    expect(closure).not.toContain(publicChild);
    expect(closure).not.toContain("packages/server-runtime/src/public/runtime");
    expect(closure).not.toContain("packages/server-runtime/src/adapters/runtime");
  }
});
