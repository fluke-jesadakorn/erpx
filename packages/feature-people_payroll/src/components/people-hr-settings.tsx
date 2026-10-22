"use client";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { hrSettingsTopics } from "@erpx/feature-people_payroll/lib/hr-settings-navigation";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { useRouter } from '@erpx/feature-shared/lib/destination-router';
import { ArrowLeft, ChevronRight } from "lucide-react";
import { createContext, useContext, useState, type FormEvent, type ReactNode } from "react";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { WorkspaceFrame, WorkspaceNavigation, WorkspaceSections, WorkspaceContent, WorkspaceContext } from "@erpx/ui/components/workspace";
import { FormDrawer } from "@erpx/ui/components/dialog";
import { LeavePolicyWizard } from "@erpx/feature-people_payroll/components/self-service/leave-policy-wizard";
import { staffLevelTitles, type OrganizationTemplatePosition } from "@erpx/feature-organization/lib/organization";
import type { HoldingPeopleTemplate } from "@erpx/feature-people_payroll/lib/people-templates";
import type { PeopleHrTopic, peopleHrSettingsData } from "@erpx/server-people_payroll/public/people-hr-settings";
import { usePeopleIntegrationCommand, usePeopleIntegrationData } from "@erpx/feature-people_payroll/components/people-integration-data";
import styles from "@erpx/feature-people_payroll/components/people-hr-settings.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";

type Payload = Awaited<ReturnType<typeof peopleHrSettingsData>>;
type LevelsData = Extract<Payload, { levels: unknown }>;
type LeaveData = Extract<Payload, { types: unknown }>;
type GroupsData = Extract<Payload, { groups: unknown }>;
type TemplatesData = Extract<Payload, { templates: unknown }>;
type LifecycleData = Extract<Payload, { settings: unknown }>;
type Send = ReturnType<typeof usePeopleIntegrationCommand>["send"];

function fields(event: FormEvent<HTMLFormElement>) {
  event.preventDefault();
  return new FormData(event.currentTarget);
}
const value = (form: FormData, key: string) => String(form.get(key) ?? "");
const lines = (form: FormData, key: string) =>
  value(form, key)
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
const HrFeedback = createContext({error: "", message: ""});
function InlineEditor({ title, children }: { title: string; children: ReactNode; appearance?: UiAppearance }) {
  const feedback = useContext(HrFeedback);
  return <FormDisclosure appearance="standard" className={styles.row} drawerTitle={title} contentClassName={styles.form} error={feedback.error}>
    <summary><span>{title}</span><ChevronRight size={16} aria-hidden="true"/></summary>{feedback.message && <p role="status">{feedback.message}</p>}{children}
  </FormDisclosure>;
}

export function PeopleHrSettings({ appearance = "standard",
  standalone = false,
  base,
  initialTopic,
  initialAction,
  initialIndex,
  initialData,
  onTopicChange,
  onClose,
}: ({
  standalone?: boolean;
  base: string;
  initialTopic?: string;
  initialAction?: string;
  initialIndex?: {topics:PeopleHrTopic[]};
  initialData?: Payload;
  onTopicChange?: (topic: string) => void;
  onClose?: () => void;
}) & { appearance?: UiAppearance }) {
  const searchParams = useSearchParams();
  const router=useRouter();
  const requestedTopic = standalone ? searchParams.get("section") ?? initialTopic : initialTopic;
  const requestedAction = standalone ? searchParams.get("editor") ?? (searchParams.get("new") === "leave-policy" ? "policy" : undefined) : initialAction;
  const urlTopic = hrSettingsTopics.some((item) => item.id === requestedTopic)
    ? (requestedTopic as PeopleHrTopic)
    : "";
  const [localTopic, setLocalTopic] = useState<PeopleHrTopic | "">(urlTopic);
  const chosenTopic = onTopicChange || standalone ? urlTopic : localTopic;
  const index = usePeopleIntegrationData<{ topics: PeopleHrTopic[] }>(
    `/api/v1${base}/people/hr-settings`,
    undefined, initialIndex,
  );
  function choose(next: PeopleHrTopic | "") {
    setLocalTopic(next);
    if (standalone) {
      const url = new URL(window.location.href);
      if(next) url.searchParams.set("section", next); else url.searchParams.delete("section");
      url.searchParams.delete("editor"); url.searchParams.delete("new");
      router.push(url.pathname+url.search,{scroll:false});
    }
    onTopicChange?.(next);
  }
  const available = hrSettingsTopics.filter(item => index.data?.topics.includes(item.id));
  const topic = chosenTopic || (standalone ? available[0]?.id : "") || "";
  if (standalone) return <WorkspaceFrame data-testid="hr-settings-workspace">
    <WorkspaceNavigation title="HR settings" label={available.find(item => item.id === topic)?.title} aria-label="HR settings navigation">
      <WorkspaceSections items={available} selected={topic} onSelect={id => choose(id as PeopleHrTopic)} label="HR settings topics" />
    </WorkspaceNavigation>
    <WorkspaceContent>
      <WorkspaceContext label="Company administration"><strong>HR settings</strong></WorkspaceContext>
      {index.loading && <p role="status">Loading available settings…</p>}
      {index.error && <p role="alert">{index.error}<Button appearance="standard" type="button" onClick={() => void index.reload()}>Retry</Button></p>}
      {topic && available.some(item => item.id === topic) && <>
        <h2>{available.find(item => item.id === topic)?.title}</h2>
        <p className={styles.note}>{available.find(item => item.id === topic)?.description}</p>
        <HrTopic appearance="standard" key={`${topic}:${requestedAction ?? ""}`} base={base} topic={topic} initialData={initialData} initialAction={requestedAction} showHeading={false}/>
      </>}
      {index.data && (!available.length || (topic && !available.some(item => item.id === topic))) && <p role="alert">Your selected role does not manage this Company HR setting.</p>}
    </WorkspaceContent>
  </WorkspaceFrame>;
  return (
    <div data-ui-appearance={appearance} className={styles.settings} aria-label="HR settings">
      {onClose ? (
        <p data-ui-appearance={appearance} className={styles.note}>Company policies and defaults</p>
      ) : (
        <header data-ui-appearance={appearance} className={styles.heading}>
          <div data-ui-appearance={appearance}>
            <h2 data-ui-appearance={appearance}>HR settings</h2>
            <p data-ui-appearance={appearance}>Company policies and defaults</p>
          </div>
        </header>
      )}
      {topic ? (
        <>
          <Button appearance={appearance} type="submit" variant="plain" className={styles.back} onClick={() => choose("")}>
            <ArrowLeft size={16} />
            All HR settings
          </Button>
          <HrTopic appearance={appearance}
            key={`${topic}:${requestedAction ?? ""}`}
            base={base}
            topic={topic}
            initialAction={requestedAction}
          />
        </>
      ) : (
        <>
          {index.loading && (
            <p data-ui-appearance={appearance} className={styles.loading} role="status">
              Loading available settings…
            </p>
          )}
          {index.error && (
            <p data-ui-appearance={appearance} className={styles.error} role="alert">
              {index.error}
              <Button appearance={appearance}
                type="submit"
                variant="primary"
                className={styles.button}
                onClick={() => void index.reload()}
              >
                Retry
              </Button>
            </p>
          )}
          {index.data && (
            <nav data-ui-appearance={appearance} className={styles.topics} aria-label="HR settings topics">
              {hrSettingsTopics
                .filter((item) => index.data!.topics.includes(item.id))
                .map((item) => (
                  <Button appearance={appearance}
                    type="submit"
                    variant="plain"
                    key={item.id}
                    onClick={() => choose(item.id)}
                  >
                    <span data-ui-appearance={appearance}>
                      <strong data-ui-appearance={appearance}>{item.title}</strong>
                      <small data-ui-appearance={appearance}>{item.description}</small>
                    </span>
                    <ChevronRight size={16} />
                  </Button>
                ))}
            </nav>
          )}
          {index.data && !index.data.topics.length && (
            <p data-ui-appearance={appearance} className={styles.note}>Your selected role does not manage Company HR settings.</p>
          )}
        </>
      )}
    </div>
  );
}

