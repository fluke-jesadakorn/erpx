import type { LinkProps } from "next/link";
import { NavigationLink as Link } from "@erpx/ui/components/navigation-context";
import { AlertCircle, CheckCircle2, Info, LoaderCircle, Search, TriangleAlert } from "lucide-react";
import {
  forwardRef,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";
import { cn } from "@erpx/ui/lib/cn";
import { Button } from "@erpx/ui/components/button";
import { buttonVariants, type UiSize } from "@erpx/ui/components/button-variants";
import { TextInput } from "@erpx/ui/components/input";

import type { UiAppearance } from "@erpx/ui/components/appearance";
import type { ActionAnchorProps as ActionProps } from "@erpx/ui/components/action-anchor";

export { Menu, MenuTrigger, MenuContent, MenuItem, MenuSeparator, Popover, PopoverTrigger, PopoverContent, PopoverClose, Combobox, LocalTabs, TabsList, TabsTrigger, TabsPanel, Tooltip, TooltipProvider } from "@erpx/ui/components/popups";
export { PageContent, Stack, Panel, SectionHeader, FormGrid, FormLayout, FormActions, FilterBar, RecordList, RecordListItem, Badge, InlineNotice } from "@erpx/ui/components/patterns";
export type { UiAppearance } from "@erpx/ui/components/appearance";

export { Button } from "@erpx/ui/components/button";
export { Card, type UiDensity } from "@erpx/ui/components/card";
export { ActionAnchor } from "@erpx/ui/components/action-anchor";
export { Field } from "@erpx/ui/components/field";
export { TextInput, Select, TextArea } from "@erpx/ui/components/input";
export { Table, TableFrame } from "@erpx/ui/components/table";
export type { UiSize } from "@erpx/ui/components/button-variants";
export type UiTone = "neutral" | "brand" | "success" | "warning" | "danger" | "info";

export const IconButton = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & {
    label: string;
    appearance?: UiAppearance;
    size?: UiSize;
    tone?: "neutral" | "danger";
  }
>(function IconButton(
  { label, size = "default", appearance = "legacy", tone = "neutral", className, children, ...props },
  ref,
) {
  const widths = { compact: "w-9", default: "w-11", spacious: "w-12" };
  return (
    <Button
      ref={ref}
      appearance={appearance}
      variant="secondary"
      size={size}
      aria-label={label}
      title={label}
      className={cn(
        "aspect-square p-0",
        widths[size],
        tone === "danger" && "border-[var(--ui-red)] bg-[var(--ui-red-soft)] text-[var(--ui-red)]",
        className,
      )}
      {...props}
    >
      {children}
    </Button>
  );
});

export function ActionLink({
  href,
  children,
  variant = "secondary",
  appearance = "legacy",
  size = "default",
  className,
  ...props
}: ActionProps & LinkProps) {
  return (
    <Link
      href={href}
      data-slot="button"
      data-size={size}
      data-variant={variant}
      data-appearance={appearance === "standard" ? appearance : undefined}
      className={cn(buttonVariants({ variant, size }), appearance === "standard" && "ui-standard-control", className)}
      {...props}
    >
      {children}
    </Link>
  );
}

export function SearchField(props: InputHTMLAttributes<HTMLInputElement> & { appearance?: UiAppearance }) {
  return (
    <span className="ui-search-field relative flex min-w-0 items-center">
      <Search
        size={16}
        aria-hidden="true"
        className="pointer-events-none absolute left-3 text-[var(--ui-muted)]"
      />
      <TextInput type="search" {...props} className={cn("pl-9", props.className)} />
    </span>
  );
}

export function Checkbox({
  label,
  description,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string; description?: string }) {
  return (
    <label className="flex min-h-11 cursor-pointer items-start gap-2.5 py-2.5">
      <TextInput type="checkbox" className="mt-0.5 size-4.5 shrink-0" {...props} />
      <span className="grid gap-0.5">
        <strong className="text-sm font-semibold">{label}</strong>
        {description && <small className="text-xs text-[var(--ui-muted)]">{description}</small>}
      </span>
    </label>
  );
}
export function Radio({
  label,
  description,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string; description?: string }) {
  return (
    <label className="flex min-h-11 cursor-pointer items-start gap-2.5 py-2.5">
      <TextInput type="radio" className="mt-0.5 size-4.5 shrink-0" {...props} />
      <span className="grid gap-0.5">
        <strong className="text-sm font-semibold">{label}</strong>
        {description && <small className="text-xs text-[var(--ui-muted)]">{description}</small>}
      </span>
    </label>
  );
}

const alertIcons = {
  success: CheckCircle2,
  warning: TriangleAlert,
  danger: AlertCircle,
  info: Info,
  neutral: Info,
  brand: Info,
};
const alertTones = {
  neutral: "border-[var(--ui-line)] bg-[var(--ui-card)] text-[var(--ui-ink)]",
  success: "border-[var(--ui-green)] bg-[var(--ui-green-soft)] text-[var(--ui-green)]",
  warning: "border-[var(--ui-amber)] bg-[var(--ui-amber-soft)] text-[var(--ui-amber)]",
  danger: "border-[var(--ui-red)] bg-[var(--ui-red-soft)] text-[var(--ui-red)]",
  info: "border-[var(--ui-brand)] bg-[var(--ui-brand-soft)] text-[var(--ui-brand)]",
  brand: "border-[var(--ui-brand)] bg-[var(--ui-brand-soft)] text-[var(--ui-brand)]",
};
export function Alert({
  tone = "info",
  title,
  children,
  actions,
  live = false,
}: {
  tone?: UiTone;
  title: string;
  children?: ReactNode;
  actions?: ReactNode;
  live?: boolean;
}) {
  const Icon = alertIcons[tone];
  return (
    <section
      data-slot="alert"
      className={cn(
        "grid grid-cols-[auto_minmax(0,1fr)] gap-3 rounded-[var(--ui-radius-lg)] border p-4 text-sm",
        alertTones[tone],
      )}
      role={live || tone === "danger" ? "alert" : "status"}
    >
      <Icon size={20} aria-hidden="true" />
      <div>
        <strong className="block text-[var(--ui-ink-strong)]">{title}</strong>
        {children && <div className="mt-1 leading-relaxed text-[var(--ui-muted)]">{children}</div>}
        {actions && <div className="mt-3 flex flex-wrap gap-2">{actions}</div>}
      </div>
    </section>
  );
}

export function Toolbar({
  children,
  label = "Page actions",
  className,
}: {
  children: ReactNode;
  label?: string;
  className?: string;
}) {
  return (
    <div
      className={cn("flex min-w-0 flex-wrap items-center gap-2", className)}
      role="toolbar"
      aria-label={label}
    >
      {children}
    </div>
  );
}
export function ResponsiveActions({ children }: { children: ReactNode }) {
  return (
    <div className="ui-responsive-actions flex flex-wrap items-center justify-end gap-2">
      {children}
    </div>
  );
}
export function Tabs({ label, children, appearance = "legacy" }: { label: string; children: ReactNode; appearance?: UiAppearance }) {
  return (
    <nav
      className={appearance === "standard" ? "ui-tab-nav" : "ui-tabs flex items-center gap-6 overflow-x-auto border-b border-[var(--ui-line)]"}
      aria-label={label}
    >
      {children}
    </nav>
  );
}
export function TabLink({
  href,
  current = false,
  children,
}: {
  href: string;
  current?: boolean;
  children: ReactNode;
}) {
  return (
    <Link
      className="inline-flex min-h-11 items-center whitespace-nowrap border-b-2 border-transparent py-1.5 text-sm text-[var(--ui-muted)] aria-[current=page]:border-[var(--ui-brand)] aria-[current=page]:font-semibold aria-[current=page]:text-[var(--ui-brand)]"
      href={href}
      aria-current={current ? "page" : undefined}
    >
      {children}
    </Link>
  );
}
export function LoadingState({ label = "Loading" }: { label?: string }) {
  return (
    <div
      className="flex min-h-30 items-center justify-center gap-2 text-sm text-[var(--ui-muted)]"
      role="status"
      aria-live="polite"
    >
      <LoaderCircle className="animate-spin motion-reduce:animate-none" aria-hidden="true" />
      {label}
    </div>
  );
}

/** URL navigation stays links; local-state tabs use LocalTabs. */
export const TabNav = Tabs;
