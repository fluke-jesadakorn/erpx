import { ChevronLeft, ChevronRight } from "lucide-react";
import styles from "./calendar.module.css";

function formatMonth(month: string) {
  return new Intl.DateTimeFormat("en", {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(`${month}-01T12:00:00Z`));
}

export function MonthNavigation({
  month,
  date,
  todayDate,
  onMonthChange,
  onDateChange,
  onToday,
  className,
}: {
  month: string;
  date?: string;
  todayDate?: string;
  onMonthChange: (month: string) => void;
  onDateChange?: (date: string) => void;
  onToday?: () => void;
  className?: string;
}) {
  function shiftMonth(direction: number) {
    const next = new Date(`${month}-01T12:00:00Z`);
    next.setUTCMonth(next.getUTCMonth() + direction);
    onMonthChange(next.toISOString().slice(0, 7));
  }

  return (
    <div className={[styles.monthNavigation, className].filter(Boolean).join(" ")} aria-label="Month navigation">
      <button
        type="button"
        className={styles.iconButton}
        aria-label="Previous month"
        onClick={() => shiftMonth(-1)}
      >
        <ChevronLeft size={17} />
      </button>
      {onDateChange && date ? (
        <input
          aria-label="Calendar date"
          type="date"
          value={date}
          onChange={(event) => event.target.value && onDateChange(event.target.value)}
        />
      ) : (
        <strong aria-live="polite">{formatMonth(month)}</strong>
      )}
      <button
        type="button"
        className={styles.iconButton}
        aria-label="Next month"
        onClick={() => shiftMonth(1)}
      >
        <ChevronRight size={17} />
      </button>
      {onToday && todayDate && (
        <button type="button" className={styles.monthToday} onClick={onToday}>
          Today
        </button>
      )}
    </div>
  );
}
