import {AppsHomeLink} from "@erpx/workplace/components/product-link-context";
import {ProductAccountTrigger} from "@erpx/workplace/components/product-account-provider";
import {AppsSidebar} from "@erpx/workplace/components/apps-sidebar";
import { headers, cookies } from "next/headers";
import { notFound, redirect } from "next/navigation";
import Link from "next/link";
import { configuredProductHandoff, productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { preferredAccountLanguage } from "@erpx/server-identity_access/public/account-preference-snapshot";
import {providerCommercialPageContext} from "@erpx/server-commercial/public/provider-page-context";
import {ProviderContextBoundary} from "@erpx/feature-identity_access/components/provider-context-boundary";
import {ProviderContextSwitcher} from "@erpx/feature-identity_access/components/provider-context-switcher";
import { ApiError } from "@erpx/server-runtime/public/http";
import { ProductNavigationBoundary } from "@erpx/workplace/components/product-navigation-boundary";
import { ProductBrand } from "@erpx/feature-brand/components/product-brand";
import { PageContent, Stack, InlineNotice } from "@erpx/ui/components/foundation";
import { CompanyPackageManager } from "@erpx/feature-commercial/components/company-package-manager";
import styles from "@erpx/workplace/components/product-shell.module.css";

export const dynamic = "force-dynamic";
export default async function CompanyPackagesPage() {
  if (productHostForHost((await headers()).get("host")) !== "admin") notFound();
  const language = await preferredAccountLanguage((await cookies()).get("erpx_language")?.value === "th" ? "th" : "en"), th = language === "th";
  let current:Awaited<ReturnType<typeof providerCommercialPageContext>>|undefined;
  try { current = await providerCommercialPageContext(); }
  catch (error) {
    if (error instanceof Error && error.message === "UNAUTHENTICATED" || error instanceof ApiError && error.status === 401)
      redirect("/sso/start?next=%2Fprovider%2Fcommercial%2Fpackages");
    if (!(error instanceof Error && error.message === "FORBIDDEN") && !(error instanceof ApiError && error.status === 403)) throw error;
  }
  const overview=current?.overview,roleLabel=current?.context.choices.find(choice=>choice.id===current?.context.selectedId)?.label;
  return <ProductNavigationBoundary language={language}><div className={styles.shell}>
    <header className={styles.header}><AppsHomeLink className={styles.brand}><ProductBrand appearance="standard" variant="symbol" className={styles.mark} /><strong>ERPX Administration</strong></AppsHomeLink>
      <nav className={styles.links}><a data-product-switch href={configuredProductHandoff("apps")!}>Apps</a><a data-product-switch href={configuredProductHandoff("account")!}>Account</a></nav><ProductAccountTrigger roleLabel={roleLabel} language={language}/></header>
    <div className={styles.body}><AppsSidebar language={language} roleLabel={roleLabel} items={[{id:"admin",label:"Admin",href:"/",groupId:"administration"},{id:"provider",label:"Platform / Provider",href:"/provider",groupId:"administration"}]}/><PageContent as="main"><Stack><h1>{th ? "จัดการแพ็กเกจบริษัท" : "Manage Company packages"}</h1>
      <Link data-product-switch href="/provider/commercial" prefetch={false}>{th ? "ตรวจย้ายสิทธิ์เดิม" : "Review legacy access migration"}</Link>
      {overview ? <ProviderContextBoundary initial={current!.context}><ProviderContextSwitcher/><CompanyPackageManager companies={overview.companies} canApply={overview.canApply} language={language} /></ProviderContextBoundary>
        : <InlineNotice tone="warning">{th ? "ต้องใช้สิทธิ์ Platform Commercial เพื่อดูหรือเปลี่ยนแพ็กเกจบริษัท" : "Current Platform Commercial authority is required to review or change Company packages."}</InlineNotice>}
    </Stack></PageContent></div>
  </div></ProductNavigationBoundary>;
}
