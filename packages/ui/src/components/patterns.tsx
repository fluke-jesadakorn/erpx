import {DestinationFilterForm} from "@erpx/ui/components/destination-filter-form";
import type { ComponentPropsWithRef, ElementType, HTMLAttributes, ReactNode } from "react";
import { cn } from "@erpx/ui/lib/cn";
import type { UiAppearance } from "@erpx/ui/components/appearance";

type BoxProps = HTMLAttributes<HTMLElement> & { as?: ElementType };
export function PageContent({ as: Element = "div", className, ...props }: BoxProps) {
  return <Element {...props} data-slot="page-content" className={cn("ui-page-content", className)} />;
}
export function Stack({ as: Element = "div", className, ...props }: BoxProps) {
  return <Element {...props} data-slot="stack" className={cn("ui-stack", className)} />;
}
export function SectionHeader({ className, ...props }: HTMLAttributes<HTMLElement>) {
  return <header {...props} data-slot="section-header" className={cn("ui-section-header", className)} />;
}
export function FormGrid({ as: Element = "div", className, ...props }: BoxProps) {
  return <Element {...props} data-slot="form-grid" className={cn("ui-form-grid", className)} />;
}
export function FormLayout({ className, layout = "grid", ...props }: ComponentPropsWithRef<"form"> & { layout?: "grid" | "stack" }) {
  return <form {...props} data-slot="form-layout" data-layout={layout} className={cn(layout === "grid" ? "ui-form-grid" : "ui-stack", className)} />;
}
export function FilterBar({ className, ...props }: ComponentPropsWithRef<"form">) {
  return <DestinationFilterForm {...props} role={props.role ?? "search"} data-slot="filter-bar" className={cn("ui-filter-bar", className)} />;
}
export function FormActions({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div {...props} data-slot="form-actions" className={cn("ui-form-actions", className)} />;
}
export function RecordList({ as: Element = "div", className, ...props }: BoxProps) {
  return <Element {...props} data-slot="record-list" className={cn("ui-record-list", className)} />;
}
export function RecordListItem({ as: Element = "article", className, ...props }: BoxProps) {
  return <Element {...props} data-slot="record-list-item" className={cn("ui-record-item", className)} />;
}
export function Badge({ tone = "neutral", className, ...props }: HTMLAttributes<HTMLSpanElement> & {
  tone?: "neutral" | "info" | "success" | "warning" | "danger";
}) {
  return <span {...props} data-slot="badge" data-tone={tone} className={cn("ui-badge", className)} />;
}
export function InlineNotice({ tone = "info", children, className, ...props }: HTMLAttributes<HTMLDivElement> & {
  tone?: "info" | "success" | "warning" | "danger";
}) {
  return <div {...props} role={props.role ?? (tone === "danger" ? "alert" : "status")} data-slot="notice" data-tone={tone} className={cn("ui-notice", className)}>{children}</div>;
}
export function Panel({ title, description, actions, children, className, appearance = "standard", ...props }: HTMLAttributes<HTMLElement> & {
  title?: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  appearance?: UiAppearance;
}) {
  return <section {...props} data-slot="panel" data-appearance={appearance} className={cn(appearance === "standard" && "ui-panel", className)}>
    {(title || description || actions) && <SectionHeader><div>{title && <h2 className="ui-section-title">{title}</h2>}{description && <p className="ui-description">{description}</p>}</div>{actions && <div className="ui-form-actions">{actions}</div>}</SectionHeader>}
    {children}
  </section>;
}
