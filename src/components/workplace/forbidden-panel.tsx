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
    <PlatformRoleSwitcher activeAssignmentId={platform.assignmentId} assignments={platform.assignments} />
  </div>;
}

/** Page failures retain the already-authorized workspace shell and its account menu. */
export async function ForbiddenPanel() {
  const from = await requestedPage();
  return <Card as="section" appearance="standard" className={styles.panel} aria-labelledby="workspace-forbidden-title" data-work-forbidden>
    <span className="eyebrow">Access required</span>
    <h1 id="workspace-forbidden-title">Your role cannot open this view</h1>
    <p>Your selected account and role do not have access to this page. Choose another role or account to continue here, or select an available workspace.</p>
    <PlatformRoleRecovery from={from} />
  </Card>;
}
