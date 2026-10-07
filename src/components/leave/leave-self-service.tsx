"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { useCallback, useEffect, useRef, useState } from "react";
import { CalendarDays, ChevronRight, Plus, ShieldCheck } from "lucide-react";
import Link from "next/link";
import { itemOnDate, type CalendarData, type CalendarItem } from "@/lib/calendar";
import { today } from "@/lib/calendar-date";
import { workFetch } from "@/lib/work-fetch";
import { MonthGrid } from "@/components/calendar/month-grid";
import { MonthNavigation } from "@/components/calendar/month-navigation";
import type { leaveCalendarMonth, leaveSummary } from "@/server/modules/people_payroll/public/leave-self-service";
import styles from "./leave-self-service.module.css";

type Summary = Awaited<ReturnType<typeof leaveSummary>>;
type MonthData = Awaited<ReturnType<typeof leaveCalendarMonth>>;
type LeaveRow = MonthData["requests"][number];
type ListFilter = "all" | "drafts" | "review" | "approved" | "closed";
type RequestPage = { data: LeaveRow[]; hasMore: boolean; nextCursor: string | null };
const filters: { id: ListFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "drafts", label: "Drafts" },
  { id: "review", label: "In review" },
  { id: "approved", label: "Approved" },
  { id: "closed", label: "Closed" },
];
const pageSize = 10;

function formatDay(
  date: string,
  options: Intl.DateTimeFormatOptions = { day: "numeric", month: "short", year: "numeric" },
) {
  return new Intl.DateTimeFormat(undefined, { ...options, timeZone: "UTC" }).format(
    new Date(date + "T12:00:00Z"),
  );
}
function units(value: unknown) {
  const amount = Number(value ?? 0);
  return (
    amount.toLocaleString(undefined, { maximumFractionDigits: 1 }) +
    " day" +
    (amount === 1 ? "" : "s")
  );
}
function requestStatus(request: Pick<LeaveRow, "state" | "cancellation_state">) {
  if (request.cancellation_state === "pending_review") return "Cancellation in review";
  if (request.state === "pending_approval") return "In review";
  if (request.state === "draft") return "Draft";
  if (request.state === "approved") return "Approved";
  return request.state.replaceAll("_", " ");
}
function overlapsDate(request: LeaveRow, date: string) {
  const end = request.end_date ?? request.start_date;
  return request.start_date <= date && end >= date;
}

