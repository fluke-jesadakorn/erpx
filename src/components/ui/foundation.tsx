import Link from "next/link";
import {
  AlertCircle,
  CheckCircle2,
  Info,
  LoaderCircle,
  Search,
  TriangleAlert,
} from "lucide-react";
import {
  forwardRef,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import styles from "./foundation.module.css";

export type UiSize = "compact" | "default" | "spacious";
export type UiTone = "neutral" | "brand" | "success" | "warning" | "danger" | "info";
export type UiDensity = "compact" | "comfortable";

function classes(...values: Array<string | false | null | undefined>) {
  return values.filter(Boolean).join(" ");
}

export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: UiSize;
  pending?: boolean;
}>(function Button({ variant = "primary", size = "default", pending = false, disabled, children, className, type = "button", ...props }, ref) {
  return <button ref={ref} type={type} className={classes(styles.button, styles[variant], styles[size], className)} disabled={disabled || pending} aria-busy={pending || undefined} {...props}>
    {pending && <LoaderCircle className={styles.spinner} size={16} aria-hidden="true" />}
    <span>{children}</span>
  </button>;
});

export const IconButton = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  size?: UiSize;
  tone?: "neutral" | "danger";
}>(function IconButton({ label, size = "default", tone = "neutral", className, type = "button", children, ...props }, ref) {
  return <button ref={ref} type={type} className={classes(styles.iconButton, styles[size], styles[tone], className)} aria-label={label} title={label} {...props}>{children}</button>;
});

export function ActionLink({ href, children, variant = "secondary", size = "default", className, prefetch }: {
  href: string;
  children: ReactNode;
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: UiSize;
  className?: string;
  prefetch?: boolean;
}) {
  return <Link href={href} prefetch={prefetch} className={classes(styles.button, styles[variant], styles[size], className)}>{children}</Link>;
}

export function Field({ label, hint, error, required, children, className }: {
  label: string;
  hint?: string;
  error?: string;
  required?: boolean;
  children: ReactNode;
  className?: string;
}) {
  return <label className={classes(styles.field, className)}>
    <span className={styles.fieldLabel} data-required={required || undefined}>{label}</span>
    {children}
    {error ? <span className={styles.fieldError} role="alert">{error}</span> : hint ? <span className={styles.fieldHint}>{hint}</span> : null}
  </label>;
}

export const TextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { invalid?: boolean; inputSize?: UiSize }>(
  function TextInput({ invalid, inputSize = "default", className, ...props }, ref) {
    return <input ref={ref} className={classes(styles.control, styles[inputSize], className)} aria-invalid={invalid || undefined} {...props} />;
  },
);

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement> & { invalid?: boolean; inputSize?: UiSize }>(
  function Select({ invalid, inputSize = "default", className, children, ...props }, ref) {
    return <select ref={ref} className={classes(styles.control, styles[inputSize], className)} aria-invalid={invalid || undefined} {...props}>{children}</select>;
  },
);

export const TextArea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement> & { invalid?: boolean }>(
  function TextArea({ invalid, className, ...props }, ref) {
    return <textarea ref={ref} className={classes(styles.control, styles.textarea, className)} aria-invalid={invalid || undefined} {...props} />;
  },
);

export function SearchField(props: InputHTMLAttributes<HTMLInputElement>) {
  return <span className={styles.search}><Search size={16} aria-hidden="true" /><TextInput type="search" {...props} /></span>;
}

export function Checkbox({ label, description, ...props }: InputHTMLAttributes<HTMLInputElement> & { label: string; description?: string }) {
  return <label className={styles.choice}><input type="checkbox" {...props} /><span><strong>{label}</strong>{description && <small>{description}</small>}</span></label>;
}

export function Radio({ label, description, ...props }: InputHTMLAttributes<HTMLInputElement> & { label: string; description?: string }) {
  return <label className={styles.choice}><input type="radio" {...props} /><span><strong>{label}</strong>{description && <small>{description}</small>}</span></label>;
}

const alertIcons = { success: CheckCircle2, warning: TriangleAlert, danger: AlertCircle, info: Info, neutral: Info, brand: Info };
export function Alert({ tone = "info", title, children, actions, live = false }: { tone?: UiTone; title: string; children?: ReactNode; actions?: ReactNode; live?: boolean }) {
  const Icon = alertIcons[tone];
  return <section className={classes(styles.alert, styles[tone])} role={live || tone === "danger" ? "alert" : "status"}>
    <Icon size={20} aria-hidden="true" />
    <div><strong>{title}</strong>{children && <div className={styles.alertBody}>{children}</div>}{actions && <div className={styles.alertActions}>{actions}</div>}</div>
  </section>;
}

export function Card({ children, density = "comfortable", className, as: Element = "section" }: { children: ReactNode; density?: UiDensity; className?: string; as?: "article" | "div" | "section" }) {
  return <Element className={classes(styles.card, styles[density], className)}>{children}</Element>;
}

export function Toolbar({ children, label = "Page actions", className }: { children: ReactNode; label?: string; className?: string }) {
  return <div className={classes(styles.toolbar, className)} role="toolbar" aria-label={label}>{children}</div>;
}

export function ResponsiveActions({ children }: { children: ReactNode }) {
  return <div className={styles.responsiveActions}>{children}</div>;
}

export function Tabs({ label, children }: { label: string; children: ReactNode }) {
  return <nav className={styles.tabs} aria-label={label}>{children}</nav>;
}

export function TabLink({ href, current = false, children }: { href: string; current?: boolean; children: ReactNode }) {
  return <Link href={href} aria-current={current ? "page" : undefined}>{children}</Link>;
}

export function TableFrame({ label, children, density = "comfortable" }: { label: string; children: ReactNode; density?: UiDensity }) {
  return <div className={classes(styles.tableFrame, styles[density])} role="region" aria-label={label} tabIndex={0}>{children}</div>;
}

export function LoadingState({ label = "Loading" }: { label?: string }) {
  return <div className={styles.loading} role="status" aria-live="polite"><LoaderCircle className={styles.spinner} aria-hidden="true" />{label}</div>;
}
