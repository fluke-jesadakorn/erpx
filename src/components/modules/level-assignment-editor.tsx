"use client";
import { useStaffCommand, type StaffOptions } from "./staff-work-controls";
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
    <details className="card">
      <summary>Change staff level</summary>
      <form
        className="business-form"
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
        <label>
          New level
          <select name="levelId" defaultValue={levelId ?? ""} required>
            <option value="">Choose a level</option>
            {levels
              .filter((l) => l.active)
              .map((l) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
          </select>
        </label>
        <label>
          Change reason
          <input name="reason" required minLength={3} maxLength={1000} />
        </label>
        <button className="button" disabled={busy}>
          Record level change
        </button>
        {error && <p role="alert">{error}</p>}
      </form>
    </details>
  );
}
