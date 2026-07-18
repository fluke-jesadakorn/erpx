import React from 'react';
import { Label } from './Label';

export interface FormFieldProps {
  label?: React.ReactNode;
  htmlFor?: string;
  required?: boolean;
  error?: React.ReactNode;
  hint?: React.ReactNode;
  className?: string;
  children: React.ReactNode;
}

export function FormField({
  label,
  htmlFor,
  required,
  error,
  hint,
  className = '',
  children,
}: FormFieldProps) {
  return (
    <div className={className}>
      {label && <Label htmlFor={htmlFor} required={required}>{label}</Label>}
      {children}
      {error && (
        <p className="text-xs text-critical mt-1.5" role="alert">{error}</p>
      )}
      {!error && hint && (
        <p className="text-xs text-mute mt-1.5">{hint}</p>
      )}
    </div>
  );
}

export default FormField;
