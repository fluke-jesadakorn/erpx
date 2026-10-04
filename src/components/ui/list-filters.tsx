import Link from "next/link";
import { Button, SearchField, Select, FilterBar, type UiAppearance } from "./foundation";
export function ListFilters({
  action,
  appearance = "legacy",
  query = "",
  placeholder = "Search",
  status,
  statuses,
  preserve = {},
}: {
  action: string;
  appearance?: UiAppearance;
  query?: string;
  placeholder?: string;
  status?: string;
  statuses?: readonly (readonly [string, string])[];
  preserve?: Record<string, string>;
}) {
  const Element = appearance === "standard" ? FilterBar : "form";
  return (
    <Element className={appearance === "standard" ? undefined : "work-filters"} action={action} role="search">
      {Object.entries(preserve).map(([name, value]) => (
        <input key={name} type="hidden" name={name} value={value} />
      ))}
      <SearchField appearance={appearance}
        name="q"
        aria-label={placeholder}
        placeholder={placeholder}
        defaultValue={query}
      />
      {statuses && (
        <Select appearance={appearance} name="status" aria-label="Status" defaultValue={status ?? "all"}>
          {statuses.map(([id, label]) => (
            <option key={id} value={id}>
              {label}
            </option>
          ))}
        </Select>
      )}
      <Button appearance={appearance} type="submit" variant="secondary">
        Search
      </Button>
      {(query || (status && status !== "all")) && (
        <Link
          className={appearance === "standard" ? "button ghost ui-standard-control" : "button ghost"}
          data-slot="button"
          href={`${action}${Object.keys(preserve).length ? `?${new URLSearchParams(preserve)}` : ""}`}
        >
          Clear filters
        </Link>
      )}
    </Element>
  );
}
