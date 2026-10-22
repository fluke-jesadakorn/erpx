"use client";
import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import { useState } from "react";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import type { OrganizationOverview } from "@erpx/feature-organization/lib/organization";
import type { SpendingException } from "@erpx/feature-shared/lib/spending";
import { useStaffCommand } from "@erpx/feature-work_projection/components/use-staff-command";
import { type UiAppearance } from "@erpx/ui/components/foundation";

export function SpendingTemplateEditor({ appearance = "legacy",
  base,
  data,
}: ({
  base: string;
  data: OrganizationOverview;
}) & { appearance?: UiAppearance }) {
  const template = data.spendingTemplate!;
  const [reviewing,setReviewing]=useState(false);
  const [exceptions, setExceptions] = useState(template.exceptions),
    { send, busy, error } = useStaffCommand(base);
  function update(index: number, patch: Partial<SpendingException>) {
    setExceptions((rows) => rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }
  return (
    <FormDisclosure appearance="standard" dismissDisabled={busy}><summary>Edit spending rules · version {template.version}</summary>
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h2 data-ui-appearance={appearance}>Company spending approval template · version {template.version}</h2>
      <p data-ui-appearance={appearance}>
        Requester → published department flow → Accounting preparation → independent Accounting
        approval (Level 1–4) → general ledger posting → Finance payment with bank slip.
      </p>
      <p data-ui-appearance={appearance} className="muted">
        Supervisor exceptions remain available for requests using older saved routes. Department
        flow changes apply to new submissions; existing routes require explicit repair.
      </p>
      <form onChangeCapture={()=>setReviewing(false)} data-ui-appearance={appearance}
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          if(!reviewing){setReviewing(true);return;}
          void send("people/organization/spending-template", {
            exceptions,
            expectedVersion: template.version,
            reason: f.get("reason"),
          });
        }}
      >
        {exceptions.map((row, i) => (
          <fieldset data-ui-appearance={appearance} key={i} className={appearance === "standard" ? "ui-form-grid" : "business-form"}>
            <legend data-ui-appearance={appearance}>Exception {i + 1}</legend>
            <Field appearance={appearance}>
              Position
              <Select appearance={appearance}
                value={row.positionId ?? ""}
                onChange={(e) => update(i, { positionId: e.target.value || null })}
              >
                <option data-ui-appearance={appearance} value="">All positions for selected request type</option>
                {data.positions
                  .filter((p) => p.active)
                  .map((p) => (
                    <option data-ui-appearance={appearance} key={p.id} value={p.id}>
                      {p.title}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance={appearance}>
              Request type
              <Select appearance={appearance}
                value={row.requestKind ?? ""}
                onChange={(e) =>
                  update(i, {
                    requestKind: (e.target.value || null) as SpendingException["requestKind"],
                  })
                }
              >
                <option data-ui-appearance={appearance} value="">All request types for selected position</option>
                <option data-ui-appearance={appearance} value="expense">Expense claim</option>
                <option data-ui-appearance={appearance} value="purchase">Purchase request</option>
                <option data-ui-appearance={appearance} value="payment">Payment request</option>
              </Select>
            </Field>
            <Field appearance={appearance}>
              Exception
              <Select appearance={appearance}
                value={row.action}
                onChange={(e) =>
                  update(i, { action: e.target.value as SpendingException["action"] })
                }
              >
                <option data-ui-appearance={appearance} value="named_supervisor">Use an explicit higher supervisor</option>
                <option data-ui-appearance={appearance} value="skip_supervisor">Skip organizational approval only</option>
              </Select>
            </Field>
            {row.action === "named_supervisor" && (
              <Field appearance={appearance}>
                Independent supervisor
                <Select appearance={appearance}
                  required
                  value={row.supervisorUserId ?? ""}
                  onChange={(e) => update(i, { supervisorUserId: e.target.value || null })}
                >
                  <option data-ui-appearance={appearance} value="">Select staff</option>
                  {data.people
                    .filter((p) => p.user_id && p.status === "active")
                    .map((p) => (
                      <option data-ui-appearance={appearance} key={p.engagement_id} value={p.user_id!}>
                        {p.legal_name} · Level {p.level_number ?? "unmapped"}
                      </option>
                    ))}
                </Select>
              </Field>
            )}
            <Field appearance={appearance}>
              Exception reason
              <TextInput appearance={appearance}
                required
                minLength={3}
                maxLength={1000}
                value={row.reason}
                onChange={(e) => update(i, { reason: e.target.value })}
              />
            </Field>
            <Button appearance={appearance}
              variant="secondary"
              type="button"
              onClick={() => setExceptions((rows) => rows.filter((_, j) => j !== i))}
            >
              Remove exception
            </Button>
          </fieldset>
        ))}
        <Button appearance={appearance}
          variant="secondary"
          type="button"
          onClick={() =>
            setExceptions((rows) => [
              ...rows,
              {
                positionId: null,
                requestKind: null,
                action: "named_supervisor",
                supervisorUserId: null,
                reason: "",
              },
            ])
          }
        >
          Add explicit exception
        </Button>
        <Field appearance={appearance}>
          Reason for this template revision
          <TextInput appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
        </Field>
        {reviewing && <section className="request-route" aria-label="Spending revision preview">
          <h3>Review version {template.version} → {template.version + 1}</h3>
          <p>{template.exceptions.length} previous exceptions → {exceptions.length} proposed exceptions. Existing submitted requests retain their saved route.</p>
          <ol>{exceptions.map((row,i)=><li key={i}>{data.positions.find(p=>p.id===row.positionId)?.title??"All positions"} · {row.requestKind??"All request types"} · {row.action==="named_supervisor"?data.people.find(p=>p.user_id===row.supervisorUserId)?.legal_name??"Choose an independent supervisor":"Skip organizational approval"} · {row.reason}</li>)}</ol>
          <p>Publication rechecks your authority, active supervisors, independence and the current version.</p>
          <Button appearance={appearance} type="button" onClick={()=>setReviewing(false)}>Back to details</Button>
        </section>}
        <div className="request-footer"><Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
          {reviewing?"Confirm template revision":"Review changes"}
        </Button></div>
        {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
      </form>
    </section>
    </FormDisclosure>
  );
}
