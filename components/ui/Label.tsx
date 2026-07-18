import React from 'react';

export interface LabelProps extends React.LabelHTMLAttributes<HTMLLabelElement> {
  required?: boolean;
}

export const Label = React.forwardRef<HTMLLabelElement, LabelProps>(
  ({ required, className = '', children, ...props }, ref) => (
    <label
      ref={ref}
      className={['block text-sm font-medium text-ink-2 mb-1.5', className].join(' ')}
      {...props}
    >
      {children}
      {required && <span className="text-critical ml-1" aria-hidden>*</span>}
    </label>
  )
);
Label.displayName = 'Label';
export default Label;
