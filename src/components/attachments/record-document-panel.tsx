"use client";

import { useState } from "react";
import { AttachmentPanel } from "./attachment-panel";
import { useAttachmentFeatureMode } from "./attachment-feature-context";

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

export function RecordDocumentPanel(props: RecordDocumentPanelProps) {
  const mode = useAttachmentFeatureMode();
  if (mode === "disabled") return null;
  return (
    <RecordDocumentPanelContent
      {...props}
      editable={props.editable !== false && mode !== "read-only"}
    />
  );
}

function RecordDocumentPanelContent({
  base,
  targetType,
  targetId,
  expectedTargetVersion,
  title = "Record documents",
  purposes,
  editable = true,
}: RecordDocumentPanelProps) {
  const [purposeKey, setPurposeKey] = useState(purposes[0]?.key ?? "");
  const [available, setAvailable] = useState<boolean | null>(null);
  const purpose = purposes.find((item) => item.key === purposeKey) ?? purposes[0];
  if (!purpose || available === false) return null;
  return (
    <section className="card business-form">
      <h2>{title}</h2>
      <label>
        Document purpose
        <select value={purpose.key} onChange={(event) => setPurposeKey(event.target.value)}>
          {purposes.map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}
        </select>
      </label>
      <AttachmentPanel
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
