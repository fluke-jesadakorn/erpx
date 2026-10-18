import {platformRoleDisplayChoices} from "@/lib/identity_access/work-role-display";
import "server-only";

import { Card } from "@/components/ui/card";
import { requirePlatformContext } from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { accessUnavailable, requestedPage } from "./forbidden-context";
import { PlatformRoleSwitcher } from "./platform-role-switcher-loader";
import styles from "./forbidden-recovery.module.css";

async function PlatformRoleRecovery({ from }: { from: string | null }) {
  if (!from || !/^\/holdings\/erpx\/manage-erpx(?:[/?#]|$)/.test(from)) return null;
  const platform = await requirePlatformContext().catch(error => {
    if (accessUnavailable(error)) return null;
    return pageError(error, "erpx");
  });
  if (!platform) return null;
  return <div className={styles.platformRoles}>
    <PlatformRoleSwitcher activeAssignmentId={platform.assignmentId} assignments={platformRoleDisplayChoices(platform.assignments)} />
  </div>;
}

/** Page failures retain the already-authorized workspace shell and its account menu. */
export async function ForbiddenPanel({ productLabel, language = "en" }: { productLabel?: string; language?: "en" | "th" } = {}) {
  const from = await requestedPage();
  const th = language === "th";
  return <Card as="section" appearance="standard" className={styles.panel} aria-labelledby="workspace-forbidden-title" data-work-forbidden>
    <span className="eyebrow">{productLabel ?? "Access required"}</span>
    <h1 id="workspace-forbidden-title">{productLabel ? th ? "ยังเปิดหน้านี้ไม่ได้" : "This view is unavailable" : "Your role cannot open this view"}</h1>
    <p>{productLabel ? th ? "หน้านี้ต้องใช้แพ็กเกจบริษัทที่พร้อมใช้งานและบัญชีพร้อมบทบาทที่มีสิทธิ์ เปิด Apps เพื่อเลือกบริษัทและผลิตภัณฑ์ที่เข้าได้ หรือเปิด Account เพื่อเปลี่ยนบัญชี" : "This page requires an available Company package and an authorized account and role. Open Apps to choose an available Company and product, or open Account to switch accounts."
      : "Your selected account and role do not have access to this page. Choose another role or account to continue here, or select an available workspace."}</p>
    {!productLabel && <PlatformRoleRecovery from={from} />}
  </Card>;
}