function HrTopic({ appearance = "legacy",
  base,
  topic,
  initialAction,
  initialData,
  showHeading = true,
}: ({
  showHeading?: boolean;
  base: string;
  topic: PeopleHrTopic;
  initialAction?: string;
  initialData?: Payload;
}) & { appearance?: UiAppearance }) {
  const resource = usePeopleIntegrationData<Payload>(
    `/api/v1${base}/people/hr-settings?topic=${topic}`,
    undefined, initialData,
  );
  const command = usePeopleIntegrationCommand(base, resource.reload);
  return (
    <HrFeedback.Provider value={{error:command.error,message:command.message}}><section data-ui-appearance={appearance}
      className={styles.topic}
      aria-label={hrSettingsTopics.find((item) => item.id === topic)?.title}
      aria-busy={resource.loading || command.busy}
    >
      {showHeading && <h3 data-ui-appearance={appearance}>{hrSettingsTopics.find((item) => item.id === topic)?.title}</h3>}
      {resource.loading && !resource.data && (
        <p data-ui-appearance={appearance} className={styles.loading} role="status">
          Loading settings…
        </p>
      )}
      {resource.error && (
        <p data-ui-appearance={appearance} className={styles.error} role="alert">
          {resource.error}
          <Button appearance={appearance}
            type="submit"
            variant="primary"
            className={styles.button}
            onClick={() => void resource.reload()}
          >
            Retry
          </Button>
        </p>
      )}
      {command.error && (
        <p data-ui-appearance={appearance} className={styles.error} role="alert">
          {command.error}
        </p>
      )}
      {command.message && (
        <p data-ui-appearance={appearance} className={styles.message} role="status">
          {command.message}
        </p>
      )}
      {resource.data && topic === "levels" && (
        <Levels appearance={appearance} data={resource.data as LevelsData} send={command.send} busy={command.busy} />
      )}
      {resource.data && topic === "leave" && (
        <LeaveSettings appearance={appearance}
          base={base}
          data={resource.data as LeaveData}
          send={command.send}
          busy={command.busy}
          reload={resource.reload}
          initialAction={initialAction}
        />
      )}
      {resource.data && topic === "calendars" && (
        <Calendars appearance={appearance} data={resource.data as LeaveData} send={command.send} busy={command.busy} />
      )}
      {resource.data && topic === "pay-groups" && (
        <PayGroups appearance={appearance} data={resource.data as GroupsData} send={command.send} busy={command.busy} />
      )}
      {resource.data && topic === "templates" && (
        <Templates appearance={appearance} data={resource.data as TemplatesData} send={command.send} busy={command.busy} />
      )}
      {resource.data && topic === "lifecycle" && (
        <Lifecycle appearance={appearance} data={resource.data as LifecycleData} send={command.send} busy={command.busy} />
      )}
    </section></HrFeedback.Provider>
  );
}

