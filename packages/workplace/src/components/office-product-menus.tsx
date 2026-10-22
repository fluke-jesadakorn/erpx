import {productAccessLabel} from '@erpx/feature-work_projection/lib/product-access-label';
import "server-only";
import {adminLaunchChoices} from "@erpx/server-work_projection/public/admin-launcher";
import {adminWorkspaceTarget} from "@erpx/server-identity_access/public/rules/admin-workspace";
import { productLaunchChoices } from "@erpx/server-work_projection/public/product-launcher";
import { product, type ProductId } from "@erpx/server-commercial/public/product-registry";
import { configuredProductHandoff } from "@erpx/feature-runtime/lib/product-hosts";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { OfficeProductPicker } from "@erpx/workplace/components/office-product-picker";

export async function OfficeProductMenus({ productId, companyId, scopeBase, workspaceLabel, language }: {
  productId: ProductId; companyId: string; scopeBase: string; workspaceLabel: string; language: "en" | "th";
}) {
  const [choices,adminChoices]=await Promise.all([productLaunchChoices(),adminLaunchChoices()]),th=language=== "th";
  const admin=adminChoices.filter(choice=>{if(choice.companyId!==companyId||choice.kind!=="company")return false;const next=new URL(choice.href).searchParams.get("next"),target=next?new URL(next,"https://admin.invalid").searchParams.get("next"):null,input=adminWorkspaceTarget(choice.companyId,target);return !!input&&input.kind==="company"&&workspaceBase(input.scope)===scopeBase;});
  const products = choices.filter(choice => choice.companyId === companyId && workspaceBase(choice.scope) === scopeBase);
  const workspaces = choices.filter(choice => choice.productId === productId);
  const purpose=(choice:typeof choices[number])=>productAccessLabel({purchasePurpose:choice.purchasePurpose,sourceRecord:choice.sourceRecord},language);
  return <OfficeProductPicker productLabel={product(productId)!.label[language]} workspaceLabel={workspaceLabel} language={language}
    appsHref={configuredProductHandoff("apps")!} accountHref={configuredProductHandoff("account")!}
    products={[...products.map(choice => ({ href: choice.href, label: product(choice.productId)!.label[language], detail: purpose(choice), current: choice.productId === productId })),...admin.map(choice=>({href:choice.href,label:th?"ดูแลบริษัท":"Company Admin"}))]}
    workspaces={workspaces.map(choice => ({ href: choice.href, label: `${choice.holding} · ${choice.company}${choice.workspace !== choice.company ? ` · ${choice.workspace}` : ""}`,
      detail: purpose(choice), current: choice.companyId === companyId && workspaceBase(choice.scope) === scopeBase }))} />;
}
