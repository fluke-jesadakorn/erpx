import Link from "next/link";
import {routeContextHref} from "@/lib/shared/route-context";
import { Button, SearchField, Select, FilterBar, type UiAppearance } from "./foundation";
export function ListFilters({
  action,
  children,
  appearance = "legacy",
  query = "",
  placeholder = "Search",
  status,
  statuses,
  preserve = {},
}: {
  action: string;
  children?: import("react").ReactNode;
  appearance?: UiAppearance;
  query?: string;
  placeholder?: string;
  status?: string;
  statuses?: readonly (readonly [string, string])[];
  preserve?: Record<string, string | string[]>;
}) {
  const Element = appearance === "standard" ? FilterBar : "form";
  return (
    <Element className={appearance === "standard" ? undefined : "work-filters"} action={action} role="search">
      {Object.entries(preserve).flatMap(([name, value]) => (Array.isArray(value)?value:[value]).map((item,index)=><input key={`${name}:${index}`} type="hidden" name={name} value={item}/>))}
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
      {children}
      <Button appearance={appearance} type="submit" variant="secondary">
        Search
      </Button>
      {(query || (status && status !== "all")) && (
        <Link
          className={appearance === "standard" ? "button ghost ui-standard-control" : "button ghost"}
          data-slot="button"
          href={routeContextHref(action,preserve)}
        >
          Clear filters
        </Link>
      )}
    </Element>
  );
}
