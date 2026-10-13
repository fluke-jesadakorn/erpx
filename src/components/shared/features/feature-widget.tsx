"use client";
import { useContext, useEffect, useRef } from "react";
import { GridLocation, InsideWidget, WidgetRenderer, type FeatureWidgetProps } from "@/components/shared/features/widget-context";
import styles from "@/components/shared/features/features.module.css";

// Fixed presentation widths formerly supplied by the configurable-widget registry.
const fixedColumns: Record<string, number> = {
  "pos-products-menu": 6,
  "pos-products-menus-recipes": 6,
  "home-schedule": 4,
  "home-attention": 4,
  "home-requests": 4,
};
export function FeatureWidget(props: FeatureWidgetProps) {
  const renderWidget = useContext(WidgetRenderer);
  if (renderWidget) {
    const { appearance: _appearance, ...legacyProps } = props;
    void _appearance;
    return renderWidget(legacyProps);
  }
  return <FixedWidget {...props} />;
}
function FixedWidget({
  id,
  title,
  children,
  className = "",
  columns = fixedColumns[id] ?? 12,
  appearance = "legacy",
  ...props
}: FeatureWidgetProps) {
  const nested = useContext(InsideWidget),
    grid = useContext(GridLocation),
    ref = useRef<HTMLElement>(null);
  const targeted = !!grid?.target && (id === grid.target || id.endsWith(`-${grid.target}`));
  useEffect(() => {
    if (targeted) {
      ref.current?.scrollIntoView({ block: "start" });
      ref.current?.focus({ preventScroll: true });
    }
  }, [targeted]);
  if (nested)
    return (
      <section {...props} id={id} className={className}>
        <h3>{title}</h3>
        {children}
      </section>
    );
  return (
    <section
      {...props}
      ref={ref}
      id={id}
      tabIndex={-1}
      data-feature-widget={id}
      className={`${styles.widget} ${appearance === "standard" ? "ui-standard-widget" : ""} ${className}`}
      style={{ ...props.style, gridColumn: `span ${columns}` }}
    >
      <header className={`${styles.widgetHeading} ${appearance === "standard" ? "ui-section-header" : ""}`}><h2>{title}</h2></header>
      <InsideWidget value={true}>
        <div data-slot="widget-content" className={`${styles.widgetContent} ${appearance === "standard" ? "ui-standard-widget-content" : ""}`}>{children}</div>
      </InsideWidget>
    </section>
  );
}
