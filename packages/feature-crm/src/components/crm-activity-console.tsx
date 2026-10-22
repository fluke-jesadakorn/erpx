"use client";
import {CRMForm as FormLayout,CRMDisclosure as FormDisclosure,CRMDraftBoundary,CRMCommandState,CRMCommandFeedback,useCRMSnapshot} from "@erpx/feature-crm/components/draft-boundary";
import {useCRMCommand} from "@erpx/feature-crm/components/use-crm-command";
import {crmCommandAllowed} from "@erpx/feature-crm/lib/purchase-controls";

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import type { crmActivityWorkspace } from "@erpx/server-crm/public/crm-activities";
import { Card } from "@erpx/ui/components/foundation";
type Data = Awaited<ReturnType<typeof crmActivityWorkspace>>;
export function CRMActivityConsole({ base, data: currentData }: { base: string; data: Data }) {
  const {snapshot:data,rootProps}=useCRMSnapshot(currentData),purchase=currentData.purchase,active=purchase?.newWork!==false,settle=purchase?.settleExisting!==false;
  const command=useCRMCommand(base,'crm/activities',input=>{const body=input as Record<string,unknown>,item=currentData.items.find(row=>row.id===body.id);return crmCommandAllowed(purchase,input)&&(body.id?!!item&&(item.canComplete||item.canManage):currentData.canCreate);}),{send,busy}=command;
  const reason = (name = "reason") => (
    <TextArea appearance="standard" name={name} minLength={3} maxLength={2000} required />
  );
  return (
    <CRMCommandState {...command}><CRMDraftBoundary><div {...rootProps}><Card as="section" appearance="standard" className="ui-panel" id="crm-activities">
      <h2>Lead activities</h2>
      <p>
        Schedule the next customer action, assign its owner and record the outcome here. Activities
        remain internal CRM evidence.
      </p>
      <CRMCommandFeedback/>
      {data.canCreate && (
        <FormLayout editorTitle="Schedule lead activity" sourceVersion={JSON.stringify([currentData.lead.version,currentData.recipients.map(row=>row.id)])} allowed={active&&currentData.canCreate}
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send({
              leadId: data.lead.id,
              expectedVersion: data.lead.version,
              activityType: f.get("activityType"),
              subject: f.get("subject"),
              details: f.get("details"),
              ownerId: f.get("ownerId"),
              dueDate: f.get("dueDate"),
            });
          }}
        >
          <Field appearance="standard">
            Activity type
            <Select appearance="standard" name="activityType">
              <option value="call">Call</option>
              <option value="email">Email</option>
              <option value="meeting">Meeting</option>
              <option value="task">Task</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Subject
            <TextInput appearance="standard" name="subject" minLength={3} maxLength={200} required />
          </Field>
          <label>Next action details{reason("details")}</label>
          <Field appearance="standard">
            Responsible salesperson
            <Select appearance="standard" name="ownerId" required defaultValue="">
              <option value="">Choose an eligible salesperson</option>
              {data.recipients.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Due date
            <TextInput appearance="standard" name="dueDate" type="date" required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !data.recipients.length}>
            Schedule activity
          </Button>
          {!data.recipients.length && (
            <p>
              No eligible salesperson is available. Ask a sales manager to restore role coverage.
            </p>
          )}
        </FormLayout>
      )}
      {data.items.map((item) => (
        <Card as="article" appearance="standard" className="ui-panel" key={item.id}>
          <h3>
            {item.activity_type} · {item.subject}
          </h3>
          <p>
            {item.status} · Owner {String(item.owner_name ?? "Unavailable")} · Due {item.due_date}
          </p>
          <p style={{ whiteSpace: "pre-wrap" }}>{item.details}</p>
          {item.outcome && <p style={{ whiteSpace: "pre-wrap" }}>Outcome: {item.outcome}</p>}
          {(item.canComplete || item.canManage) && (
            <FormDisclosure appearance="standard">
              <summary>Record activity decision</summary>
              <FormLayout sourceVersion={currentData.items.find(row=>row.id===item.id)?.version} allowed={settle&&!!currentData.items.find(row=>row.id===item.id&&(row.canComplete||row.canManage))}
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget),
                    action = String(f.get("action"));
                  void send({
                    id: item.id,
                    action,
                    expectedVersion: Number(item.version),
                    ownerId: f.get("ownerId"),
                    dueDate: f.get("dueDate"),
                    reason: f.get("reason"),
                  });
                }}
              >
                <Field appearance="standard">
                  Action
                  <Select appearance="standard" name="action">
                    {item.canComplete && (
                      <>
                        <option value="complete">Complete activity</option>
                        <option value="cancel">Cancel activity</option>
                      </>
                    )}
                    {item.canManage && <option value="reassign">Reassign activity</option>}
                  </Select>
                </Field>
                {item.canManage && (
                  <>
                    <Field appearance="standard">
                      Responsible salesperson
                      <Select appearance="standard" name="ownerId" defaultValue={String(item.owner_id)}>
                        {data.recipients.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.name}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Due date
                      <TextInput appearance="standard" name="dueDate" type="date" defaultValue={String(item.due_date)} />
                    </Field>
                  </>
                )}
                {item.canComplete && <label>Outcome and reason{reason()}</label>}
                {item.canManage && !item.canComplete && <label>Management reason{reason()}</label>}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Save activity decision
                </Button>
              </FormLayout>
            </FormDisclosure>
          )}
          <details>
            <summary>Activity history</summary>
            {data.history
              .filter((h) => h.activity_id === item.id)
              .map((h) => (
                <div key={String(h.id)}>
                  <p>
                    Version {String(h.version)} · {String(h.action)} · {String(h.actor_id)} ·{" "}
                    {String(h.note)}
                  </p>
                </div>
              ))}
          </details>
        </Card>
      ))}
      {!data.items.length && <p>No CRM activities yet.</p>}
    </Card></div></CRMDraftBoundary></CRMCommandState>
  );
}
