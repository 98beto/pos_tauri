import type { HTMLAttributes, ReactNode } from "react";

export type EmptyStateProps = HTMLAttributes<HTMLDivElement> & {
  title: ReactNode;
  description?: ReactNode;
  icon?: ReactNode;
  action?: ReactNode;
};

export function EmptyState({ title, description, icon = "+", action, className = "", ...props }: EmptyStateProps) {
  return (
    <div className={`grid min-h-64 place-items-center p-8 text-center ${className}`} {...props}>
      <div className="max-w-sm">
        <span className="mx-auto grid size-14 place-items-center rounded-[1.0625rem] border border-dashed border-line-strong bg-surface-soft font-heading text-2xl text-brand" aria-hidden="true">
          {icon}
        </span>
        <h2 className="mt-4 text-sm font-bold text-ink">{title}</h2>
        {description && <p className="mt-2 text-xs leading-5 text-muted">{description}</p>}
        {action && <div className="mt-4">{action}</div>}
      </div>
    </div>
  );
}
