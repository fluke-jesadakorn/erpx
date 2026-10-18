"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { SetupChecklist } from "@/components/ui/setup-checklist";
import { FrontLocalizedTree } from "@/components/front/front-language";
import type {BackofficePosConsoleContext} from "./pos-backoffice-console";

export function BackofficePosConsoleReadinessScreen({context}: {context: BackofficePosConsoleContext}) {
  const {tr, data, erpBase, register, shift, dailyPosRoot, currentTabLoaded} = context;
  return <FrontLocalizedTree>{(
                <SetupChecklist
                  title={tr("Ready to sell")}
                  items={[
                    {
                      id: "settings",
                      label: tr("Accounting and sales setup"),
                      ready: !!data.settings?.enabled,
                      owner: tr("POS administrator / Finance"),
                      href: canonicalUiHref(`${erpBase}/pos?tab=settings`),
                      detail: data.settings?.enabled
                        ? tr("Configuration enabled")
                        : tr("Complete setup and enable sales"),
                    },
                    {
                      id: "register",
                      label: tr("Active register"),
                      ready: !!register?.active,
                      owner: tr("POS administrator"),
                      href: canonicalUiHref(`${erpBase}/pos?tab=settings`),
                    },
                    {
                      id: "shift",
                      label: tr("Open cash session"),
                      ready: !!shift,
                      owner: tr("Cashier"),
                      href: `${dailyPosRoot}?tab=shifts`,
                      detail: shift
                        ? tr("Ready to sell")
                        : tr("Open a session before the first sale"),
                    },
                    ...(currentTabLoaded
                      ? [
                          {
                            id: "products",
                            label: tr("Products in Commerce"),
                            ready: data.products.length > 0,
                            owner: "Commerce",
                            href: canonicalUiHref(`${erpBase}/commerce`),
                            detail: data.products.length
                              ? `${data.products.length} ${tr("active products available")}`
                              : tr("Add at least one product before the first sale"),
                          },
                        ]
                      : []),
                  ]}
                />
              )}</FrontLocalizedTree>;
}
