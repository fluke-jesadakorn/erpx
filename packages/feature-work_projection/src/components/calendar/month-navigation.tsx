import { Button, TextInput } from "@erpx/ui/components/foundation";
import { ChevronLeft, ChevronRight } from "lucide-react";
import styles from "@erpx/feature-work_projection/components/calendar/calendar.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";

function formatMonth(month: string) {
  return new Intl.DateTimeFormat("en", {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(`${month}-01T12:00:00Z`));
}

export function MonthNavigation({ appearance = "legacy",
  month,
  date,
  todayDate,
  onMonthChange,
  onDateChange,
  onToday,
  className,
}: ({
  month: string;
  date?: string;
  todayDate?: string;
  onMonthChange: (month: string) => void;
  onDateChange?: (date: string) => void;
  onToday?: () => void;
  className?: string;
}) & { appearance?: UiAppearance }) {
  function shiftMonth(direction: number) {
    const next = new Date(`${month}-01T12:00:00Z`);
    next.setUTCMonth(next.getUTCMonth() + direction);
    onMonthChange(next.toISOString().slice(0, 7));
  }

  return (
    <div data-ui-appearance={appearance}
      className={[styles.monthNavigation, className].filter(Boolean).join(" ")}
      aria-label="Month navigation"
    >
      <Button appearance={appearance}
        variant="ghost"
        type="button"
        className={styles.iconButton}
        aria-label="Previous month"
        onClick={() => shiftMonth(-1)}
      >
        <ChevronLeft size={17} />
      </Button>
      {onDateChange && date ? (
        <TextInput appearance={appearance}
          aria-label="Calendar date"
          type="date"
          value={date}
          onChange={(event) => event.target.value && onDateChange(event.target.value)}
        />
      ) : (
        <strong data-ui-appearance={appearance} aria-live="polite">{formatMonth(month)}</strong>
      )}
      <Button appearance={appearance}
        variant="ghost"
        type="button"
        className={styles.iconButton}
        aria-label="Next month"
        onClick={() => shiftMonth(1)}
      >
        <ChevronRight size={17} />
      </Button>
      {onToday && todayDate && (
        <Button appearance={appearance} variant="plain" type="button" className={styles.monthToday} onClick={onToday}>
          Today
        </Button>
      )}
    </div>
  );
}
