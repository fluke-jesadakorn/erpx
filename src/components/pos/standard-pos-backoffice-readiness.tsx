"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {Badge,Card} from "@/components/ui/foundation";
import Link from "@/components/workplace/workspace-link";
import {frontText} from "@/lib/shared/front-language";
import styles from "./standard-pos-backoffice-console.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import type {BackofficePosConsoleContext} from "./standard-pos-backoffice-console";

export function StandardPosBackofficeReadinessScreen({context}: {context: BackofficePosConsoleContext}) {
  const {tr, data, erpBase, register, shift, dailyPosRoot, currentTabLoaded} = context;
  return <FrontLocalizedTree>{(
                <StandardPosBackofficeReadiness
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


/** Readiness remains an owner projection; these links issue no setup command. */
export function StandardPosBackofficeReadiness({title,items}:{title:string;items:{id:string;label:string;ready:boolean;owner:string;href:string;detail?:string}[]}) {
  const language=useFrontLanguage(),tr=(value:string)=>frontText(value,language),missing=items.filter(item=>!item.ready).length;
  return <Card appearance="standard" aria-label={tr(title)} className={styles.readiness}>
    <header><h2>{tr(title)}</h2><Badge tone={missing?"warning":"success"}>{missing?(language==="th"?`เตรียม ${missing} รายการ`:`${missing} to prepare`):tr("Ready")}</Badge></header>
    <ul>{items.map(item=><li key={item.id}><span aria-hidden="true">{item.ready?"✓":"○"}</span><div><strong>{tr(item.label)}</strong><p>{tr(item.owner)}{item.detail?" · "+tr(item.detail):""}</p></div><Link href={item.href}>{tr(item.ready?"Review":"Set up")}</Link></li>)}</ul>
  </Card>;
}
