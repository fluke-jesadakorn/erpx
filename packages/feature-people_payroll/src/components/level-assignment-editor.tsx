"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import { useStaffCommand, type StaffOptions } from "@erpx/feature-work_projection/components/staff-work-controls";
import { FormLayout } from "@erpx/ui/components/foundation";
export function LevelAssignmentEditor({
  base,
  engagementId,
  version,
  levelId,
  levels,
}: {
  base: string;
  engagementId: string;
  version: number;
  levelId: string | null;
  levels: StaffOptions["levels"];
}) {
  const { send, busy, error } = useStaffCommand(base);
  return (
    <details className="ui-panel">
      <summary>Change staff level</summary>
      <FormLayout
        className="ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const data = new FormData(e.currentTarget);
          void send(`staff/engagements/${engagementId}/level`, {
            levelId: data.get("levelId"),
            expectedVersion: version,
            reason: data.get("reason"),
          });
        }}
      >
        <Field appearance="standard">
          New level
          <Select appearance="standard" name="levelId" defaultValue={levelId ?? ""} required>
            <option value="">Choose a level</option>
            {levels
              .filter((l) => l.active)
              .map((l) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Change reason
          <TextInput appearance="standard" name="reason" required minLength={3} maxLength={1000} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          Record level change
        </Button>
        {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      </FormLayout>
    </details>
  );
}
