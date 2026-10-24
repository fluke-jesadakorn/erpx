"use client";
import dynamic from "next/dynamic";
import { Button as UiButton } from "@erpx/ui/components/foundation";
import { FeatureWidget as FeatureWidgetBase } from "@erpx/feature-shared/components/features/feature-page";
import type {PosRow} from '@erpx/server-pos/public/pos-core';
import { type ComponentProps, type FormEvent, type ReactNode } from "react";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
import type {BackofficePosConsoleContext} from "@erpx/feature-pos/components/standard-pos-backoffice-console";


const AttachmentPanel = dynamic(() => import("@erpx/feature-files/components/attachment-panel").then(module => module.AttachmentPanel));


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
export function BackofficePosConsoleSaleVoidScreen({context}: {context: BackofficePosConsoleContext & {sale: NonNullable<BackofficePosConsoleContext["sale"]>}}) {
  const {saleCommand, base, sale, busy, online} = context;
  return <FrontLocalizedTree>{(
                  <FeatureWidget
                    id="pos-void-evidence"
                    title="Cancel sale with evidence"
                    className={styles.panel}
                  >
                    <form
                      className={styles.form}
                      onSubmit={(event) => {
                        const form = fields(event),
                          setId = text(form, "attachmentSetId");
                        void saleCommand("sale.cancel", { attachmentSetIds: setId ? [setId] : [] });
                      }}
                    >
                      <AttachmentPanel
                        base={base}
                        targetType="pos_sale"
                        targetId={s(sale, "id")}
                        expectedTargetVersion={n(sale, "version")}
                        componentKind="action"
                        componentRef="void"
                        purposeKey="void_evidence"
                        title="Void evidence"
                        description="Optionally attach the supervisor note, bill image, or other evidence supporting this cancellation."
                        visibility="Visible to authorized POS colleagues in this Branch."
                        compact
                        fieldName="attachmentSetId"
                      />
                      <Button type="submit" secondary disabled={busy || !online}>
                        Cancel this sale
                      </Button>
                    </form>
                  </FeatureWidget>
                )}</FrontLocalizedTree>;
}
