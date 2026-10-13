"use client";
import { Field, Select } from "@/components/ui/foundation";

import { useState } from "react";
import { AttachmentPanel } from "@/components/files/attachment-panel";
import { useAttachmentFeatureMode } from "@/components/files/attachment-feature-context";
import { type UiAppearance } from "@/components/ui/foundation";

export type RecordDocumentPurpose = {
  key: string;
  label: string;
  description: string;
  visibility?: string;
  editable?: boolean;
};

type RecordDocumentPanelProps = {
  base: string;
  targetType: string;
  targetId: string;
  expectedTargetVersion: number;
  title?: string;
  purposes: readonly RecordDocumentPurpose[];
  editable?: boolean;
};

export function RecordDocumentPanel(props: (RecordDocumentPanelProps) & { appearance?: UiAppearance }) {
  const mode = useAttachmentFeatureMode();
  if (mode === "disabled") return null;
  return (
    <RecordDocumentPanelContent appearance={(props.appearance ?? "legacy")}
      {...props}
      editable={props.editable !== false && mode !== "read-only"}
    />
  );
}

function RecordDocumentPanelContent({ appearance = "legacy",
  base,
  targetType,
  targetId,
  expectedTargetVersion,
  title = "Record documents",
  purposes,
  editable = true,
}: (RecordDocumentPanelProps) & { appearance?: UiAppearance }) {
  const [purposeKey, setPurposeKey] = useState(purposes[0]?.key ?? "");
  const [available, setAvailable] = useState<boolean | null>(null);
  const purpose = purposes.find((item) => item.key === purposeKey) ?? purposes[0];
  if (!purpose || available === false) return null;
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel ui-form-grid" : "card business-form"}>
      <h2 data-ui-appearance={appearance}>{title}</h2>
      <Field appearance={appearance}>
        Document purpose
        <Select appearance={appearance} value={purpose.key} onChange={(event) => setPurposeKey(event.target.value)}>
          {purposes.map((item) => (
            <option data-ui-appearance={appearance} key={item.key} value={item.key}>
              {item.label}
            </option>
          ))}
        </Select>
      </Field>
      <AttachmentPanel appearance={appearance}
        key={`${targetType}:${targetId}:${expectedTargetVersion}:${purpose.key}`}
        base={base}
        targetType={targetType}
        targetId={targetId}
        expectedTargetVersion={expectedTargetVersion}
        componentKind="record"
        purposeKey={purpose.key}
        title={purpose.label}
        description={purpose.description}
        visibility={purpose.visibility}
        bindMode="record"
        readOnly={!editable || purpose.editable === false}
        showHistory
        onAvailabilityChange={setAvailable}
      />
    </section>
  );
}
