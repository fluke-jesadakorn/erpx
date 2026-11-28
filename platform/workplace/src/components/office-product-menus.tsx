import "server-only";
import { holdingRoutingEnabled } from "@erpx/platform-contracts/holding-hosts";
import { productAccessLabel } from "@erpx/work-projection-ui/lib/product-access-label";
import { buildAppsCompanies } from "@erpx/work-projection-ui/lib/apps-launcher";
import { adminLaunchChoices } from "@erpx/work-projection-server/public/admin-launcher";
import { productLaunchChoices } from "@erpx/work-projection-server/public/product-launcher";
import { product, type ProductId } from "@erpx/commercial-server/public/product-registry";
import { providerEntryOverview } from "@erpx/identity-access-server/public/provider-entry";
import { configuredProductHandoff } from "@erpx/platform-runtime-ui/lib/product-hosts";
import { AppsLauncher } from "@erpx/platform-workplace/components/apps-launcher";

export async function OfficeProductMenus({ productId, accountId, companyId, scopeBase, workspaceLabel, language, productLabel }: {
  productId: ProductId | "admin"; accountId: string; companyId: string; scopeBase: string; workspaceLabel: string; language: "en" | "th"; productLabel?: string;
}) {
  const routed = holdingRoutingEnabled(process.env);
  const holdingSlug = scopeBase.split("/")[2];
  const [choices, adminChoices, provider] = await Promise.all([
    productLaunchChoices(undefined, routed ? holdingSlug : undefined), adminLaunchChoices(), providerEntryOverview(),
  ]);
  const th = language === "th";
  const companies = buildAppsCompanies(choices.map(choice => ({
    ...choice, label: choice.productId === "apps" ? th ? "ฟีด" : "Feed" : product(choice.productId)!.label[language],
    accessLabel: productAccessLabel(choice, language),
  })), adminChoices);
  return <AppsLauncher key={accountId} accountId={accountId} language={language}
    currentCompanyId={JSON.stringify([holdingSlug, companyId])} currentWorkspaceId={scopeBase} currentProductId={productId}
    productLabel={productLabel ?? (productId === "apps" ? th ? "แอป" : "Apps" : productId === "admin" ? th ? "ดูแลบริษัท" : "Administration" : product(productId)!.label[language])}
    workspaceLabel={workspaceLabel}
    companies={routed ? companies.filter(company => company.holdingSlug === holdingSlug) : companies}
    provider={provider ? { href: configuredProductHandoff("admin", "/provider")!, role: provider.role } : null} />;
}
