import React from 'react';

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'critical' | 'positive';
export type ButtonSize = 'sm' | 'md' | 'lg';

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  leftIcon?: React.ReactNode;
  rightIcon?: React.ReactNode;
}

const VARIANT: Record<ButtonVariant, string> = {
  primary: 'border border-accent/70 bg-accent text-paper shadow-[0_10px_28px_-14px_color-mix(in_oklab,var(--accent)_80%,transparent)] hover:bg-accent-strong',
  secondary: 'glass-input text-ink hover:bg-paper-3/70',
  ghost: 'border border-transparent bg-transparent text-ink-2 hover:border-rule hover:bg-paper-3/45 hover:text-ink',
  critical: 'border border-critical/70 bg-critical text-paper shadow-[0_10px_28px_-16px_color-mix(in_oklab,var(--critical)_70%,transparent)] hover:bg-critical-strong',
  positive: 'border border-positive/70 bg-positive text-paper shadow-[0_10px_28px_-16px_color-mix(in_oklab,var(--positive)_70%,transparent)] hover:bg-positive-strong',
};

const SIZE: Record<ButtonSize, string> = {
  sm: 'h-9 px-3 text-xs rounded-md gap-1.5',
  md: 'h-10 px-3.5 text-sm rounded-md gap-2',
  lg: 'h-11 px-4 text-sm rounded-lg gap-2',
};

const BASE =
  'inline-flex items-center justify-center font-medium tracking-[-0.01em] transition-[transform,background-color,border-color,color,box-shadow] duration-[var(--dur-base)] hover:-translate-y-px active:translate-y-0 disabled:opacity-45 disabled:pointer-events-none disabled:transform-none whitespace-nowrap select-none';

const Spinner = () => (
  <span
    className="inline-block h-3.5 w-3.5 animate-spin rounded-full border-2 border-current border-t-transparent"
    aria-hidden="true"
  />
);

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      variant = 'secondary',
      size = 'md',
      className = '',
      loading = false,
      leftIcon,
      rightIcon,
      children,
      disabled,
      type = 'button',
      ...props
    },
    ref
  ) => (
    <button
      ref={ref}
      type={type}
      disabled={disabled || loading}
      className={[BASE, VARIANT[variant], SIZE[size], className].join(' ')}
      {...props}
    >
      {loading ? <Spinner /> : leftIcon}
      {children}
      {!loading && rightIcon}
    </button>
  )
);
Button.displayName = 'Button';
export default Button;