export function LeaveSelfService({
  base,
  initial,
  initialMonth,
  initialFilter,
}: {
  base: string;
  initial: Summary;
  initialMonth: string;
  initialFilter: ListFilter;
}) {
  const [month, setMonth] = useState(initialMonth);
  const [filter, setFilter] = useState(initialFilter);
  const currentDate = today(initial.policy?.timezone ?? "Asia/Bangkok");
  const currentMonth = currentDate.slice(0, 7);
  const [selectedDate, setSelectedDate] = useState<string | null>(null);
  const [calendar, setCalendar] = useState<MonthData | null>(null);
  const [teamItems, setTeamItems] = useState<CalendarItem[]>([]);
  const [calendarError, setCalendarError] = useState("");
  const [teamError, setTeamError] = useState("");
  const [calendarBusy, setCalendarBusy] = useState(true);
  const [requests, setRequests] = useState<LeaveRow[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [listBusy, setListBusy] = useState(true);
  const [listError, setListError] = useState("");
  const [moreBusy, setMoreBusy] = useState(false);
  const [retryMonth, setRetryMonth] = useState(0);
  const [retryList, setRetryList] = useState(0);
  const monthRef = useRef(initialMonth);
  const filterRef = useRef(initialFilter);
  const monthSequence = useRef(0);
  const listSequence = useRef(0);

  const writeUrl = useCallback(
    (patch: { month?: string; filter?: ListFilter }, replace = false) => {
      const url = new URL(window.location.href);
      if (patch.month) url.searchParams.set("month", patch.month);
      if (patch.filter) url.searchParams.set("filter", patch.filter);
      window.history[replace ? "replaceState" : "pushState"](null, "", url);
    },
    [],
  );

  const loadMonth = useCallback(
    async (selectedMonth: string) => {
      const sequence = ++monthSequence.current;
      const ownRequest = workFetch(
        "/api/v1" + base + "/leave/calendar?month=" + encodeURIComponent(selectedMonth),
        { cache: "no-store" },
      ).then(async (response) => {
        const body = await response.json();
        if (!response.ok)
          throw new Error(body.error?.message ?? "Your leave calendar could not be loaded.");
        return body.data as MonthData;
      });

      const teamRequest = (async () => {
        const accumulated: CalendarItem[] = [];
        let cursor = 0;
        let failedSource = false;
        do {
          const query = new URLSearchParams({
            date: selectedMonth + "-01",
            view: "month",
            mode: "team",
            layers: "leave",
            cursor: String(cursor),
            limit: "200",
          });
          const response = await workFetch("/api/v1" + base + "/calendar?" + query, {
            cache: "no-store",
          });
          const body = await response.json();
          if (!response.ok)
            throw new Error(body.error?.message ?? "Team availability could not be loaded.");
          const page = body.data as CalendarData;
          accumulated.push(...page.items);
          failedSource ||= page.sources.some(
            (source) => source.source === "leave" && source.status === "error",
          );
          cursor = page.nextCursor ?? -1;
        } while (cursor >= 0);
        return { items: accumulated, failedSource };
      })();

      const [ownResult, teamResult] = await Promise.allSettled([ownRequest, teamRequest]);
      if (sequence !== monthSequence.current) return;
      if (ownResult.status === "fulfilled") setCalendar(ownResult.value);
      else
        setCalendarError(
          ownResult.reason instanceof Error
            ? ownResult.reason.message
            : "Your leave calendar could not be loaded.",
        );
      if (teamResult.status === "fulfilled") {
        setTeamItems(
          teamResult.value.items.filter(
            (item) => item.source === "leave" && item.status === "approved" && item.redacted,
          ),
        );
        if (teamResult.value.failedSource)
          setTeamError(
            "Some approved team absences could not be loaded. Your requests and holidays are still available.",
          );
      } else {
        setTeamError(
          "Team availability could not be loaded. Your requests and holidays are still available.",
        );
      }
      setCalendarBusy(false);
    },
    [base],
  );

  const loadRequests = useCallback(
    async (selectedFilter: ListFilter, append = false, cursor?: string) => {
      const sequence = ++listSequence.current;
      try {
        const query = new URLSearchParams({ filter: selectedFilter, limit: String(pageSize) });
        if (cursor) query.set("cursor", cursor);
        const response = await workFetch("/api/v1" + base + "/leave-requests?" + query, {
          cache: "no-store",
        });
        const body = (await response.json()) as RequestPage & { error?: { message?: string } };
        if (!response.ok)
          throw new Error(body.error?.message ?? "Your requests could not be loaded.");
        if (sequence !== listSequence.current) return;
        setRequests((current) => (append ? [...current, ...body.data] : body.data));
        setNextCursor(body.nextCursor);
      } catch (error) {
        if (sequence === listSequence.current)
          setListError(
            error instanceof Error ? error.message : "Your requests could not be loaded.",
          );
      } finally {
        if (sequence === listSequence.current) {
          setListBusy(false);
          setMoreBusy(false);
        }
      }
    },
    [base, setListError],
  );

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadMonth(month);
    }, 0);
    return () => window.clearTimeout(timer);
  }, [loadMonth, month, retryMonth]);
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadRequests(filter);
    }, 0);
    return () => window.clearTimeout(timer);
  }, [filter, loadRequests, retryList]);
  useEffect(() => {
    writeUrl({ month, filter }, true);
  }, [writeUrl, month, filter]);
  useEffect(() => {
    const onPopState = () => {
      const url = new URL(window.location.href);
      const nextMonth = url.searchParams.get("month");
      const nextFilter = url.searchParams.get("filter") as ListFilter | null;
      if (
        nextMonth &&
        /^\d{4}-(0[1-9]|1[0-2])$/.test(nextMonth) &&
        nextMonth !== monthRef.current
      ) {
        monthRef.current = nextMonth;
        monthSequence.current += 1;
        setCalendarBusy(true);
        setCalendarError("");
        setTeamError("");
        setCalendar(null);
        setTeamItems([]);
        setMonth(nextMonth);
      }
      if (filters.some((item) => item.id === nextFilter) && nextFilter !== filterRef.current) {
        filterRef.current = nextFilter!;
        listSequence.current += 1;
        setListBusy(true);
        setListError("");
        setRequests([]);
        setNextCursor(null);
        setFilter(nextFilter!);
      }
      setSelectedDate(null);
    };
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  function changeMonth(nextMonth: string) {
    if (nextMonth === monthRef.current) return;
    monthRef.current = nextMonth;
    monthSequence.current += 1;
    setCalendarBusy(true);
    setCalendarError("");
    setTeamError("");
    setCalendar(null);
    setTeamItems([]);
    setMonth(nextMonth);
    setSelectedDate(null);
    writeUrl({ month: nextMonth });
  }
  function changeFilter(nextFilter: ListFilter) {
    if (nextFilter === filterRef.current) return;
    filterRef.current = nextFilter;
    listSequence.current += 1;
    setListBusy(true);
    setListError("");
    setRequests([]);
    setNextCursor(null);
    setFilter(nextFilter);
    writeUrl({ filter: nextFilter });
  }

  const ownRequests = calendar?.requests ?? [];
  const ownIds = new Set(ownRequests.map((request) => request.id));
  const visibleTeamItems = teamItems.filter((item) => !ownIds.has(item.sourceId));
  const monthItemsForDate = (date: string) =>
    visibleTeamItems.filter((item) => itemOnDate(item, date, item.timezone));
  const dateRequests = (date: string) =>
    ownRequests.filter((request) => overlapsDate(request, date));
  const dateHolidays = (date: string) =>
    (calendar?.holidays ?? []).filter((holiday) => holiday.holiday_date === date);
  const selectedRequests = selectedDate ? dateRequests(selectedDate) : [];
  const selectedTeam = selectedDate ? monthItemsForDate(selectedDate) : [];
  const selectedHolidays = selectedDate ? dateHolidays(selectedDate) : [];
  const selectedIsEmpty =
    !!selectedDate &&
    !calendarBusy &&
    !!calendar &&
    !selectedRequests.length &&
    !selectedTeam.length &&
    !selectedHolidays.length;

  return (
    <div className={styles.page}>
      <header className={styles.hero}>
        <div>
          <p className={styles.eyebrow}>People · Self-service</p>
          <h1>My leave</h1>
          <p>Plan time away, check your balance, and follow each request.</p>
        </div>
        <Link data-slot="button" className={['ui-standard-control', ("button " + styles.requestAction)].filter(Boolean).join(' ')} href={base + "/leave/new"}>
          <Plus size={17} /> Request leave
        </Link>
      </header>

      {initial.setupRequired && (
        <section className={styles.setup} role="status">
          <ShieldCheck size={19} />
          <div>
            <strong>HR setup required</strong>
            <p>{initial.setupMessage} Earlier requests remain available below.</p>
          </div>
        </section>
      )}

      <section className={styles.balanceSection} aria-labelledby="leave-balances-title">
        <div className={styles.sectionHeading}>
          <div>
            <h2 id="leave-balances-title">Your balance</h2>
            <p>
              {initial.policy
                ? initial.policy.name + " · " + initial.policy.timezone
                : "Balance details will appear when HR assigns a leave policy."}
            </p>
          </div>
          <span>{currentDate.slice(0, 4)}</span>
        </div>
        {initial.balances.length ? (
          <div className={styles.balanceList}>
            {initial.balances.map((item) => (
              <div className={styles.balanceItem} key={item.type.id}>
                <CalendarDays size={16} aria-hidden="true" />
                <strong>{item.type.name}</strong>
                <span className={styles.available}>
                  {item.type.tracked ? units(item.available) : "Untracked"}
                </span>
                <span className={styles.balanceMeta}>Available</span>
                <span className={styles.balanceMeta}>Reserved {units(item.reserved)}</span>
              </div>
            ))}
          </div>
        ) : (
          <p className={styles.quietEmpty}>No leave balance is available yet.</p>
        )}
      </section>

      <section className={styles.calendarSection} aria-labelledby="leave-calendar-title">
        <div className={styles.sectionHeading}>
          <div>
            <h2 id="leave-calendar-title">Time away</h2>
            <p>Select a day to see requests, holidays, and permitted team availability.</p>
          </div>
          <MonthNavigation appearance="standard"
            className={styles.monthNavigation}
            month={month}
            todayDate={currentDate}
            onMonthChange={changeMonth}
            onToday={() => changeMonth(currentMonth)}
          />
        </div>
        {calendarError && (
          <InlineNotice tone="danger" className={styles.inlineError} role="alert">
            <span>{calendarError}</span>
            <Button appearance="standard"
              variant="ghost"
              type="button"
              onClick={() => {
                setCalendarBusy(true);
                setCalendarError("");
                setTeamError("");
                setRetryMonth((value) => value + 1);
              }}
            >
              Retry
            </Button>
          </InlineNotice>
        )}
        {teamError && (
          <InlineNotice tone="info" className={styles.partialNotice} role="status">
            <span>{teamError}</span>
            <Button appearance="standard"
              variant="ghost"
              type="button"
              onClick={() => {
                setCalendarBusy(true);
                setTeamError("");
                setRetryMonth((value) => value + 1);
              }}
            >
              Retry
            </Button>
          </InlineNotice>
        )}
        {calendarBusy && !calendar && (
          <InlineNotice tone="info" className={styles.quietEmpty} role="status">
            Loading this month…
          </InlineNotice>
        )}
        <div className={styles.calendarFrame} aria-busy={calendarBusy}>
          <MonthGrid
            month={month}
            currentDate={currentDate}
            selectedDate={selectedDate}
            onSelectDate={(date) => {
              if (date.slice(0, 7) !== month) changeMonth(date.slice(0, 7));
              setSelectedDate(date);
            }}
            renderDay={(date) => (
              <>
                {dateRequests(date).map((request) => (
                  <Link
                    key={request.id}
                    className={styles.calendarItem + " " + (styles["state_" + request.state] ?? "")}
                    data-calendar-entry="request"
                    href={
                      base +
                      "/tasks/people_payroll/" +
                      encodeURIComponent(request.id) +
                      "?from=leave"
                    }
                    aria-label={
                      (request.leave_type_name ?? "Leave request") +
                      ", " +
                      requestStatus(request) +
                      ", " +
                      formatDay(date)
                    }
                    onClick={(event) => event.stopPropagation()}
                  >
                    <span>{request.leave_type_name ?? "Leave request"}</span>
                    <small>{requestStatus(request)}</small>
                  </Link>
                ))}
                {monthItemsForDate(date).map((item) => (
                  <div
                    key={item.id}
                    className={styles.calendarItem + " " + styles.teamItem}
                    data-calendar-entry="team"
                    title={item.title}
                  >
                    <span>{item.title}</span>
                    <small>Approved absence</small>
                  </div>
                ))}
                {dateHolidays(date).map((holiday) => (
                  <div
                    key={holiday.holiday_date + ":" + holiday.name}
                    className={styles.calendarItem + " " + styles.holidayItem}
                    data-calendar-entry="holiday"
                  >
                    <span>{holiday.name}</span>
                    <small>Holiday</small>
                  </div>
                ))}
              </>
            )}
          />
        </div>
        {selectedDate && (
          <section
            className={styles.daySummary}
            aria-live="polite"
            aria-labelledby="selected-day-title"
          >
            <div className={styles.daySummaryHeading}>
              <div>
                <p className={styles.eyebrow}>Selected day</p>
                <h3 id="selected-day-title">
                  {formatDay(selectedDate, {
                    weekday: "long",
                    day: "numeric",
                    month: "long",
                    year: "numeric",
                  })}
                </h3>
              </div>
              {selectedIsEmpty && (
                <Link
                  className={styles.dayRequestLink}
                  href={base + "/leave/new?date=" + selectedDate}
                >
                  <Plus size={15} /> Request this day
                </Link>
              )}
            </div>
            {calendarBusy && (
              <InlineNotice tone="info" className={styles.quietEmpty} role="status">
                Loading this day’s leave details…
              </InlineNotice>
            )}
            {!calendarBusy && !calendar && (
              <InlineNotice tone="danger" className={styles.inlineError} role="alert">
                Your leave calendar could not be loaded.
              </InlineNotice>
            )}
            {selectedRequests.length > 0 && (
              <div className={styles.daySummaryGroup}>
                <h4>Your requests</h4>
                {selectedRequests.map((request) => (
                  <Link
                    key={request.id}
                    href={
                      base +
                      "/tasks/people_payroll/" +
                      encodeURIComponent(request.id) +
                      "?from=leave"
                    }
                  >
                    <span>
                      {request.leave_type_name ?? "Leave request"} ·{" "}
                      {units(request.calculated_units ?? request.days)}
                    </span>
                    <strong>
                      {requestStatus(request)} <ChevronRight size={15} />
                    </strong>
                  </Link>
                ))}
              </div>
            )}
            {selectedTeam.length > 0 && (
              <div className={styles.daySummaryGroup}>
                <h4>Team availability</h4>
                {selectedTeam.map((item) => (
                  <p key={item.id}>
                    <span>{item.title}</span>
                    <small>Approved absence</small>
                  </p>
                ))}
              </div>
            )}
            {selectedHolidays.length > 0 && (
              <div className={styles.daySummaryGroup}>
                <h4>Assigned holidays</h4>
                {selectedHolidays.map((holiday) => (
                  <p key={holiday.holiday_date + ":" + holiday.name}>
                    <span>{holiday.name}</span>
                    <small>Holiday</small>
                  </p>
                ))}
              </div>
            )}
            {selectedIsEmpty && (
              <p className={styles.quietEmpty}>No requests or assigned holidays on this day.</p>
            )}
          </section>
        )}
      </section>

      <section className={styles.requestsSection} aria-labelledby="leave-requests-title">
        <div className={styles.sectionHeading}>
          <div>
            <h2 id="leave-requests-title">Your requests</h2>
            <p>One list for drafts, reviews, upcoming leave, and past decisions.</p>
          </div>
        </div>
        <div className={styles.filters} role="group" aria-label="Filter leave requests">
          {filters.map((item) => (
            <Button appearance="standard"
              variant="ghost"
              type="button"
              key={item.id}
              aria-pressed={filter === item.id}
              onClick={() => changeFilter(item.id)}
            >
              {item.label}
            </Button>
          ))}
        </div>
        {listError && (
          <InlineNotice tone="danger" className={styles.inlineError} role="alert">
            <span>{listError}</span>
            <Button appearance="standard"
              variant="ghost"
              type="button"
              onClick={() => {
                setListBusy(true);
                setListError("");
                setRetryList((value) => value + 1);
              }}
            >
              Retry
            </Button>
          </InlineNotice>
        )}
        {listBusy ? (
          <InlineNotice tone="info" className={styles.quietEmpty} role="status">
            Loading your requests…
          </InlineNotice>
        ) : requests.length ? (
          <div className={styles.requestList}>
            {requests.map((request) => (
              <Link
                className={styles.requestRow}
                key={request.id}
                href={
                  base + "/tasks/people_payroll/" + encodeURIComponent(request.id) + "?from=leave"
                }
              >
                <span className={styles.requestMark} aria-hidden="true" />
                <span className={styles.requestInfo}>
                  <strong>{request.leave_type_name ?? "Legacy annual leave"}</strong>
                  <span>
                    {formatDay(request.start_date)}
                    {request.end_date && request.end_date !== request.start_date
                      ? " – " + formatDay(request.end_date)
                      : ""}{" "}
                    · {units(request.calculated_units ?? request.days)}
                  </span>
                </span>
                <span className={styles.status}>{requestStatus(request)}</span>
                <ChevronRight size={17} aria-hidden="true" />
              </Link>
            ))}
            {nextCursor && (
              <Button appearance="standard"
                variant="ghost"
                type="button"
                className={styles.loadMore}
                disabled={moreBusy}
                onClick={() => {
                  setMoreBusy(true);
                  void loadRequests(filter, true, nextCursor);
                }}
              >
                {moreBusy ? "Loading…" : "Load more requests"}
              </Button>
            )}
          </div>
        ) : (
          !listError && (
            <div className={styles.listEmpty}>
              <CalendarDays size={23} />
              <strong>
                {filter === "all"
                  ? "No leave requests yet"
                  : "No " +
                    (filters.find((item) => item.id === filter)?.label.toLowerCase() ?? "") +
                    " requests"}
              </strong>
              <span>
                {filter === "all"
                  ? "Your saved drafts and submitted requests will appear here."
                  : "Try another filter to see more of your leave history."}
              </span>
              {filter === "all" && <Link href={base + "/leave/new"}>Create a request</Link>}
            </div>
          )
        )}
      </section>

      <details className={styles.policySection}>
        <summary>
          <span>
            <ShieldCheck size={17} /> Policy details and holidays
          </span>
          <ChevronRight size={17} aria-hidden="true" />
        </summary>
        <div className={styles.policyBody}>
          {initial.policy ? (
            <p>
              <strong>{initial.policy.name}</strong> · version {initial.policy.version}.
              Calculations use {initial.policy.timezone}; published roster assignments take priority
              over the policy workweek.
            </p>
          ) : (
            <p>{initial.setupMessage ?? "No active leave policy is assigned."}</p>
          )}
          <div className={styles.holidayList}>
            <h3>Holidays this month</h3>
            {(calendar?.holidays ?? []).length ? (
              calendar!.holidays.map((holiday) => (
                <div key={holiday.holiday_date + ":" + holiday.name}>
                  <time>{formatDay(holiday.holiday_date)}</time>
                  <span>{holiday.name}</span>
                </div>
              ))
            ) : (
              <p className={styles.quietEmpty}>
                {calendarBusy ? "Loading assigned holidays…" : "No assigned holidays this month."}
              </p>
            )}
          </div>
        </div>
      </details>
    </div>
  );
}
