import "server-only";
import { holdingRoutingEnabled } from "@erpx/contracts/holding-hosts";
import { productAccessLabel } from "@erpx/feature-work_projection/lib/product-access-label";
import { buildAppsCompanies } from "@erpx/feature-work_projection/lib/apps-launcher";
import { adminLaunchChoices } from "@erpx/server-work_projection/public/admin-launcher";
import { productLaunchChoices } from "@erpx/server-work_projection/public/product-launcher";
import { product, type ProductId } from "@erpx/server-commercial/public/product-registry";
import { providerEntryOverview } from "@erpx/server-identity_access/public/provider-entry";
import { configuredProductHandoff } from "@erpx/feature-runtime/lib/product-hosts";
import { AppsLauncher } from "@erpx/workplace/components/apps-launcher";

export async function OfficeProductMenus({ productId, accountId, companyId, scopeBase, workspaceLabel, language, productLabel }: {
  productId: ProductId | "admin"; accountId: string; companyId: string; scopeBase: string; workspaceLabel: string; language: "en" | "th"; productLabel?: string;
}) {
  const [choices, adminChoices, provider] = await Promise.all([productLaunchChoices(), adminLaunchChoices(), providerEntryOverview()]);
  const th = language === "th";
  const holdingSlug = scopeBase.split("/")[2];
  const companies = buildAppsCompanies(choices.map(choice => ({
    ...choice, label: choice.productId === "apps" ? th ? "ฟีด" : "Feed" : product(choice.productId)!.label[language],
    accessLabel: productAccessLabel(choice, language),
  })), adminChoices);
  return <AppsLauncher key={accountId} accountId={accountId} language={language}
    currentCompanyId={JSON.stringify([holdingSlug, companyId])} currentWorkspaceId={scopeBase}
    productLabel={productLabel ?? (productId === "apps" ? th ? "แอป" : "Apps" : productId === "admin" ? th ? "ดูแลบริษัท" : "Administration" : product(productId)!.label[language])}
    workspaceLabel={workspaceLabel}
    companies={holdingRoutingEnabled(process.env) ? companies.filter(company => company.holdingSlug === holdingSlug) : companies}
    provider={provider ? { href: configuredProductHandoff("admin", "/provider")!, role: provider.role } : null} />;
}
