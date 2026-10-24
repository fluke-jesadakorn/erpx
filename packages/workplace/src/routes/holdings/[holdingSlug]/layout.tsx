import type { LayoutProps } from "@erpx/contracts/next-routes";
import {manufacturingInitialPhysicalStockCountEntry} from "@erpx/server-inventory/public/rules/manufacturing-initial-physical-stock-count";
import {initialPhysicalStockCountEntry} from "@erpx/server-inventory/public/rules/initial-physical-stock-count";
import { workforceSelfServiceEntry } from "@erpx/server-people_payroll/public/rules/self-service-product-entry";
import { socialPostSourceEntry } from "@erpx/server-collaboration/public/rules/product-entry";
import {workRoleDisplayChoices} from "@erpx/feature-identity_access/lib/work-role-display";
import {ProductWorkLinks} from "@erpx/workplace/components/product-link-context";
import { canonicalUiHref,workspacePathParts } from "@erpx/feature-work_projection/lib/destination-registry";
import {projectTaskSourceEntry} from "@erpx/server-projects/public/rules/product-entry";
import {collaborationPresentationQueries} from "@erpx/server-collaboration/public/presentation-queries";
import { homeContextKey } from "@erpx/server-work_projection/public/home-cache";
import { workspaceNavigation, restrictedWorkspaceNavigation } from "@erpx/server-work_projection/public/navigation-snapshot";
import { workProjectionPresentationQueries } from "@erpx/server-work_projection/public/presentation-queries";

