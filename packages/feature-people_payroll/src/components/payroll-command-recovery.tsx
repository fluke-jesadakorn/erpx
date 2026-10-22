"use client";
import {Button, type UiAppearance} from "@erpx/ui/components/foundation";
import {useAccountPreferences} from "@erpx/feature-identity_access/components/account-preferences-context";

export function PayrollCommandRecovery({pending, busy, retry, appearance = "legacy"}: {pending: boolean; busy: boolean; retry: () => Promise<boolean>; appearance?: UiAppearance}) {
  const th = useAccountPreferences()?.preference?.language === "th";
  if (!pending) return null;
  return <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-stack" : "content-stack"} role="status">
    <p data-ui-appearance={appearance}>{th ? "ผลคำขอเงินเดือนเดิมยังไม่ได้รับการยืนยัน เก็บแบบร่างใหม่ไว้และกู้คืนคำขอเดิมก่อน" : "The original Payroll result is not confirmed. Keep your newer draft and recover the original request first."}</p>
    <Button appearance={appearance} type="button" variant="secondary" disabled={busy} onClick={() => void retry()}>{th ? "กู้คืนคำขอเงินเดือนเดิม" : "Recover original Payroll request"}</Button>
  </div>;
}
