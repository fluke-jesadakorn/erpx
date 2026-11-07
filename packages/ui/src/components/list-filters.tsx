"use client";
import {DestinationFilterForm} from "@erpx/ui/components/destination-filter-form";
import {usePathname} from 'next/navigation';
import { NavigationLink as Link, useUiNavigation } from "@erpx/ui/components/navigation-context";
import { Button, SearchField, Select, FilterBar, type UiAppearance } from "@erpx/ui/components/foundation";
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
  const pathname=usePathname(), navigation=useUiNavigation();
  const {action:target,preserve:retained}=navigation.resolveFilters(pathname,action,preserve);
  const Element = appearance === "standard" ? FilterBar : DestinationFilterForm;
  return (
    <Element className={appearance === "standard" ? undefined : "work-filters"} action={target} role="search">
      {Object.entries(retained).flatMap(([name, value]) => (Array.isArray(value)?value:[value]).map((item,index)=><input key={`${name}:${index}`} type="hidden" name={name} value={item}/>))}
      <SearchField key={query} appearance={appearance}
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
          href={navigation.filterHref(target,retained)}
        >
          Clear filters
        </Link>
      )}
    </Element>
  );
}
