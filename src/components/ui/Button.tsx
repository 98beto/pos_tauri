import type { ButtonHTMLAttributes } from "react";
import { Spinner } from "./Spinner";

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
  loading?: boolean;
};

const variantClasses = {
  primary: "border-brand bg-brand text-white hover:border-brand-dark hover:bg-brand-dark",
  secondary: "border-line-strong bg-surface text-ink hover:border-[#aeb7c7] hover:bg-surface-soft",
  ghost: "border-transparent bg-transparent text-muted hover:bg-surface-soft hover:text-ink",
  danger: "border-transparent bg-danger-soft text-danger hover:border-danger/20 hover:bg-danger/15",
};

const sizeClasses = {
  sm: "min-h-9 px-3 text-xs",
  md: "min-h-10 px-4 text-sm",
  lg: "min-h-12 px-5 text-sm",
};

export function Button({
  variant = "secondary",
  size = "md",
  loading = false,
  disabled,
  className = "",
  children,
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={`inline-flex items-center justify-center gap-2 rounded-control border font-bold transition duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed disabled:opacity-45 ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
      {...props}
    >
      {loading && <Spinner className="size-3.5" label="" aria-hidden="true" />}
      {children}
    </button>
  );
}