function Levels({ appearance = "legacy", data, send, busy }: ({ data: LevelsData; send: Send; busy: boolean }) & { appearance?: UiAppearance }) {
  return (
    <div data-ui-appearance={appearance} className={styles.stack}>
      <p data-ui-appearance={appearance} className={styles.note}>
        Level 1 is most senior; Level 6 is staff. Levels organize work and keep their existing
        approval rules.
      </p>
      {data.levels.map((level) => (
        <InlineEditor appearance={appearance}
          key={`${level.id}:${level.version}`}
          title={`${level.name} · ${level.level_number ? `Level ${level.level_number}` : level.code}`}
        >
          <form data-ui-appearance={appearance}
            className={styles.form}
            onSubmit={(event) => {
              const f = fields(event);
              void send(`staff/levels/${level.id}`, {
                code: value(f, "code"),
                name: value(f, "name"),
                rank: Number(f.get("rank")),
                ...(level.level_number ? { levelNumber: level.level_number } : {}),
                active: f.has("active"),
                expectedVersion: level.version,
              });
            }}
          >
            <Field appearance={appearance}>
              Code
              <TextInput appearance={appearance} name="code" required maxLength={30} defaultValue={level.code} />
            </Field>
            <Field appearance={appearance}>
              Name
              <TextInput appearance={appearance} name="name" required maxLength={100} defaultValue={level.name} />
            </Field>
            <TextInput appearance={appearance} name="rank" type="hidden" value={level.rank} />
            <Field appearance={appearance} orientation="inline" className={styles.check}>
              <TextInput appearance={appearance} type="checkbox" name="active" defaultChecked={!!level.active} />
              Active level
            </Field>
            <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
              Save level
            </Button>
          </form>
          {level.level_number && (
            <form data-ui-appearance={appearance}
              className={styles.form}
              onSubmit={(event) => {
                const f = fields(event),
                  current = data.defaults.find((item) => item.level_id === level.id);
                void send(`people/organization/levels/${level.id}/default-leave`, {
                  annualDays: Number(f.get("annual")),
                  sickDays: Number(f.get("sick")),
                  expectedVersion: current?.version ?? 0,
                });
              }}
            >
              <h4 data-ui-appearance={appearance}>Leave default</h4>
              <p data-ui-appearance={appearance} className={styles.note}>
                Save a new policy version. Existing assignments retain their history; a level change
                applies to leave on the next 1 January.
              </p>
              <div data-ui-appearance={appearance} className={styles.fields}>
                <Field appearance={appearance}>
                  Annual days
                  <TextInput appearance={appearance}
                    name="annual"
                    type="number"
                    min={0}
                    max={365}
                    step={0.5}
                    required
                    defaultValue={
                      data.defaults.find((item) => item.level_id === level.id)?.annual_days ?? 0
                    }
                  />
                </Field>
                <Field appearance={appearance}>
                  Sick days
                  <TextInput appearance={appearance}
                    name="sick"
                    type="number"
                    min={0}
                    max={365}
                    step={0.5}
                    required
                    defaultValue={
                      data.defaults.find((item) => item.level_id === level.id)?.sick_days ?? 0
                    }
                  />
                </Field>
              </div>
              <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.button} disabled={busy}>
                Save leave default
              </Button>
            </form>
          )}
        </InlineEditor>
      ))}
      <InlineEditor appearance={appearance} title="Add staff level">
        <form data-ui-appearance={appearance}
          className={styles.form}
          onSubmit={(event) => {
            const f = fields(event);
            void send("staff/levels", {
              code: value(f, "code"),
              name: value(f, "name"),
              rank: Number(f.get("rank")),
              active: true,
              expectedVersion: 0,
            });
          }}
        >
          <Field appearance={appearance}>
            Code
            <TextInput appearance={appearance} name="code" required maxLength={30} />
          </Field>
          <Field appearance={appearance}>
            Name
            <TextInput appearance={appearance} name="name" required maxLength={100} />
          </Field>
          <Field appearance={appearance}>
            Rank
            <TextInput appearance={appearance}
              name="rank"
              type="number"
              min={-10000}
              max={10000}
              defaultValue={1}
              required
            />
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Create level
          </Button>
        </form>
      </InlineEditor>
      {!data.levels.some((level) => level.level_number) && (
        <Button appearance={appearance}
          type="submit"
          variant="primary"
          className={styles.button}
          disabled={busy}
          onClick={() => void send("people/organization/levels", {})}
        >
          Install six standard staff levels
        </Button>
      )}
    </div>
  );
}

function Calendars({ appearance = "legacy", data, send, busy }: ({ data: LeaveData; send: Send; busy: boolean }) & { appearance?: UiAppearance }) {
  return (
    <div data-ui-appearance={appearance} className={styles.stack}>
      {data.calendars.map((calendar) => (
        <div data-ui-appearance={appearance} className={styles.row} key={calendar.id}>
          <strong data-ui-appearance={appearance}>{calendar.name}</strong>
          {data.holidays
            .filter((holiday) => holiday.calendar_id === calendar.id)
            .map((holiday) => (
              <small data-ui-appearance={appearance} key={holiday.id}>
                {holiday.holiday_date} · {holiday.name}
              </small>
            ))}
        </div>
      ))}
      <InlineEditor appearance={appearance} title="Create holiday calendar">
        <form data-ui-appearance={appearance}
          className={styles.form}
          onSubmit={(event) => {
            const f = fields(event);
            void send("leave/admin", {
              resource: "calendar",
              name: value(f, "name"),
              timezone: value(f, "timezone"),
            });
          }}
        >
          <Field appearance={appearance}>
            Calendar name
            <TextInput appearance={appearance} name="name" required maxLength={100} />
          </Field>
          <Field appearance={appearance}>
            Timezone
            <TextInput appearance={appearance} name="timezone" required defaultValue="Asia/Bangkok" />
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Create calendar
          </Button>
        </form>
      </InlineEditor>
      <InlineEditor appearance={appearance} title="Add holiday">
        <form data-ui-appearance={appearance}
          className={styles.form}
          onSubmit={(event) => {
            const f = fields(event);
            void send("leave/admin", {
              resource: "holiday",
              calendarId: value(f, "calendar"),
              date: value(f, "date"),
              name: value(f, "name"),
            });
          }}
        >
          <CalendarSelect appearance={appearance} data={data} />
          <Field appearance={appearance}>
            Date
            <TextInput appearance={appearance} name="date" type="date" required />
          </Field>
          <Field appearance={appearance}>
            Holiday name
            <TextInput appearance={appearance} name="name" required maxLength={100} />
          </Field>
          <Button appearance={appearance}
            clientManaged type="submit"
            variant="primary"
            className={styles.primary}
            disabled={busy || !data.calendars.length}
          >
            Add holiday
          </Button>
        </form>
      </InlineEditor>
      <p data-ui-appearance={appearance} className={styles.note}>Assign calendars to staff from their Working time details.</p>
    </div>
  );
}
function CalendarSelect({ appearance = "legacy",
  data,
  optional = false,
}: ({
  data: Pick<LeaveData, "calendars">;
  optional?: boolean;
}) & { appearance?: UiAppearance }) {
  return (
    <Field appearance={appearance}>
      Holiday calendar
      <Select appearance={appearance} name="calendar" required={!optional}>
        <option data-ui-appearance={appearance} value="">{optional ? "No holiday calendar" : "Choose a calendar"}</option>
        {data.calendars.map((calendar) => (
          <option data-ui-appearance={appearance} key={calendar.id} value={calendar.id}>
            {calendar.name}
          </option>
        ))}
      </Select>
    </Field>
  );
}
function EngagementSelect({ appearance = "legacy", data }: ({ data: LeaveData }) & { appearance?: UiAppearance }) {
  return (
    <Field appearance={appearance}>
      Staff engagement
      <Select appearance={appearance} name="engagement" required>
        <option data-ui-appearance={appearance} value="">Choose staff</option>
        {data.engagements.map((engagement) => (
          <option data-ui-appearance={appearance} key={engagement.id} value={engagement.id}>
            {engagement.name} · {engagement.worker_type}
          </option>
        ))}
      </Select>
    </Field>
  );
}

