import { OfficeProductMenus } from "@erpx/workplace/components/office-product-menus";

export function AdminWorkspaceMenus({ accountId, companyId, scopeBase, workspaceLabel, management, language }: {
  accountId: string; companyId: string; scopeBase: string; workspaceLabel: string; management: boolean; language: "en" | "th";
}) {
  return <OfficeProductMenus accountId={accountId} productId="admin" companyId={companyId} scopeBase={scopeBase} workspaceLabel={workspaceLabel} language={language} productLabel={management ? "ERPX Management" : undefined} />;
}
