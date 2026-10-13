import { Button } from "@/components/ui/foundation";
import type { ReactNode } from "react";
import { addDays, rosterMonday } from "@/lib/people_payroll/people-roster";
import styles from "@/components/work_projection/calendar/calendar.module.css";

const weekdays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function formatDay(date: string) {
  return new Intl.DateTimeFormat("en", { day: "numeric", timeZone: "UTC" }).format(
    new Date(`${date}T12:00:00Z`),
  );
}

export function MonthGrid({
  month,
  currentDate,
  selectedDate,
  dropTargetDate,
  onSelectDate,
  eventCount,
  renderMore,
  renderDay,
}: {
  month: string;
  currentDate?: string;
  selectedDate?: string | null;
  dropTargetDate?: string | null;
  onSelectDate?: (date: string) => void;
  eventCount?: (date: string) => number;
  renderMore?: (date: string, count: number) => ReactNode;
  renderDay: (date: string) => ReactNode;
}) {
  const from = `${month}-01`;
  const monthLength = new Date(
    Date.UTC(Number(month.slice(0, 4)), Number(month.slice(5, 7)), 0),
  ).getUTCDate();
  const offset = (new Date(`${from}T12:00:00Z`).getUTCDay() + 6) % 7;
  const cellCount = Math.ceil(offset / 7 + monthLength / 7) * 7;
  const firstCell = rosterMonday(from);

  return (
    <div className={styles.scroll} data-calendar-view="month">
      <div className={styles.month} data-month-grid>
        {weekdays.map((day) => (
          <div key={day} className={styles.dayHeading} data-calendar-weekday>
            {day}
          </div>
        ))}
        {Array.from({ length: cellCount }, (_, index) => addDays(firstCell, index)).map((date) => {
          const inMonth = date.slice(0, 7) === month;
          const count = eventCount?.(date) ?? 0;
          return (
            <section
              key={date}
              data-calendar-date={date}
              data-selected={selectedDate === date || undefined}
              data-event-count={count}
              className={
                styles.monthDay +
                " " +
                (!inMonth ? styles.outside : "") +
                " " +
                (date === currentDate ? styles.today : "") +
                " " +
                (date === dropTargetDate ? styles.dateDropTarget : "")
              }
            >
              <header>
                {onSelectDate ? (
                  <Button
                    variant="plain"
                    type="button"
                    aria-label={`Show ${new Intl.DateTimeFormat("en", { dateStyle: "full", timeZone: "UTC" }).format(new Date(`${date}T12:00:00Z`))}`}
                    aria-pressed={selectedDate === date}
                    onClick={() => onSelectDate(date)}
                  >
                    {formatDay(date)}
                  </Button>
                ) : (
                  formatDay(date)
                )}
                {count > 0 && (
                  <span
                    className={styles.monthEventCount}
                    data-calendar-event-count
                    aria-label={`${count} loaded ${count === 1 ? "event" : "events"}`}
                  >
                    {count}
                  </span>
                )}
              </header>
              <div className={styles.monthContents}>
                {eventCount ? (
                  <div className={styles.monthPreviews} data-month-event-previews>
                    {renderDay(date)}
                  </div>
                ) : (
                  renderDay(date)
                )}
                {renderMore && count > 2 && renderMore(date, count - 2)}
              </div>
            </section>
          );
        })}
      </div>
    </div>
  );
}
