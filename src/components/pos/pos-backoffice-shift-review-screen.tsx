"use client";
import dynamic from "next/dynamic";
import { Button as UiButton, Field, TextInput } from "@/components/ui/foundation";
import { FeatureWidget as FeatureWidgetBase } from "@/components/shared/features/feature-page";
import type { PosRow } from "@/server/modules/pos/public/pos-core";
import { type ComponentProps, type FormEvent, type ReactNode } from "react";
import styles from "./pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {BackofficePosConsoleContext} from "./pos-backoffice-console";


const AttachmentPanel = dynamic(() => import("@/components/files/attachment-panel").then(module => module.AttachmentPanel));


type Row = PosRow;


const s = (r: Row, k: string) => String(r[k] ?? "");


const n = (r: Row, k: string) => Number(r[k] ?? 0);


const fields = (e: FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  return new FormData(e.currentTarget);
};


const text = (f: FormData, k: string) => String(f.get(k) ?? "");


function FeatureWidget(props: ComponentProps<typeof FeatureWidgetBase>) {
  const language = useFrontLanguage();
  return <FeatureWidgetBase {...props} title={frontText(props.title, language)} />;
}


function Button({
  children,
  onClick,
  disabled,
  secondary = false,
  type = "button",
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  secondary?: boolean;
  type?: "button" | "submit";
}) {
  const language = useFrontLanguage();
  return (
    <UiButton
      variant="secondary"
      type={type}
      clientManaged={type === "submit"}
      onClick={onClick}
      disabled={disabled}
      className={`${styles.button} ${secondary ? styles.secondary : ""}`}
    >
      {typeof children === "string" ? frontText(children, language) : children}
    </UiButton>
  );
}
export function BackofficePosConsoleShiftReviewScreen({context}: {context: BackofficePosConsoleContext}) {
  const {data, language, send, base, busy, online} = context;
  return <FrontLocalizedTree>{data.shifts
                  .filter((item) => s(item, "state") === "closed")
                  .map((item) => (
                    <FeatureWidget
                      key={`variance-${s(item, "id")}`}
                      id={`pos-cash-variance-${s(item, "id")}`}
                      title={`Cash variance · ${s(item, "register_name")}`}
                      className={styles.panel}
                    >
                      {item.opened_by === data.userId || item.closed_by === data.userId ? (
                        <p role="status" className={styles.muted}>
                          {language === "th"
                            ? "ผู้เปิดหรือปิดกะต้องให้ผู้จัดการอีกคนตรวจยอดเงินสด"
                            : "A different manager must review this cash session because you opened or closed it."}
                        </p>
                      ) : (
                        <form
                          className={styles.form}
                          onSubmit={(event) => {
                            const form = fields(event),
                              setId = text(form, "attachmentSetId");
                            void send("shift.review", {
                              id: item.id,
                              expectedVersion: item.version,
                              reason: text(form, "reason"),
                              attachmentSetIds: setId ? [setId] : [],
                            });
                          }}
                        >
                          <Field>
                            Review result
                            <TextInput name="reason" required minLength={3} />
                          </Field>
                          <AttachmentPanel
                            base={base}
                            targetType="pos_shift"
                            targetId={s(item, "id")}
                            expectedTargetVersion={n(item, "version")}
                            componentKind="action"
                            componentRef="variance-review"
                            purposeKey="cash_variance"
                            title="Cash variance evidence"
                            description="Attach the count sheet, till photo, or investigation support reviewed for this variance."
                            visibility="Visible to authorized POS reviewers in this Branch."
                            compact
                            fieldName="attachmentSetId"
                          />
                          <Button type="submit" disabled={busy || !online}>
                            Review and record variance
                          </Button>
                        </form>
                      )}
                    </FeatureWidget>
                  ))}</FrontLocalizedTree>;
}
