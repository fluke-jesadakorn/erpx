"use client";
import {useState} from 'react';
import {Button,Field,Select,TextInput,type UiAppearance} from '@/components/ui/foundation';
import {formatUiDateTime} from '@/lib/shared/ui-format';
import {PreferredDateTimeField} from '@/components/people_payroll/time/preferred-date-time-field';
import type {TemplateRow,ScheduleRow} from '@/server/modules/operations/public/routines';
import type {RoutinePurchases} from '@/server/modules/operations/public/rules/routine-purchase-controls';
import {useRoutineCommand} from './use-routine-command';
import {RoutineCommandFeedback} from './routine-command-feedback';
import {RoutineEditor} from './routine-editor';
export function RoutineSchedules({ appearance = "legacy",
  base,
  template,
  schedules,
  canEdit,
  canRun=true,
  purchases,
}: ({
  base: string;
  template: TemplateRow;
  schedules: ScheduleRow[];
  canEdit: boolean;
  canRun?: boolean;
  purchases?:RoutinePurchases;
}) & { appearance?: UiAppearance }) {
  const purchase=template.project_id?purchases?.projects:purchases?.work,command=useRoutineCommand(base,purchase,purchases?.context??base,attempt=>attempt.path.endsWith('/run')?canRun:canEdit),{busy,error}=command,active=!purchase||purchase.newWork,settle=!purchase||purchase.settleExisting;
  const send=command.send,
    [frequency, setFrequency] = useState("daily"),
    [versions,setVersions]=useState<Record<string,number>>({}),
    [createdSchedule,setCreatedSchedule]=useState<{id:string;version:number}|null>(null),
    [defaultDueInstant] = useState(() => Date.now() + 86400000);
  function scheduleForm(schedule?: ScheduleRow) {
    return (
      <form data-routine-draft data-ui-appearance={appearance}
        key={schedule?.id ?? "new"}
        className={appearance === "standard" ? "ui-form-grid" : "business-form"}
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget),target=schedule??createdSchedule;
          void send(
            `routine-templates/${template.id}/schedules${target ? `/${target.id}` : ""}`,
            {
              frequency: f.get("frequency"),
              timezone: f.get("timezone"),
              occurrenceTime: f.get("occurrenceTime"),
              startDate: f.get("startDate"),
              endDate: f.get("endDate") || null,
              weekday: f.get("weekday") === "" ? null : Number(f.get("weekday")),
              monthDay: f.get("monthDay") === "" ? null : Number(f.get("monthDay")),
              dueOffsetHours: Number(f.get("dueOffsetHours")),
              expectedVersion: target?Math.max(target.version,versions[target.id]??0):1,
            },
            e.currentTarget,
            false,
            undefined,
            result=>{setVersions(values=>({...values,[result.id]:result.version}));if(!schedule)setCreatedSchedule({id:result.id,version:result.version});},
          );
        }}
      >
        <Field appearance={appearance}>
          Frequency
          <Select appearance={appearance}
            name="frequency"
            defaultValue={schedule?.frequency ?? frequency}
            onChange={(e) => setFrequency(e.target.value)}
          >
            <option data-ui-appearance={appearance} value="daily">Daily</option>
            <option data-ui-appearance={appearance} value="weekly">Weekly</option>
            <option data-ui-appearance={appearance} value="monthly">Monthly</option>
          </Select>
        </Field>
        <Field appearance={appearance}>
          Timezone
          <TextInput appearance={appearance} name="timezone" defaultValue={schedule?.timezone ?? "Asia/Bangkok"} required />
        </Field>
        <Field appearance={appearance}>
          Occurrence time
          <TextInput appearance={appearance}
            name="occurrenceTime"
            type="time"
            defaultValue={schedule?.occurrence_time ?? "09:00"}
            required
          />
        </Field>
        <Field appearance={appearance}>
          Start date
          <TextInput appearance={appearance}
            name="startDate"
            type="date"
            required
            defaultValue={schedule?.start_date ?? new Date().toISOString().slice(0, 10)}
          />
        </Field>
        <Field appearance={appearance}>
          End date
          <TextInput appearance={appearance} name="endDate" type="date" defaultValue={schedule?.end_date ?? ""} />
        </Field>
        <Field appearance={appearance}>
          Weekly weekday
          <Select appearance={appearance} name="weekday" defaultValue={schedule?.weekday ?? ""}>
            <option data-ui-appearance={appearance} value="">Use start weekday</option>
            {["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"].map(
              (day, i) => (
                <option data-ui-appearance={appearance} key={day} value={i}>
                  {day}
                </option>
              ),
            )}
          </Select>
        </Field>
        <Field appearance={appearance}>
          Monthly day
          <TextInput appearance={appearance}
            name="monthDay"
            type="number"
            min={1}
            max={31}
            defaultValue={schedule?.month_day ?? ""}
            placeholder="Use start day"
          />
        </Field>
        <Field appearance={appearance}>
          Due offset in hours
          <TextInput appearance={appearance}
            name="dueOffsetHours"
            type="number"
            required
            min={0}
            max={8760}
            defaultValue={schedule?.due_offset_hours ?? 24}
          />
        </Field>
        <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy||!active}>
          Save paused schedule
        </Button>
      </form>
    );
  }
  return (
    <>
      <RoutineCommandFeedback command={command}/>
      <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <h2 data-ui-appearance={appearance}>Run routine now</h2>
        <RoutineEditor title="Create routine occurrence" appearance={appearance} busy={busy} error={error} recovery={<RoutineCommandFeedback command={command}/>}><form data-routine-draft data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(e) => {
            e.preventDefault();
            void send(
              `routine-templates/${template.id}/run`,
              {
                dueAt: String(new FormData(e.currentTarget).get("dueAt")),
              },
              e.currentTarget,
              false,
              "staff-tasks",
            );
          }}
        >
          <PreferredDateTimeField appearance={appearance}
            name="dueAt"
            label="Task due date and time"
            required
            initialInstant={defaultDueInstant}
          />
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy || !template.published||!active||!canRun}>
            Create occurrence
          </Button>
        </form></RoutineEditor>
      </section>
      <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <h2 data-ui-appearance={appearance}>Schedules</h2>
        <p data-ui-appearance={appearance} className="muted">
          Each occurrence creates independent work. Monthly days beyond month-end use the last day.
        </p>
        {schedules.map((s) => (
          <article data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"} key={s.id}>
            <p data-ui-appearance={appearance}>
              <strong data-ui-appearance={appearance}>
                {s.frequency} at {s.occurrence_time} · {s.timezone}
              </strong>{" "}
              · {s.state}
            </p>
            <p data-ui-appearance={appearance}>
              Next occurrence:{" "}
              {s.next_occurrence_at ? formatUiDateTime(s.next_occurrence_at, s.timezone) : "None"}
            </p>
            {s.last_error && <p data-ui-appearance={appearance} role="alert">Generation failed: {s.last_error}</p>}
            {canEdit && s.state !== "ended" && (
              <>
                <Button appearance={appearance}
                  clientManaged type="submit"
                  variant="secondary"

                  disabled={busy||(s.state==="active"?!settle:!active)}
                  onClick={() =>
                    void send(
                      `routine-schedules/${s.id}/${s.state === "active" ? "pause" : "resume"}`,
                      { expectedVersion: s.version },
                      undefined,
                      s.state==="active",
                    )
                  }
                >
                  {s.state === "active" ? "Pause schedule" : "Resume schedule"}
                </Button>
                <Button appearance={appearance} clientManaged variant="secondary" disabled={busy||!settle} onClick={()=>void send(`routine-schedules/${s.id}/end`,{expectedVersion:s.version},undefined,true)}>End schedule</Button>
                {s.state === "paused" && active && <RoutineEditor title="Edit paused schedule" appearance={appearance} busy={busy} error={error} onReset={()=>setFrequency('daily')} recovery={<RoutineCommandFeedback command={command}/>}>{scheduleForm(s)}</RoutineEditor>}
              </>
            )}
          </article>
        ))}
        {canEdit && active && (
          <RoutineEditor title="Add schedule" appearance={appearance} busy={busy} error={error} onReset={()=>{setFrequency("daily");setCreatedSchedule(null);}} recovery={<RoutineCommandFeedback command={command}/>}>
            <p data-ui-appearance={appearance}>
              Schedules are saved paused. Resume after reviewing assignment and approval
              configuration.
            </p>
            {scheduleForm()}
          </RoutineEditor>
        )}
        {canEdit&&template.published===1&&<Button appearance={appearance} clientManaged variant="secondary" disabled={busy||!settle} onClick={()=>void send(`routine-templates/${template.id}`,{title:template.title,description:template.description,assigneeUserId:template.assignee_user_id,projectId:template.project_id,startApprovers:JSON.parse(template.start_approvers_json),completionApprovers:JSON.parse(template.completion_approvers_json),published:false,expectedVersion:template.version},undefined,true)}>Unpublish routine</Button>}
        {purchase&&!active&&<p className="ui-muted">This retained routine is available for history and existing work. New occurrences and configuration require an active package.</p>}
        {error && (
          <p data-ui-appearance={appearance} className="error" role="alert">
            {error}
          </p>
        )}
      </section>
    </>
  );
}
