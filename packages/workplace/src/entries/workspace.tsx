import {requestProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import {holdingRoutingEnabled,holdingHostForHost} from '@erpx/contracts/holding-hosts';
import {AppsHomeLink} from "@erpx/workplace/components/product-link-context";
import {productAccessLabel} from '@erpx/feature-work_projection/lib/product-access-label';
import { productEntrySsoHref, type EntryPageSearchParams } from '@erpx/server-identity_access/public/rules/page-entry-return';
import { measureReadPhase } from "@erpx/server-runtime/public/runtime/read-metrics";
import { preferredAccountLanguage } from "@erpx/server-identity_access/public/account-preference-snapshot";
import { cookies, headers } from "next/headers";
import { notFound, redirect } from "next/navigation";
import { product } from "@erpx/server-commercial/public/product-registry";
import { configuredProductOrigin, configuredProductHandoff, productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { createAuth } from "@erpx/server-identity_access/public/auth";
import { providerEntryOverview } from "@erpx/server-identity_access/public/provider-entry";
import { adminLaunchChoices } from "@erpx/server-work_projection/public/admin-launcher";
import { productLaunchChoices, existingWorkspaceLaunchChoices } from "@erpx/server-work_projection/public/product-launcher";
import { ProductBrand } from "@erpx/feature-brand/components/product-brand";
import { PageContent, Stack, SectionHeader, InlineNotice } from "@erpx/ui/components/foundation";
import { ProductNavigationBoundary } from "@erpx/workplace/components/product-navigation-boundary";
import {ProductAccountTrigger} from '@erpx/workplace/components/product-account-provider';
import {AppsSidebar} from '@erpx/workplace/components/apps-sidebar';
import { savedAppsWorkspaceForPresentation } from "@erpx/server-work_projection/public/workspace-entry-presentation-query";
import {AppsLauncher} from '@erpx/workplace/components/apps-launcher';
import {buildAppsCompanies} from '@erpx/feature-work_projection/lib/apps-launcher';
import styles from "@erpx/workplace/components/product-shell.module.css";

export const dynamic = "force-dynamic";
export default async function ProductEntryPage({ searchParams, applications = false }: { searchParams?: Promise<EntryPageSearchParams>; applications?: boolean }) {
  if (applications) redirect("/?launcher=open");
  const requestHeaders = await headers();
  const host = productHostForHost(requestHeaders.get("host"));
  if (!host || host === "account" || host === "booking" || host === "portal" || host === "legal-portal" || host === "forms") notFound();
  const language = await preferredAccountLanguage((await cookies()).get("erpx_language")?.value === "th" ? "th" : "en");
  const session = await (await createAuth(requestProductOrigin(host,requestHeaders)!)).api.getSession({ headers: requestHeaders });
  if (!session) redirect(holdingRoutingEnabled(process.env)?'/sso/start?'+new URLSearchParams({next:requestHeaders.get('x-erpx-visible-path')??'/'}):productEntrySsoHref(await searchParams ?? {}));
  const sharedAdmin = host === "apps" || host === "admin";
  const [products, adminChoices, provider] = await Promise.all([
    measureReadPhase("product_entry_products", () => productLaunchChoices(host === "apps" || host === "admin" ? undefined : host)),
    sharedAdmin ? measureReadPhase("product_entry_admin", adminLaunchChoices) : [],
    sharedAdmin ? measureReadPhase("product_entry_provider", providerEntryOverview) : null,
  ]);
  const choices = products.filter(choice => host === "apps" || host === choice.productId);
  // During phased migration Apps keeps every old work entry available even
  // after the first Company grant. Remove recovery only at accepted cutover.
  if (!holdingRoutingEnabled(process.env)&&choices.length === 1 && host !== "admin" && host !== "apps") redirect(choices[0].href);
  const existingWorkspaces = host === "apps" || host === "admin" || choices.length === 0 ? await measureReadPhase("product_entry_recovery", existingWorkspaceLaunchChoices) : [];
  const remembered = host === "apps"&&!holdingRoutingEnabled(process.env) ? await savedAppsWorkspaceForPresentation(session.user.id) : null;
  return <ProductNavigationBoundary language={language}><div className={styles.shell}>
    <header className={styles.header}><AppsHomeLink className={styles.brand}><ProductBrand appearance="standard" variant="symbol" className={styles.mark} /><strong>{host === "apps" ? "ERPX Apps" : `ERPX ${product(host)?.label[language] ?? "Administration"}`}</strong></AppsHomeLink>
      <span className={styles.context}/><ProductAccountTrigger/><nav className={styles.links}>{host === "apps" && <AppsLauncher key={session.user.id} accountId={session.user.id} language={language} entry requireSelection={holdingRoutingEnabled(process.env)} currentCompanyId={remembered?.companyId} currentWorkspaceId={remembered?.workspaceId}
        companies={buildAppsCompanies(choices.map(choice => ({...choice, label: choice.productId === "apps" ? (language === "th" ? "ฟีด" : "Feed") : product(choice.productId)!.label[language], accessLabel: productAccessLabel(choice, language)})), adminChoices)}
        provider={provider ? {href: configuredProductHandoff("admin", "/provider")!, role: provider.role} : null} existingWorkspaces={existingWorkspaces} />}<a data-product-switch href={new URL("/", configuredProductOrigin("account")!).href}>Account</a></nav></header>
    {host === "apps" ? <>
      <PageContent as="main"><h1>{language === "th" ? "ฟีด" : "Feed"}</h1><p>{language === "th" ? "เลือกแอปและพื้นที่ทำงานจากแถบนำทางเพื่อเริ่มใช้งาน" : "Choose an app and workspace from the navigation to get started."}</p></PageContent>
    </> : <div className={styles.body}>
    <AppsSidebar language={language} currentId={host} items={[
      {id:'applications', label:language === "th" ? "แอปทั้งหมด" : "All apps", href:configuredProductHandoff('apps', '/?launcher=open')!},
      ...choices.filter((choice,index,all)=>all.findIndex(item=>item.productId===choice.productId)===index).map(choice=>({id:choice.productId,productId:choice.productId,label:product(choice.productId)!.label[language],href:choice.href})),
      {id:'account',label:language==='th'?'บัญชีส่วนตัว':'Account',href:configuredProductOrigin('account')!},
    ]}/>
    <PageContent as="main"><Stack><SectionHeader><h1>{language === "th" ? "เลือกพื้นที่ทำงาน" : "Choose a workspace"}</h1></SectionHeader>
      {choices.length === 0 && adminChoices.length === 0 && !provider && <InlineNotice tone="info">{existingWorkspaces.length > 0
        ? (language === "th" ? "เปิดพื้นที่ทำงานเดิมได้จากรายการด้านล่าง การตั้งค่าผลิตภัณฑ์แยกยังอยู่ระหว่างดำเนินการ" : "Open your existing workspaces below. Product setup is still in progress.")
        : (language === "th" ? "ยังไม่มีผลิตภัณฑ์ที่มีสิทธิ์แพ็กเกจหรือประวัติและสิทธิ์พื้นที่ทำงานพร้อมใช้งาน" : "No products have both available purchase/history rights and current workspace access.")}</InlineNotice>}
      <div className={styles.grid}>{choices.map(choice => <a data-product-switch className={styles.tile} href={choice.href} key={choice.href}><strong>{product(choice.productId)!.label[language]}</strong><span>{choice.company}</span><span className={styles.muted}>{choice.workspace === choice.company ? choice.holding : `${choice.holding} · ${choice.workspace}`}</span>{productAccessLabel(choice,language)&&<span>{productAccessLabel(choice,language)}</span>}</a>)}</div>
      {adminChoices.some(choice => choice.kind === "company") && <section><Stack><h2>{language === "th" ? "ดูแลบริษัท" : "Company administration"}</h2><div className={styles.grid}>{adminChoices.filter(choice => choice.kind === "company").map(choice => <a data-product-switch className={styles.tile} href={choice.href} key={choice.href}><strong>{choice.company}</strong><span>{language === "th" ? "พื้นที่ดูแลบริษัท" : "Company administration"}</span><span className={styles.muted}>{choice.holding} · {choice.workspace}</span></a>)}</div></Stack></section>}
      {provider && <div className={styles.grid}><a data-product-switch className={styles.tile} href={configuredProductHandoff("admin", "/provider")!}><strong>{language === "th" ? "พื้นที่ผู้ให้บริการ" : "Provider administration"}</strong><span className={styles.muted}>{provider.role}</span></a></div>}
      {existingWorkspaces.length > 0 && <section><Stack><h2>{language === "th" ? "พื้นที่ทำงานเดิมระหว่างการย้าย" : "Existing workspaces during migration"}</h2><div className={styles.grid}>{existingWorkspaces.map(workspace => <a data-product-switch className={styles.tile} href={workspace.href} key={workspace.href}><strong>{workspace.holding}</strong><span className={styles.muted}>{language === "th" ? "เปิดพื้นที่ทำงานเดิม" : "Open existing workspace"}</span></a>)}</div></Stack></section>}
    </Stack></PageContent></div>}
  </div></ProductNavigationBoundary>;
}