import { RecordWorkspaceProvider } from "@erpx/feature-records/components/record-workspace";
import { RecordScopeProvider } from "@erpx/ui/components/record-context";
import { MainContent } from "@erpx/ui/components/main";
import { Button } from "@erpx/ui/components/foundation";
import { measureReadPhase } from "@erpx/server-runtime/public/runtime/read-metrics";
import { financeWorkspaceV2Enabled } from "@erpx/server-finance/public/finance-presentation";
import { ProductBrand } from "@erpx/feature-brand/components/product-brand";
import { preferredAccountAppearance, preferredAccountLanguage } from "@erpx/server-identity_access/public/account-preference-snapshot";
import { configuredProductHandoff, configuredProductOrigin, productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { product } from "@erpx/server-commercial/public/product-registry";
import { productPageAccess } from "@erpx/server-identity_access/public/product-workspace";
import { adminPageAccess, adminWorkspaceTarget, adminWorkRolePath } from "@erpx/server-identity_access/public/admin-workspace";
import { ApiError } from "@erpx/server-runtime/public/http";
import { OfficeProductMenus } from "@erpx/workplace/components/office-product-menus";
import { AdminWorkspaceMenus } from "@erpx/workplace/components/admin-workspace-menus";
import { ProductNavigationBoundary } from "@erpx/workplace/components/product-navigation-boundary";
import { redirect } from "next/navigation";
import { WorkspaceBrandMark } from "@erpx/feature-brand/components/workspace-brand";
import { FeatureEnvironment } from "@erpx/feature-shared/components/features/feature-environment";
import { NavigationCountsProvider } from "@erpx/workplace/components/navigation-counts";

import { localShortcutKey } from "@erpx/feature-work_projection/lib/local-shortcuts";
import { CurrencyProvider } from "@erpx/feature-finance/components/money/currency-provider";
import { RememberWorkspace } from "@erpx/workplace/components/remember-workspace";
import styles from "@erpx/workplace/components/shell.module.css";
import { WorkContextBoundary } from "@erpx/workplace/components/work-context-boundary";
import { ReturnToFront } from "@erpx/workplace/components/return-to-front";
import { ConsoleCommandSearch as CommandSearch, ConsoleWorkplaceNavigation as WorkplaceNavigation } from "@erpx/workplace/components/console-ui-utilities";
import { WorkspaceAssistant } from "@erpx/workplace/components/workspace-assistant";
import {
  WorkspaceBreadcrumb,
  type WorkspaceCrumb,
} from "@erpx/workplace/components/workspace-breadcrumb";
import { WorkspaceMenu } from "@erpx/workplace/components/workspace-menu";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { administrationAccess } from "@erpx/server-runtime/public/administration";
import {
  publicScope,
  canEnterManageErpx,
  requirePlatformContext,
  requireAccountAccess,
  requireWorkContext,
} from "@erpx/server-identity_access/public/authorization";

import { pageError } from "@erpx/server-work_projection/public/page-error";

import { listWorkspaceCompanyPresentationFacts } from "@erpx/server-identity_access/public/workspace-company-presentation";
import {moneyContext} from '@erpx/server-finance/public/company-money';
import { companyFeatureCatalogue } from "@erpx/server-pms/public/hospitality-rollout";
import { availableNavigationAppIds } from "@erpx/feature-work_projection/lib/workplace-catalogue";
import { workspaceChoices } from "@erpx/server-organization/public/workspace-choices";
import { Building2 } from "lucide-react";
import { cookies, headers } from "next/headers";
import Link from "@erpx/workplace/components/workspace-link";
import { Suspense, type ComponentProps } from "react";
import { ConsoleAccountSwitcher as AccountSwitcher, ConsoleCurrencySelector as CurrencySelector } from "@erpx/workplace/components/console-ui-utilities";
async function DeferredWorkspaceMenu(
  props: Omit<ComponentProps<typeof WorkspaceMenu>, "choices" | "canManageErpx">,
) {
  const [choices, canManageErpx] = await Promise.all([
    workspaceChoices(),
    canEnterManageErpx().catch(() => false),
  ]);
  return <WorkspaceMenu appearance="standard" {...props} choices={choices} canManageErpx={canManageErpx} />;
}
export const dynamic = "force-dynamic";
export default async function WorkplaceLayout({
  children,
  record,
  params,
}: LayoutProps<"/holdings/[holdingSlug]"> & { record?: React.ReactNode }) {
  const [requestHeaders, { holdingSlug }] = await Promise.all([headers(), params]);
  const workContextCheckedAt = Date.now();
  const audience = productHostForHost(requestHeaders.get("host"));
  const currentProduct = audience && product(audience);
  // The Front daily shell and the original owner backoffice share one product.
  // Canonical Holding pages retain the Office shell for setup and long forms.
  const officeProduct = currentProduct ?? null;
  const adminManagementPath = /^\/holdings\/erpx\/erpx-management(?:\/|$)/.test(
    (requestHeaders.get("x-erpx-page-path") ?? "").split(/[?#]/, 1)[0] ?? "",
  );
  const productLanding=audience === "admin"
    ? adminManagementPath ? "erpx-management" : "administration"
    : officeProduct?.shell === "front" ? officeProduct.id : officeProduct?.landing ?? "home";
  let sourceRecord=false;
  let sourceProjectTask=false;
  let sourceConversation=false;
  let sourceSelfService=false;
  let sourcePostingSetup=false;
  let sourceManufacturingInitialStock=false;
  let sourceManufacturingPostingSetup=false;
  let sourceSalesPostingSetup=false;
  let sourcePayrollPostingSetup=false,sourceExpensePostingSetup=false;
  let sourceInitialPhysicalStock=false;
  if (audience === "admin") {
    const path = requestHeaders.get("x-erpx-page-path") ?? "";
    let companyId: string | undefined;
    try {
      // Resolve the Company without claiming an acting role. The normal Admin
      // selector can recover an absent/stale role before any owner data loads.
      const account = await requireAccountAccess(holdingSlug);
      companyId = account.scope.companyId;
      const access = await adminPageAccess(companyId, path);
      if (!access.target) redirect(access.chooseRole);
      if (access.target !== path) redirect(access.target);
    } catch (error) {
      if (error instanceof Error && error.message === "UNAUTHENTICATED"
        || error instanceof ApiError && error.status === 401)
        redirect("/sso/start?" + new URLSearchParams({ next: path || "/" }));
      if (error instanceof ApiError && error.code === "COMPANY_REQUIRED") redirect("/");
      const input = companyId && adminWorkspaceTarget(companyId, path);
      if (input && error instanceof ApiError && error.code === "ADMIN_ROLE_REQUIRED")
        redirect(adminWorkRolePath(input, "access-changed"));
      return pageError(error, holdingSlug);
    }
  }
  // A bookmark must satisfy the same product/role decision as Apps entry,
  // before the generic Work navigation starts loading owner data.
  if (officeProduct) {
    try {
      const actor = await requireWorkContext(holdingSlug);
      const path = requestHeaders.get("x-erpx-page-path");
      const access = await productPageAccess(officeProduct.id, actor.companyId, path ?? "");
      sourceRecord=access.sourceRecord;
      sourceProjectTask=officeProduct.id==="work"&&!!path&&!!projectTaskSourceEntry(workspacePathParts(new URL(path,"https://entry.invalid").pathname)?.path??"");
      const sourcePath=path?workspacePathParts(new URL(path,"https://entry.invalid").pathname)?.path??"":"";
      sourceConversation=officeProduct.id==="work"&&!!socialPostSourceEntry(sourcePath);
      sourceSelfService=officeProduct.id==="work"&&!!workforceSelfServiceEntry(sourcePath);
      sourcePostingSetup=officeProduct.id==="supply"&&sourcePath==="purchasing/setup/posting";
      sourceInitialPhysicalStock=officeProduct.id==="sales"&&!!initialPhysicalStockCountEntry(sourcePath);
      sourceManufacturingInitialStock=officeProduct.id==="manufacturing"&&!!manufacturingInitialPhysicalStockCountEntry(sourcePath);
      sourceManufacturingPostingSetup=officeProduct.id==="manufacturing"&&sourcePath==="production/manufacturing/settings/posting";
      sourceSalesPostingSetup=officeProduct.id==="sales"&&sourcePath==="customers-sales/sales/settings/posting";
      sourcePayrollPostingSetup=officeProduct.id==="people"&&sourcePath==="people-organization/people/payroll/settings/posting";
      sourceExpensePostingSetup=officeProduct.id==="finance"&&sourcePath==="finance/setup/expenses";
      if (!access.target) redirect(access.chooseRole);
      if (access.target !== path) redirect(access.target);
    } catch (error) { return pageError(error, holdingSlug); }
  }
  const navigationPending = sourceRecord
    ? restrictedWorkspaceNavigation(holdingSlug)
    : workspaceNavigation(holdingSlug);
  let context, initialNavigation;
  try {
    ({ context, initialNavigation } = await navigationPending);
  } catch (error) {
    return pageError(error, holdingSlug);
  }
  const moneyPending = moneyContext(context);
  const cataloguePending = !sourceRecord && process.env.NODE_ENV === "production" ? companyFeatureCatalogue(context) : Promise.resolve([]);
  const theme = await preferredAccountAppearance((await cookies()).get("erpx_appearance")?.value);
  const language = await preferredAccountLanguage((await cookies()).get("erpx_language")?.value === "th" ? "th" : "en");
  const holdingName = context.holdingName;
  const base = workspaceBase(publicScope(context));
  const scopeLabel =
    context.scope.kind === "department"
      ? context.scope.departmentName
      : context.scope.kind === "branch"
        ? context.scope.branchName
        : "Holding";
  const presentation = await workProjectionPresentationQueries(context.holdingId);
  const [company, workspaceBrand, companyFacts, admin, executiveVisible, money, featureCatalogue] = await measureReadPhase("layout", () => {
    const companyFactsPending = !sourceRecord && context.scope.kind === "holding"
      ? listWorkspaceCompanyPresentationFacts(context.holdingId)
      : Promise.resolve([]);
    const companyPending = companyFactsPending.then((facts) => {
      const selected = facts.find((company) => company.id === context.companyId);
      return selected
        ? { slug: selected.slug, display_name: selected.name }
        : presentation.activeCompanyIdentity(context.companyId);
    });
    return Promise.all([
      companyPending,
      presentation.resolveWorkspaceBrand(context),
      companyFactsPending,
      sourceRecord ? Promise.resolve({sections:[],manageOrganization:false}) : administrationAccess(context),
      sourceRecord ? Promise.resolve(false) : presentation.executiveVisible(context),
      moneyPending,
      cataloguePending,
    ]);
  });
  const companies = companyFacts.map(({ id, name }) => ({ id, name }));
  const companySlug = company?.slug;
  const frontLocation =
    context.scope.kind === "holding"
      ? "holding"
      : context.scope.kind === "branch"
        ? `branches/${context.scope.branchSlug}`
        : context.scope.branchSlug
          ? `branches/${context.scope.branchSlug}/departments/${context.scope.departmentSlug}`
          : `departments/${context.scope.departmentSlug}`;
  const frontBase = companySlug
    ? `/front/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(companySlug)}`
    : undefined;
  const frontHref = (app: "pos" | "pms") =>
    frontBase ? `${frontBase}/${app}/${frontLocation}/` : undefined;
  const handbookAccess=sourceRecord?{editor:false,owner:false}:await (await collaborationPresentationQueries(context.holdingId)).handbookAccess(context).catch(()=>({editor:false,owner:false}));
  const navigation = {
    ...(officeProduct ? { productId: officeProduct.id } : {}),
    handbookAccess:sourceRecord?{editor:false,owner:false}:handbookAccess,
    financeWorkspaceV2: financeWorkspaceV2Enabled(),
    manageErpxVisible:
      !sourceRecord &&
      context.holdingId === "h003" &&
      context.scope.kind === "holding" &&
      hasPermission(context, "manage_erpx.access") &&
      (await requirePlatformContext()
        .then((platform) => platform.userId === context.userId)
        .catch(() => false)),
    executiveVisible:sourceRecord?false:executiveVisible,
    workspaceBase: base,
    frontPosHref: frontHref("pos"),
    frontPmsHref: frontHref("pms"),
    scopeKind: context.scope.kind,
    beta: context.beta,
    role: context.role,
    moduleIds: sourceRecord?[]:context.modules,
    permissions: sourceRecord?[]:context.iam?.permissions,
    adminSections: sourceRecord?[]:admin.sections,
    canManageApprovals: !sourceRecord && admin.manageOrganization,
    ...(process.env.NODE_ENV === "production"
      ? {
          availableAppIds: availableNavigationAppIds(featureCatalogue),
        }
      : {}),
  };
  const companyName =
    companies.find((company) => company.id === context.companyId)?.name ??
    company?.display_name ??
    "Company unavailable";
  const workspaceLabel =
    context.scope.kind === "holding" ? companyName : `${companyName} · ${scopeLabel}`;
  const holdingBase = workspaceBase({ kind: "holding", holdingSlug });
  const ancestor = (label: string, path: string): WorkspaceCrumb => ({
    label,
    // Scoped routes share an internal layout after proxy rewriting. A document
    // navigation refreshes its authorization context when changing workspace.
    fullNavigation: path !== base,
    ...(context.workspaces.some((workspace) => workspace.href === `${path}/home`)
      ? { href: canonicalUiHref(`${path}/${productLanding}`) }
      : {}),
  });
  const ancestors = [ancestor(holdingName, holdingBase)];
  if (context.scope.kind !== "holding" && context.scope.branchSlug) {
    ancestors.push(
      ancestor(
        context.scope.branchName ?? context.scope.branchSlug,
        workspaceBase({ kind: "branch", holdingSlug, branchSlug: context.scope.branchSlug }),
      ),
    );
  }
  if (context.scope.kind === "department")
    ancestors.push(ancestor(context.scope.departmentName, base));
  const workplace = (
      <CurrencyProvider initial={money} apiBase={`/api/v1${base}`}>
        <div className={styles.shell}>
          <RememberWorkspace
            base={base}
            companyId={context.companyId}
            assignmentId={context.assignmentId}
            revision={context.authority!.revision}
          />
          <a href="#workspace-content" className="skip-link">
            Skip to content
          </a>
          <aside className={styles.sidebar} data-app-chrome>
            <Link className={styles.brand} href={sourceRecord?configuredProductHandoff("apps")!:canonicalUiHref(`${base}/${productLanding}`)}>
              <ProductBrand appearance="standard" className={`${styles.productBrand} ${styles.productBrandWordmark}`} />
              <ProductBrand appearance="standard"
                variant="symbol"
                className={`${styles.productBrand} ${styles.productBrandSymbol}`}
              />
              <small>{officeProduct?.label[language] ?? "WORKSPACE"}</small>
            </Link>
            {!sourceRecord && <Suspense>
              <WorkplaceNavigation {...navigation} />
            </Suspense>}
            <div className={styles.sidebarBottom}>
              <div>
                <Building2 size={17} />
                <div>
                  <strong>{holdingName}</strong>
                  <small>Connected work, shared progress</small>
                </div>
              </div>
            </div>
          </aside>
          <div className={styles.main} data-app-main>
            <header className={styles.topbar} data-app-chrome>
              <div className={styles.workspace}>
                <WorkspaceBrandMark appearance="standard" brand={workspaceBrand} compact />
                {sourceRecord ? <Link href={configuredProductHandoff("apps")!} aria-label={language==="th"?"เลือกผลิตภัณฑ์ใน Apps":"Choose a product in Apps"}>{workspaceLabel} · Apps</Link> : officeProduct ? <Suspense fallback={<span>{workspaceLabel}</span>}><OfficeProductMenus productId={officeProduct.id} companyId={context.companyId} scopeBase={base} workspaceLabel={workspaceLabel} language={language} /></Suspense> : audience === "admin" ? <Suspense fallback={<span>{workspaceLabel}</span>}><AdminWorkspaceMenus companyId={context.companyId} scopeBase={base} workspaceLabel={workspaceLabel} management={adminManagementPath} language={language}/></Suspense> : <Suspense
                  fallback={
                    <Button appearance="standard"
                      variant="ghost"
                      className={styles.workspaceTrigger}
                      type="button"
                      aria-label="Switch workspace"
                      title={`Change workspace (currently ${workspaceLabel})`}
                      disabled
                    >
                      <Building2 size={18} aria-hidden="true" />
                      <span className={styles.workspaceTriggerLabel}>{workspaceLabel}</span>
                    </Button>
                  }
                >
                  <DeferredWorkspaceMenu
                    label={workspaceLabel}
                    current={
                      context.scope.kind === "holding"
                        ? {
                            kind: "holding",
                            holdingSlug,
                            companyId: context.companyId,
                          }
                        : context.scope.kind === "branch"
                          ? {
                              kind: "branch",
                              holdingSlug,
                              companyId: context.companyId,
                              branchId: context.scope.branchId,
                            }
                          : {
                              kind: "department",
                              holdingSlug,
                              companyId: context.companyId,
                              branchId: context.scope.branchId,
                              departmentId: context.scope.departmentId,
                            }
                    }
                  />
                </Suspense>}

                <Suspense
                  fallback={
                    <span className={styles.breadcrumbFallback}>
                      {scopeLabel === "Holding" ? holdingName : scopeLabel}
                    </span>
                  }
                >
                  {sourceRecord?<span>{officeProduct?.id==="work"?(sourceConversation?(language==="th"?"บทสนทนาของรายการต้นทาง":"Source conversation"):sourceSelfService?(language==="th"?"คำขอของฉัน":"My requests"):sourceProjectTask?(language==="th"?"งานในโครงการ":"Project task"):(language==="th"?"การแจ้งเตือน":"Notifications")):officeProduct?.id==="recruit"?(language==="th"?"ความเป็นส่วนตัวของผู้สมัคร":"Candidate privacy"):officeProduct?.id==="people"?(sourcePayrollPostingSetup?(language==="th"?"ตั้งค่าบัญชีเงินเดือน":"Payroll posting setup"):requestHeaders.get("x-erpx-page-path")?.includes("/people/payroll/")?(language==="th"?"การชำระภาระผูกพันเงินเดือน":"Payroll settlement"):(language==="th"?"รับเข้าทำงานจากข้อเสนอที่ยอมรับแล้ว":"Accepted hire handoff")):sourceExpensePostingSetup?(language==="th"?"ตั้งค่าบัญชีค่าใช้จ่าย":"Expense posting setup"):sourceInitialPhysicalStock?(language==="th"?"ตรวจนับสต็อกครั้งแรก":"Initial physical stock count"):sourceManufacturingInitialStock?(language==="th"?"ตรวจนับวัสดุตั้งต้น":"Initial physical material count"):sourceManufacturingPostingSetup?(language==="th"?"ตั้งค่าบัญชีการผลิต":"Manufacturing posting setup"):sourceSalesPostingSetup?(language==="th"?"ตั้งค่าบัญชีงานขาย":"Sales posting setup"):officeProduct?.id==="supply"?(sourcePostingSetup?(language==="th"?"ตั้งค่าบัญชีจัดซื้อและสินค้า":"Purchase and stock posting setup"):(language==="th"?"งานคุณภาพจากรายการต้นทาง":"Source quality work")):(language==="th"?"งานการเงินที่รวมในผลิตภัณฑ์":"Included Finance work")}</span>:<WorkspaceBreadcrumb
                    workspaceBase={base}
                    ancestors={ancestors}
                    navigation={navigation}
                  />}
                </Suspense>
              </div>
              <div className={styles.actions}>
                {(officeProduct || audience === "admin") && <a data-product-switch href={configuredProductHandoff("apps")!}>Apps</a>}
                <CurrencySelector />
                {officeProduct && <Link href={`${base}/inbox/notifications`}>{language==="th"?"การแจ้งเตือน":"Notifications"}</Link>}
                {!sourceRecord && <CommandSearch {...navigation} />}
                <AccountSwitcher
                  accountNavigation={configuredProductOrigin("account")!==null}
                  workspaceBase={base}
                  assignments={workRoleDisplayChoices(context.availableAssignments ?? context.assignments)}
                  preferredAssignmentId={context.assignmentId}
                  contextRevision={context.authority!.revision}
                  companyId={context.companyId}
                  activeUser={{
                    id: context.userId,
                    name: context.name,
                    email: context.email,
                  }}
                  theme={theme}
                />
              </div>
            </header>
            <div>
              <WorkContextBoundary appearance="standard"
                key={homeContextKey(context)}
                base={base}
                userId={context.userId}
                companyId={context.companyId}
                assignmentId={context.assignmentId}
                revision={context.authority!.revision}
                initialCurrent={{version:1,base,userId:context.userId,companyId:context.companyId,assignmentId:context.assignmentId,revision:context.authority!.revision,checkedAt:workContextCheckedAt}}
              >
                {!sourceRecord&&<Suspense>
                  <WorkspaceAssistant
                    base={base}
                    userId={context.userId}
                    companyId={context.companyId}
                    assignmentId={context.assignmentId}
                    crossContext={hasPermission(context, "ai.context.cross")}
                  />
                </Suspense>}
                <div className={styles.workspaceBody}>
                  <MainContent
                    id="workspace-content"
                    className={styles.workspacePage}
                    tabIndex={-1}
                  >
                    <Suspense>
                      <ReturnToFront
                        base={base}
                        holdingSlug={holdingSlug}
                        companySlug={companySlug}
                        companyId={context.companyId}
                        userId={context.userId}
                        assignmentId={context.assignmentId}
                      />
                    </Suspense>
                    <FeatureEnvironment
                      navigation={navigation}
                      productionCatalogue={
                        !sourceRecord && process.env.NODE_ENV === "production"
                          ? featureCatalogue.map(({ id, name, state }) => ({ id, name, state }))
                          : undefined
                      }
                    >
                      <RecordWorkspaceProvider><RecordScopeProvider base={base}>{children}{record}</RecordScopeProvider></RecordWorkspaceProvider>
                    </FeatureEnvironment>
                  </MainContent>
                </div>
              </WorkContextBoundary>
            </div>
          </div>
          <div data-app-chrome>
            {!sourceRecord && <Suspense>
              <WorkplaceNavigation {...navigation} mobile />
            </Suspense>}
          </div>
        </div>
      </CurrencyProvider>
  );
  const counted=sourceRecord?workplace:<NavigationCountsProvider key={homeContextKey(context)} base={base} initial={initialNavigation} shortcutStorageKey={localShortcutKey(context.userId,context.companyId,base)}>{workplace}</NavigationCountsProvider>;
  return <ProductWorkLinks companyId={context.companyId} companySlug={companySlug??undefined} workspaceBase={base}>{officeProduct || audience === "admin" ? <ProductNavigationBoundary language={language}>{counted}</ProductNavigationBoundary> : counted}</ProductWorkLinks>;
}
