import React from 'react';

export type TextareaSize = 'sm' | 'md' | 'lg';

export interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  textareaSize?: TextareaSize;
  invalid?: boolean;
}

const SIZE: Record<TextareaSize, string> = {
  sm: 'px-3 py-2 text-xs',
  md: 'px-3.5 py-2.5 text-sm',
  lg: 'px-4 py-3 text-sm',
};

const BASE = 'glass-input w-full text-ink placeholder:text-mute/85 disabled:opacity-50 disabled:pointer-events-none min-h-[96px] resize-y';

export const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ textareaSize = 'md', invalid, className = '', ...props }, ref) => (
    <textarea
      ref={ref}
      className={[
        BASE,
        SIZE[textareaSize],
        invalid && 'border-critical focus:border-critical focus-visible:shadow-[0_0_0_3px_color-mix(in_oklab,var(--critical)_28%,transparent)]',
        className,
      ].join(' ')}
      {...props}
    />
  )
);
Textarea.displayName = 'Textarea';
export default Textarea;
