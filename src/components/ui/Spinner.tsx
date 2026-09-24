import type { HTMLAttributes } from "react";

export type SpinnerProps = HTMLAttributes<HTMLSpanElement> & {
  label?: string;
};

export function Spinner({ label = "Cargando", className = "", ...props }: SpinnerProps) {
  return (
    <span
      className={`inline-block size-4 animate-spin rounded-full border-2 border-current border-r-transparent ${className}`}
      role="status"
      aria-label={label}
      {...props}
    />
  );
}
