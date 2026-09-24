import { forwardRef, useId, type InputHTMLAttributes, type ReactNode } from "react";

export type FieldProps = InputHTMLAttributes<HTMLInputElement> & {
  label: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
};

export const Field = forwardRef<HTMLInputElement, FieldProps>(function Field({ label, hint, error, id, className = "", required, ...props }, ref) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const descriptionId = hint || error ? `${inputId}-description` : undefined;

  return (
    <div className="grid gap-2">
      <label htmlFor={inputId} className="text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">
        {label}
        {required && <span className="ml-1 text-danger" aria-hidden="true">*</span>}
      </label>
      <input
        ref={ref}
        id={inputId}
        required={required}
        aria-invalid={error ? true : undefined}
        aria-describedby={descriptionId}
        className={`h-11 w-full rounded-control border bg-surface px-3 text-sm text-ink outline-none transition placeholder:text-muted/70 focus:border-brand focus:ring-3 focus:ring-brand/12 disabled:cursor-not-allowed disabled:bg-surface-soft disabled:text-muted ${error ? "border-danger" : "border-line-strong"} ${className}`}
        {...props}
      />
      {(error || hint) && (
        <p id={descriptionId} role={error ? "alert" : undefined} className={`text-xs leading-5 ${error ? "text-danger" : "text-muted"}`}>
          {error ?? hint}
        </p>
      )}
    </div>
  );
});
