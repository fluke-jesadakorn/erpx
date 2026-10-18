import { headers, cookies } from "next/headers";
import { notFound, redirect } from "next/navigation";
import Link from "next/link";
import { configuredProductHandoff, productHostForHost } from "@/lib/runtime/product-hosts";
import { preferredAccountLanguage } from "@/server/modules/identity_access/public/account-preference-snapshot";
import {providerCommercialPageContext} from "@/server/modules/commercial/public/provider-page-context";
import {ProviderContextBoundary} from "@/components/identity_access/provider-context-boundary";
import {ProviderContextSwitcher} from "@/components/identity_access/provider-context-switcher";
import { ApiError } from "@/server/modules/runtime/public/http";
import { ProductNavigationBoundary } from "@/components/workplace/product-navigation-boundary";
import { ProductBrand } from "@/components/brand/product-brand";
import { PageContent, Stack, InlineNotice } from "@/components/ui/foundation";
import { CompanyPackageManager } from "@/components/commercial/company-package-manager";
import styles from "@/components/workplace/product-shell.module.css";

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
  const overview=current?.overview;
  return <ProductNavigationBoundary language={language}><div className={styles.shell}>
    <header className={styles.header}><Link className={styles.brand} data-product-switch href="/" prefetch={false}><ProductBrand appearance="standard" variant="symbol" className={styles.mark} /><strong>ERPX Administration</strong></Link>
      <nav className={styles.links}><a data-product-switch href={configuredProductHandoff("apps")!}>Apps</a><a data-product-switch href={configuredProductHandoff("account")!}>Account</a></nav></header>
    <PageContent as="main"><Stack><h1>{th ? "จัดการแพ็กเกจบริษัท" : "Manage Company packages"}</h1>
      <Link data-product-switch href="/provider/commercial" prefetch={false}>{th ? "ตรวจย้ายสิทธิ์เดิม" : "Review legacy access migration"}</Link>
      {overview ? <ProviderContextBoundary initial={current!.context}><ProviderContextSwitcher/><CompanyPackageManager companies={overview.companies} canApply={overview.canApply} language={language} /></ProviderContextBoundary>
        : <InlineNotice tone="warning">{th ? "ต้องใช้สิทธิ์ Platform Commercial เพื่อดูหรือเปลี่ยนแพ็กเกจบริษัท" : "Current Platform Commercial authority is required to review or change Company packages."}</InlineNotice>}
    </Stack></PageContent>
  </div></ProductNavigationBoundary>;
}