function LeaveSettings({ appearance = "legacy",
  base,
  data,
  send,
  busy,
  reload,
  initialAction,
}: ({
  base: string;
  data: LeaveData;
  send: Send;
  busy: boolean;
  reload: () => Promise<void>;
  initialAction?: string;
}) & { appearance?: UiAppearance }) {
  const [creatingPolicy, setCreatingPolicy] = useState(initialAction === "policy"),
    [eligibility, setEligibility] = useState("");
  function policyMode(open: boolean) {
    setCreatingPolicy(open);
    const url = new URL(window.location.href);
    if (!url.pathname.endsWith("/people/settings") && url.searchParams.get("panel") !== "settings") return;
    if (open) {
      url.searchParams.set("editor", "policy");
      window.history.pushState(null, "", url);
    } else {
      if (url.searchParams.get("new") === "leave-policy") url.searchParams.delete("new");
      if (url.searchParams.get("editor") === "policy") url.searchParams.delete("editor");
      window.history.replaceState(null, "", url);
    }
  }
  if (creatingPolicy)
    return (
      <FormDrawer appearance="standard" open title="Create leave policy" onClose={()=>policyMode(false)}>
      <LeavePolicyWizard appearance={appearance}
        base={base}
        types={data.types}
        onSaved={async () => {
          await reload();
          policyMode(false);
        }}
        onCancel={() => policyMode(false)}
      /></FormDrawer>
    );
  return (
    <div data-ui-appearance={appearance} className={styles.stack}>
      <div data-ui-appearance={appearance} className={styles.sectionHead}>
        <p data-ui-appearance={appearance}>
          {data.types.length} leave types · {data.policies.length} policy versions
        </p>
        <Button appearance={appearance}
          type="submit"
          variant="primary"
          className={styles.button}
          onClick={() => policyMode(true)}
        >
          Create policy
        </Button>
      </div>
      {data.policies.map((policy) => (
        <div data-ui-appearance={appearance} className={styles.row} key={policy.id}>
          <strong data-ui-appearance={appearance}>
            {policy.name} · v{policy.version}
          </strong>
          <small data-ui-appearance={appearance}>
            {policy.effective_from} — {policy.effective_to ?? "ongoing"}
          </small>
        </div>
      ))}
      <InlineEditor appearance={appearance} title="Create leave type">
        <form data-ui-appearance={appearance}
          className={styles.form}
          onSubmit={(event) => {
            const f = fields(event);
            void send("leave/admin", {
              resource: "type",
              code: value(f, "code"),
              name: value(f, "name"),
              description: value(f, "description"),
              tracked: f.has("tracked"),
              evidenceAfterUnits: value(f, "evidence") ? Number(f.get("evidence")) : null,
              evidenceNote: value(f, "evidenceNote"),
            });
          }}
        >
          <div data-ui-appearance={appearance} className={styles.fields}>
            <Field appearance={appearance}>
              Code
              <TextInput appearance={appearance} name="code" required maxLength={40} />
            </Field>
            <Field appearance={appearance}>
              Name
              <TextInput appearance={appearance} name="name" required maxLength={100} />
            </Field>
          </div>
          <Field appearance={appearance}>
            Description
            <TextArea appearance={appearance} name="description" maxLength={500} />
          </Field>
          <Field appearance={appearance} orientation="inline" className={styles.check}>
            <TextInput appearance={appearance} type="checkbox" name="tracked" defaultChecked />
            Track entitlement
          </Field>
          <Field appearance={appearance}>
            Evidence required after (days)
            <TextInput appearance={appearance} name="evidence" type="number" min={0.5} step={0.5} />
          </Field>
          <Field appearance={appearance}>
            Evidence guidance
            <TextArea appearance={appearance} name="evidenceNote" maxLength={500} />
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Create leave type
          </Button>
        </form>
      </InlineEditor>
      <InlineEditor appearance={appearance} title="Assign work policy">
        <form data-ui-appearance={appearance}
          className={styles.form}
          onSubmit={(event) => {
            const f = fields(event);
            void send("leave/admin", {
              resource: "assignment",
              engagementId: value(f, "engagement"),
              policyVersionId: value(f, "policy"),
              calendarId: value(f, "calendar") || null,
              effectiveFrom: value(f, "from"),
              effectiveTo: value(f, "to") || null,
            });
          }}
        >
          <EngagementSelect appearance={appearance} data={data} />
          <Field appearance={appearance}>
            Policy
            <Select appearance={appearance} name="policy" required>
              <option data-ui-appearance={appearance} value="">Choose a policy version</option>
              {data.policies.map((policy) => (
                <option data-ui-appearance={appearance} key={policy.id} value={policy.id}>
                  {policy.name} · v{policy.version}
                </option>
              ))}
            </Select>
          </Field>
          <CalendarSelect appearance={appearance} data={data} optional />
          <div data-ui-appearance={appearance} className={styles.fields}>
            <Field appearance={appearance}>
              Effective from
              <TextInput appearance={appearance} name="from" type="date" required />
            </Field>
            <Field appearance={appearance}>
              Effective to
              <TextInput appearance={appearance} name="to" type="date" />
            </Field>
          </div>
          <Button appearance={appearance}
            clientManaged type="submit"
            variant="primary"
            className={styles.primary}
            disabled={busy || !data.policies.length}
          >
            Assign policy
          </Button>
        </form>
      </InlineEditor>
      <InlineEditor appearance={appearance} title="Overtime eligibility">
        <form data-ui-appearance={appearance}
          className={styles.form}
          onSubmit={(event) => {
            const f = fields(event);
            void send("overtime-requests/eligibility", {
              engagementId: eligibility,
              expectedVersion:
                data.eligibilities.find((item) => item.engagement_id === eligibility)?.version ?? 0,
              eligible: value(f, "eligible") === "true",
              topLevel: f.has("topLevel"),
              reason: value(f, "reason"),
            });
          }}
        >
          <Field appearance={appearance}>
            Staff engagement
            <Select appearance={appearance}
              required
              value={eligibility}
              onChange={(event) => setEligibility(event.target.value)}
            >
              <option data-ui-appearance={appearance} value="">Choose staff</option>
              {data.engagements.map((item) => (
                <option data-ui-appearance={appearance} key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </Select>
          </Field>
          <div data-ui-appearance={appearance} className={styles.form} key={eligibility}>
            <Field appearance={appearance}>
              Eligibility
              <Select appearance={appearance}
                name="eligible"
                defaultValue={String(
                  data.eligibilities.find((item) => item.engagement_id === eligibility)?.eligible ??
                    true,
                )}
              >
                <option data-ui-appearance={appearance} value="true">Eligible</option>
                <option data-ui-appearance={appearance} value="false">Ineligible</option>
              </Select>
            </Field>
            <Field appearance={appearance} orientation="inline" className={styles.check}>
              <TextInput appearance={appearance}
                type="checkbox"
                name="topLevel"
                defaultChecked={
                  data.eligibilities.find((item) => item.engagement_id === eligibility)
                    ?.top_level ?? false
                }
              />
              Confirmed top-level engagement
            </Field>
          </div>
          <Field appearance={appearance}>
            Reason
            <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
          </Field>
          <Button appearance={appearance}
            clientManaged type="submit"
            variant="primary"
            className={styles.primary}
            disabled={busy || !eligibility}
          >
            Save eligibility
          </Button>
        </form>
      </InlineEditor>
      <InlineEditor appearance={appearance} title="Record entitlement adjustment">
        <form data-ui-appearance={appearance}
          className={styles.form}
          onSubmit={(event) => {
            const f = fields(event);
            void send("leave/admin", {
              resource: "adjustment",
              engagementId: value(f, "engagement"),
              leaveTypeId: value(f, "type"),
              units: Number(f.get("units")),
              effectiveDate: value(f, "date"),
              note: value(f, "reason"),
            });
          }}
        >
          <EngagementSelect appearance={appearance} data={data} />
          <Field appearance={appearance}>
            Leave type
            <Select appearance={appearance} name="type" required>
              <option data-ui-appearance={appearance} value="">Choose a leave type</option>
              {data.types.map((item) => (
                <option data-ui-appearance={appearance} key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </Select>
          </Field>
          <div data-ui-appearance={appearance} className={styles.fields}>
            <Field appearance={appearance}>
              Units
              <TextInput appearance={appearance} name="units" type="number" min={-365} max={365} step={0.5} required />
            </Field>
            <Field appearance={appearance}>
              Effective date
              <TextInput appearance={appearance} name="date" type="date" required />
            </Field>
          </div>
          <Field appearance={appearance}>
            Reason
            <TextArea appearance={appearance} name="reason" required maxLength={500} />
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Record adjustment
          </Button>
        </form>
      </InlineEditor>
      <h4 data-ui-appearance={appearance}>Current assignments</h4>
      {data.assignments
        .filter((item) => !item.effective_to)
        .map((item) => (
          <InlineEditor appearance={appearance} key={`${item.id}:${item.version}`} title={`${item.name} · ${item.policy}`}>
            <form data-ui-appearance={appearance}
              className={styles.form}
              onSubmit={(event) => {
                const f = fields(event);
                void send("leave/admin", {
                  resource: "assignment_end",
                  id: item.id,
                  expectedVersion: item.version,
                  effectiveTo: value(f, "to"),
                });
              }}
            >
              <p data-ui-appearance={appearance} className={styles.note}>Effective from {item.effective_from}</p>
              <Field appearance={appearance}>
                End assignment on
                <TextInput appearance={appearance} name="to" type="date" required min={item.effective_from} />
              </Field>
              <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.button} disabled={busy}>
                End assignment
              </Button>
            </form>
          </InlineEditor>
        ))}
    </div>
  );
}

function PayGroups({ appearance = "legacy", data, send, busy }: ({ data: GroupsData; send: Send; busy: boolean }) & { appearance?: UiAppearance }) {
  return (
    <div data-ui-appearance={appearance} className={styles.stack}>
      {data.groups.map((group) => (
        <div data-ui-appearance={appearance} className={styles.row} key={String(group.id)}>
          <strong data-ui-appearance={appearance}>{String(group.name)}</strong>
          <small data-ui-appearance={appearance}>
            {String(group.currency)} · {String(group.frequency)}
          </small>
        </div>
      ))}
      {data.canManage && (
        <InlineEditor appearance={appearance} title="Configure pay group">
          <form data-ui-appearance={appearance}
            className={styles.form}
            onSubmit={(event) => {
              const f = fields(event);
              void send("people/pay-groups", {
                name: value(f, "name"),
                countryCode: value(f, "countryCode"),
                currency: value(f, "currency"),
                frequency: value(f, "frequency"),
                timezone: value(f, "timezone"),
                overtimeMultiplierBps: Math.round(Number(f.get("multiplier")) * 10000),
                prorationMethod: value(f, "proration"),
              });
            }}
          >
            <Field appearance={appearance}>
              Name
              <TextInput appearance={appearance} name="name" required maxLength={120} />
            </Field>
            <div data-ui-appearance={appearance} className={styles.fields}>
              <Field appearance={appearance}>
                Country code
                <TextInput appearance={appearance}
                  name="countryCode"
                  required
                  minLength={2}
                  maxLength={2}
                  defaultValue="TH"
                />
              </Field>
              <Field appearance={appearance}>
                Currency
                <TextInput appearance={appearance}
                  name="currency"
                  required
                  minLength={3}
                  maxLength={3}
                  defaultValue="THB"
                />
              </Field>
            </div>
            <Field appearance={appearance}>
              Frequency
              <Select appearance={appearance} name="frequency" defaultValue="monthly">
                <option data-ui-appearance={appearance} value="weekly">Weekly</option>
                <option data-ui-appearance={appearance} value="fortnightly">Fortnightly</option>
                <option data-ui-appearance={appearance} value="semimonthly">Semimonthly</option>
                <option data-ui-appearance={appearance} value="monthly">Monthly</option>
              </Select>
            </Field>
            <Field appearance={appearance}>
              Timezone
              <TextInput appearance={appearance} name="timezone" required defaultValue="Asia/Bangkok" />
            </Field>
            <Field appearance={appearance}>
              Overtime multiplier
              <TextInput appearance={appearance}
                name="multiplier"
                type="number"
                min={1}
                max={5}
                step={0.01}
                required
                defaultValue={1.5}
              />
            </Field>
            <Field appearance={appearance}>
              Proration
              <Select appearance={appearance} name="proration">
                <option data-ui-appearance={appearance} value="calendar_days">Calendar days</option>
                <option data-ui-appearance={appearance} value="none">Manual full-period inputs</option>
              </Select>
            </Field>
            <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
              Save pay group policy
            </Button>
          </form>
        </InlineEditor>
      )}
      <p data-ui-appearance={appearance} className={styles.note}>
        Assign a pay group from a person’s Compensation details. Prepare, calculate and approve pay
        periods in Payroll.
      </p>
    </div>
  );
}

function Lifecycle({ appearance = "legacy", data, send, busy }: ({ data: LifecycleData; send: Send; busy: boolean }) & { appearance?: UiAppearance }) {
  return (
    <div className={styles.stack}>
      <dl className={styles.summary}><div><dt>IT owner</dt><dd>{data.owners.find(owner => owner.id === data.settings.itAssigneeId)?.name ?? "Unassigned IT queue"}</dd></div><div><dt>Preparation tasks</dt><dd>{data.settings.onboardingTasks.length} onboarding · {data.settings.offboardingTasks.length} offboarding</dd></div></dl>
      <InlineEditor title="Configure lifecycle handoff"><form data-ui-appearance={appearance}
      className={styles.form}
      key={data.settings.version}
      onSubmit={(event) => {
        const f = fields(event);
        void send("people/lifecycle/settings", {
          expectedVersion: data.settings.version,
          itAssigneeId: value(f, "owner") || null,
          onboardingTasks: lines(f, "onboarding"),
          offboardingTasks: lines(f, "offboarding"),
        });
      }}
    >
      <p data-ui-appearance={appearance} className={styles.note}>
        Route preparation tasks to the IT owner. Position assignment and access preparation retain
        their approval workflows.
      </p>
      <Field appearance={appearance}>
        IT owner
        <Select appearance={appearance} name="owner" defaultValue={data.settings.itAssigneeId ?? ""}>
          <option data-ui-appearance={appearance} value="">Unassigned IT queue</option>
          {data.settings.itAssigneeId &&
            !data.owners.some((owner) => owner.id === data.settings.itAssigneeId) && (
              <option data-ui-appearance={appearance} value={data.settings.itAssigneeId}>Current assigned owner</option>
            )}
          {data.owners.map((owner) => (
            <option data-ui-appearance={appearance} key={owner.id} value={owner.id}>
              {owner.name}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance={appearance}>
        Onboarding tasks, one per line
        <TextArea appearance={appearance} name="onboarding" defaultValue={data.settings.onboardingTasks.join("\n")} />
      </Field>
      <Field appearance={appearance}>
        Offboarding tasks, one per line
        <TextArea appearance={appearance} name="offboarding" defaultValue={data.settings.offboardingTasks.join("\n")} />
      </Field>
      <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
        Save lifecycle handoff
      </Button>
    </form></InlineEditor></div>
  );
}

function Templates({ appearance = "legacy", data, send, busy }: ({ data: TemplatesData; send: Send; busy: boolean }) & { appearance?: UiAppearance }) {
  const [draft, setDraft] = useState<OrganizationTemplatePosition[]>(
    staffLevelTitles.map((title, index) => ({
      key: `level-${index + 1}`,
      title,
      levelNumber: index + 1,
      reportsToKey: index ? `level-${index}` : null,
      isCeo: index === 0,
      rosterRequired: false,
    })),
  );
  const [selected, setSelected] = useState(""),
    [branch, setBranch] = useState("");
  const template = data.templates.find((item) => item.id === selected);
  const [holding, setHolding] = useState<HoldingPeopleTemplate | null | undefined>(undefined),
    [preview, setPreview] = useState<{
      template: HoldingPeopleTemplate;
      token: string;
      message: string;
    } | null>(null);
  function update(index: number, change: Partial<OrganizationTemplatePosition>) {
    setDraft((current) => current.map((item, i) => (i === index ? { ...item, ...change } : item)));
  }
  return (
    <div data-ui-appearance={appearance} className={styles.stack}>
      <InlineEditor appearance={appearance} title="Create organization template">
        <form data-ui-appearance={appearance}
          className={styles.form}
          onSubmit={(event) => {
            const f = fields(event);
            void send("people/organization/templates", {
              name: value(f, "name"),
              positions: draft,
              expectedVersion: 0,
            });
          }}
        >
          <Field appearance={appearance}>
            Template name
            <TextInput appearance={appearance}
              name="name"
              required
              minLength={2}
              maxLength={120}
              defaultValue="Standard organization"
            />
          </Field>
          {draft.map((position, index) => (
            <fieldset data-ui-appearance={appearance} key={position.key}>
              <legend data-ui-appearance={appearance}>Position {index + 1}</legend>
              <Field appearance={appearance}>
                Title
                <TextInput appearance={appearance}
                  required
                  minLength={2}
                  maxLength={160}
                  value={position.title}
                  onChange={(event) => update(index, { title: event.target.value })}
                />
              </Field>
              <Field appearance={appearance}>
                Default level
                <Select appearance={appearance}
                  value={position.levelNumber}
                  disabled={position.isCeo}
                  onChange={(event) => update(index, { levelNumber: Number(event.target.value) })}
                >
                  {staffLevelTitles.map((title, i) => (
                    <option data-ui-appearance={appearance} key={i} value={i + 1}>
                      Level {i + 1} · {title}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance={appearance}>
                Reports to
                <Select appearance={appearance}
                  value={position.reportsToKey ?? ""}
                  disabled={position.isCeo}
                  onChange={(event) => update(index, { reportsToKey: event.target.value || null })}
                >
                  <option data-ui-appearance={appearance} value="">Company leadership</option>
                  {draft
                    .filter((item) => item.levelNumber < position.levelNumber)
                    .map((item) => (
                      <option data-ui-appearance={appearance} key={item.key} value={item.key}>
                        {item.title}
                      </option>
                    ))}
                </Select>
              </Field>
              <Field appearance={appearance} orientation="inline" className={styles.check}>
                <TextInput appearance={appearance}
                  type="checkbox"
                  checked={!!position.rosterRequired}
                  onChange={(event) => update(index, { rosterRequired: event.target.checked })}
                />
                Weekly roster required
              </Field>
              {!position.isCeo && (
                <Button appearance={appearance}
                  variant="primary"
                  className={styles.button}
                  type="button"
                  onClick={() => setDraft((current) => current.filter((_, i) => i !== index))}
                >
                  Remove position
                </Button>
              )}
            </fieldset>
          ))}
          <Button appearance={appearance}
            variant="primary"
            className={styles.button}
            type="button"
            onClick={() =>
              setDraft((current) => [
                ...current,
                {
                  key: crypto.randomUUID(),
                  title: "New position",
                  levelNumber: 6,
                  reportsToKey: "level-1",
                  isCeo: false,
                  rosterRequired: false,
                },
              ])
            }
          >
            Add position
          </Button>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Save template
          </Button>
        </form>
      </InlineEditor>
      <InlineEditor appearance={appearance} title="Apply reviewed organization template">
        <div data-ui-appearance={appearance} className={styles.form}>
          <Field appearance={appearance}>
            Template
            <Select appearance={appearance} value={selected} onChange={(event) => setSelected(event.target.value)}>
              <option data-ui-appearance={appearance} value="">Choose a template</option>
              {data.templates.map((item) => (
                <option data-ui-appearance={appearance} key={item.id} value={item.id}>
                  {item.name} · v{item.version}
                </option>
              ))}
            </Select>
          </Field>
          {template && (
            <form data-ui-appearance={appearance}
              className={styles.form}
              onSubmit={(event) => {
                const f = fields(event);
                void send(`people/organization/templates/${template.id}/apply`, {
                  expectedVersion: template.version,
                  branchId: branch || null,
                  departmentId: value(f, "department") || null,
                  reason: value(f, "reason"),
                });
              }}
            >
              <Field appearance={appearance}>
                Target branch
                <Select appearance={appearance} value={branch} onChange={(event) => setBranch(event.target.value)}>
                  <option data-ui-appearance={appearance} value="">Company-wide</option>
                  {data.nodes
                    .filter((node) => node.kind === "branch")
                    .map((node) => (
                      <option data-ui-appearance={appearance} key={node.id} value={node.id}>
                        {node.display_name}
                      </option>
                    ))}
                </Select>
              </Field>
              <Field appearance={appearance}>
                Target department
                <Select appearance={appearance} name="department" key={branch}>
                  <option data-ui-appearance={appearance} value="">All departments</option>
                  {data.nodes
                    .filter(
                      (node) => node.kind === "department" && (node.branch_id ?? "") === branch,
                    )
                    .map((node) => (
                      <option data-ui-appearance={appearance} key={node.id} value={node.id}>
                        {node.display_name}
                      </option>
                    ))}
                </Select>
              </Field>
              {template.positions.map((position) => (
                <p data-ui-appearance={appearance} className={styles.note} key={position.key}>
                  {position.title} · Level {position.levelNumber} →{" "}
                  {template.positions.find((item) => item.key === position.reportsToKey)?.title ??
                    "Company leadership"}
                </p>
              ))}
              <p data-ui-appearance={appearance} className={styles.note}>
                The existing CEO position is reused. Other positions are added as vacancies.
              </p>
              <Field appearance={appearance}>
                Reason
                <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
              </Field>
              <Field appearance={appearance} orientation="inline" className={styles.check}>
                <TextInput appearance={appearance} type="checkbox" required />I reviewed the positions and placement
              </Field>
              <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
                Apply template
              </Button>
            </form>
          )}
        </div>
      </InlineEditor>
      <h4 data-ui-appearance={appearance}>Holding HR templates</h4>
      {data.holding.templates.map((item) => (
        <div data-ui-appearance={appearance} className={styles.row} key={item.id}>
          <strong data-ui-appearance={appearance}>
            {item.name} · v{item.version}
          </strong>
          <small data-ui-appearance={appearance}>
            {item.adoptedVersion ? `Company uses v${item.adoptedVersion}` : "Not adopted"} ·{" "}
            {item.definition.attributes.length} shared fields
          </small>
          <div data-ui-appearance={appearance} className={styles.actions}>
            {data.holding.canPublish && (
              <Button appearance={appearance}
                type="submit"
                variant="primary"
                className={styles.button}
                onClick={() => {
                  setHolding(item);
                  setPreview(null);
                }}
              >
                Edit template
              </Button>
            )}
            {data.holding.canAdopt && (
              <Button appearance={appearance}
                type="submit"
                variant="primary"
                className={styles.button}
                disabled={busy}
                onClick={async () => {
                  const result = await send(
                    "people/templates/preview",
                    {
                      templateId: item.id,
                      revision: item.version,
                      reason: "Review Holding template adoption",
                    },
                    "Template preview ready.",
                  );
                  if (result) {
                    setHolding(undefined);
                    setPreview({
                      template: item,
                      token: result.previewToken,
                      message: result.message,
                    });
                  }
                }}
              >
                Preview adoption
              </Button>
            )}
          </div>
        </div>
      ))}
      {data.holding.canPublish && (
        <Button appearance={appearance}
          type="submit"
          variant="primary"
          className={styles.button}
          onClick={() => {
            setHolding(null);
            setPreview(null);
          }}
        >
          Create Holding HR template
        </Button>
      )}
      {holding !== undefined && (
        <form data-ui-appearance={appearance}
          className={styles.form}
          key={holding?.id ?? "new"}
          onSubmit={async (event) => {
            const f = fields(event),
              result = await send("people/templates/publish", {
                id: holding?.id,
                expectedVersion: holding?.version ?? 0,
                name: value(f, "name"),
                reason: value(f, "reason"),
                definition: {
                  onboardingTasks: lines(f, "onboarding"),
                  offboardingTasks: lines(f, "offboarding"),
                  attributes: holding?.definition.attributes ?? [],
                },
              });
            if (result) setHolding(undefined);
          }}
        >
          <Field appearance={appearance}>
            Name
            <TextInput appearance={appearance}
              name="name"
              required
              minLength={2}
              maxLength={120}
              defaultValue={holding?.name}
            />
          </Field>
          <Field appearance={appearance}>
            Onboarding tasks, one per line
            <TextArea appearance={appearance}
              name="onboarding"
              defaultValue={holding?.definition.onboardingTasks.join("\n")}
            />
          </Field>
          <Field appearance={appearance}>
            Offboarding tasks, one per line
            <TextArea appearance={appearance}
              name="offboarding"
              defaultValue={holding?.definition.offboardingTasks.join("\n")}
            />
          </Field>
          <Field appearance={appearance}>
            Reason
            <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
          </Field>
          <div data-ui-appearance={appearance} className={styles.actions}>
            <Button appearance={appearance}
              variant="primary"
              type="button"
              className={styles.button}
              disabled={busy}
              onClick={() => setHolding(undefined)}
            >
              Cancel
            </Button>
            <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
              Publish revision
            </Button>
          </div>
        </form>
      )}
      {preview && (
        <section data-ui-appearance={appearance} className={styles.stack}>
          <strong data-ui-appearance={appearance}>Review Company adoption</strong>
          <p data-ui-appearance={appearance} className={styles.note}>{preview.message}</p>
          <p data-ui-appearance={appearance} className={styles.note}>
            Onboarding: {preview.template.definition.onboardingTasks.join(", ") || "None"}
          </p>
          <p data-ui-appearance={appearance} className={styles.note}>
            Offboarding: {preview.template.definition.offboardingTasks.join(", ") || "None"}
          </p>
          <p data-ui-appearance={appearance} className={styles.note}>
            Shared fields:{" "}
            {preview.template.definition.attributes.map((item) => item.label).join(", ") || "None"}
          </p>
          <div data-ui-appearance={appearance} className={styles.actions}>
            <Button appearance={appearance}
              type="submit"
              variant="primary"
              className={styles.button}
              disabled={busy}
              onClick={() => setPreview(null)}
            >
              Cancel
            </Button>
            <Button appearance={appearance}
              type="submit"
              variant="primary"
              className={styles.primary}
              disabled={busy}
              onClick={async () => {
                const result = await send("people/templates/adopt", {
                  templateId: preview.template.id,
                  revision: preview.template.version,
                  reason: "Adopt reviewed Holding template",
                  previewToken: preview.token,
                });
                if (result) setPreview(null);
              }}
            >
              Adopt for Company
            </Button>
          </div>
        </section>
      )}
    </div>
  );
}
