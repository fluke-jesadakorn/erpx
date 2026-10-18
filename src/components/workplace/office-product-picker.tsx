"use client";

import { useState } from "react";
import { Button } from "@/components/ui/foundation";
import { Dialog } from "@/components/ui/dialog";
import styles from "./office-product-menus.module.css";

export type OfficeProductChoice = { href: string; label: string; detail?: string; current?: boolean };

export function OfficeProductPicker({ productLabel, workspaceLabel, products, workspaces, appsHref, accountHref, language }: {
  productLabel: string; workspaceLabel: string; products: OfficeProductChoice[]; workspaces: OfficeProductChoice[];
  appsHref: string; accountHref: string; language: "en" | "th";
}) {
  const [kind, setKind] = useState<"product" | "workspace" | null>(null), th = language === "th";
  const choices = kind === "product" ? products : workspaces;
  return <nav className={styles.menus} data-ui-appearance="standard" aria-label={th ? "ผลิตภัณฑ์และพื้นที่ทำงาน" : "Product and workspace"}>
    <Button appearance="standard" type="button" variant="ghost" clientManaged onClick={() => setKind("product")} aria-haspopup="dialog"><span className={styles.label}>{productLabel}</span></Button>
    <Button appearance="standard" type="button" variant="ghost" clientManaged onClick={() => setKind("workspace")} aria-haspopup="dialog"><span className={styles.label}>{workspaceLabel}</span></Button>
    <a data-product-switch className={styles.account} href={accountHref}>{th ? "บัญชี" : "Account"}</a>
    <Dialog appearance="standard" open={kind !== null} onClose={() => setKind(null)}
      title={kind === "product" ? th ? "เลือกผลิตภัณฑ์" : "Choose a product" : th ? "เลือกพื้นที่ทำงาน" : "Choose a workspace"}>
      <div className={styles.options}>
        {choices.map(choice => <a data-product-switch key={choice.href} href={choice.href} aria-current={choice.current ? "page" : undefined}>
          <strong>{choice.label}</strong>{choice.detail && <span>{choice.detail}</span>}
        </a>)}
        {!choices.length && <p>{th ? "ไม่มีพื้นที่ทำงานที่เปิดใช้ผลิตภัณฑ์นี้" : "No available product workspaces"}</p>}
        <a data-product-switch href={appsHref}>{th ? "ผลิตภัณฑ์และบริษัททั้งหมด" : "All products and Companies"}</a>
      </div>
    </Dialog>
  </nav>;
}
