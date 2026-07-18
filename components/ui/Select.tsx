'use client';
import React from 'react';
import { ChevronDown } from 'lucide-react';

export type SelectSize = 'sm' | 'md' | 'lg';

export interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  selectSize?: SelectSize;
  invalid?: boolean;
}

const SIZE: Record<SelectSize, string> = {
  sm: 'h-9 pl-3 pr-8 text-xs',
  md: 'h-10 pl-3.5 pr-9 text-sm',
  lg: 'h-11 pl-4 pr-10 text-sm',
};

const BASE = 'glass-input w-full appearance-none text-ink disabled:opacity-50 disabled:pointer-events-none cursor-pointer';

export const Select = React.forwardRef<HTMLSelectElement, SelectProps>(
  ({ selectSize = 'md', invalid, className = '', children, ...props }, ref) => (
    <div className="relative">
      <select
        ref={ref}
        className={[
          BASE,
          SIZE[selectSize],
          invalid && 'border-critical focus:border-critical focus-visible:shadow-[0_0_0_3px_color-mix(in_oklab,var(--critical)_28%,transparent)]',
          className,
        ].join(' ')}
        {...props}
      >
        {children}
      </select>
      <ChevronDown className="absolute right-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-mute pointer-events-none" aria-hidden />
    </div>
  )
);
Select.displayName = 'Select';
export default Select;
