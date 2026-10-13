"use client";
import { Field, Select, TextInput } from "@/components/ui/foundation";

import { useEffect, useMemo, useRef, useState } from "react";
import { localDateTimeCandidates, localMinuteAt } from "@/lib/work_projection/calendar-timeline";
import { utcOffset } from "@/lib/shared/ui-format";
import { useDisplayTimeZone } from "@/components/people_payroll/time/time-preference-context";
import { type UiAppearance } from "@/components/ui/foundation";

/** A generic instant editor. Domain wall-clock fields keep their own timezone controls. */
export function PreferredDateTimeField({ appearance = "legacy",
  name,
  label,
  required = false,
  step,
  initialInstant,
  initialLocal = "",
  valueLocal,
  onLocalChange,
  onResolvedChange,
}: ({
  name: string;
  label: string;
  required?: boolean;
  step?: number;
  initialInstant?: number | null;
  initialLocal?: string;
  valueLocal?: string;
  onLocalChange?: (value: string) => void;
  onResolvedChange?: (value: string | null) => void;
}) & { appearance?: UiAppearance }) {
  const { timeZone, ready } = useDisplayTimeZone();
  const initial =
    initialInstant == null
      ? initialLocal
      : (() => {
          const local = localMinuteAt(initialInstant, timeZone);
          return `${local.date}T${local.time}`;
        })();
  const [internalLocal, setInternalLocal] = useState<string | null>(null);
  const local = valueLocal ?? internalLocal ?? initial;
  const setLocal = (value: string) => {
    if (valueLocal === undefined) setInternalLocal(value);
    onLocalChange?.(value);
  };
  const [choice, setChoice] = useState<number | null | undefined>(undefined);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const form = input.current?.form;
    if (!form) return;
    const reset = () => {
      if (valueLocal === undefined) setInternalLocal(null);
      onLocalChange?.(initial);
      setChoice(undefined);
    };
    form.addEventListener("reset", reset);
    return () => form.removeEventListener("reset", reset);
  }, [initial, onLocalChange, valueLocal]);
  const candidates = useMemo(() => {
    if (!ready || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(?::\d{2})?$/.test(local)) return [];
    const seconds = local.length > 16 ? Number(local.slice(17)) : 0;
    return localDateTimeCandidates(local.slice(0, 16), timeZone).map(
      (value) => value + seconds * 1000,
    );
  }, [local, timeZone, ready]);
  const preferred = choice === undefined ? (initialInstant ?? null) : choice;
  const selected =
    candidates.length === 1
      ? candidates[0]
      : candidates.includes(preferred ?? -1)
        ? preferred
        : null;
  const resolved = selected === null ? null : new Date(selected).toISOString();
  const lastResolved = useRef<string | null | undefined>(undefined);
  useEffect(() => {
    if (lastResolved.current !== resolved) {
      lastResolved.current = resolved;
      onResolvedChange?.(resolved);
    }
  }, [resolved, onResolvedChange]);
  const invalid =
    local && ready && !candidates.length
      ? `This local time does not exist in ${timeZone}. Choose another time.`
      : local && candidates.length > 1 && selected === null
        ? "This time occurs twice. Choose the UTC offset."
        : "";
  useEffect(() => {
    input.current?.setCustomValidity(invalid || (!ready ? "Wait for your timezone to load." : ""));
  }, [invalid, ready]);
  return (
    <div data-ui-appearance={appearance} className="preferred-date-time-field">
      <Field appearance={appearance}>
        {label}
        <TextInput appearance={appearance}
          ref={input}
          aria-label={label}
          type="datetime-local"
          step={step}
          required={required}
          disabled={!ready}
          value={local}
          onChange={(event) => {
            setLocal(event.target.value);
            setChoice(null);
          }}
        />
      </Field>
      <small data-ui-appearance={appearance}>
        Timezone: {ready ? timeZone : "Loading…"}
        {selected !== null ? ` · ${utcOffset(selected, timeZone)}` : ""}
      </small>
      {candidates.length > 1 && (
        <Field appearance={appearance}>
          UTC offset
          <Select appearance={appearance}
            aria-label={`${label} UTC offset`}
            value={selected ?? ""}
            onChange={(event) => setChoice(event.target.value ? Number(event.target.value) : null)}
          >
            <option data-ui-appearance={appearance} value="">Choose an occurrence</option>
            {candidates.map((value) => (
              <option data-ui-appearance={appearance} key={value} value={value}>
                {utcOffset(value, timeZone)}
              </option>
            ))}
          </Select>
        </Field>
      )}
      {invalid && <small data-ui-appearance={appearance} role="alert">{invalid}</small>}
      <TextInput appearance={appearance} type="hidden" name={name} value={resolved ?? ""} />
    </div>
  );
}
