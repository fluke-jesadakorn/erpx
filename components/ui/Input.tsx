import React from 'react';

export type InputSize = 'sm' | 'md' | 'lg';

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  inputSize?: InputSize;
  invalid?: boolean;
  leftIcon?: React.ReactNode;
}

const SIZE: Record<InputSize, string> = {
  sm: 'h-9 px-3 text-xs',
  md: 'h-10 px-3.5 text-sm',
  lg: 'h-11 px-4 text-sm',
};

const ICON_PAD: Record<InputSize, string> = {
  sm: 'pl-9',
  md: 'pl-10',
  lg: 'pl-11',
};

const BASE =
  'glass-input w-full text-ink placeholder:text-mute/85 disabled:opacity-50 disabled:pointer-events-none';

const INVALID =
  'border-critical focus:border-critical focus-visible:shadow-[0_0_0_3px_color-mix(in_oklab,var(--critical)_28%,transparent)]';

export const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ inputSize = 'md', invalid, leftIcon, className = '', ...props }, ref) => {
    const inputClass = [
      BASE,
      SIZE[inputSize],
      leftIcon && ICON_PAD[inputSize],
      invalid && INVALID,
      className,
    ]
      .filter(Boolean)
      .join(' ');

    if (!leftIcon) {
      return <input ref={ref} className={inputClass} {...props} />;
    }

    return (
      <div className="relative">
        <span className="absolute left-3 top-1/2 -translate-y-1/2 text-mute pointer-events-none flex items-center">
          {leftIcon}
        </span>
        <input ref={ref} className={inputClass} {...props} />
      </div>
    );
  }
);
Input.displayName = 'Input';
export default Input;
