import type { HTMLAttributes, ReactNode } from "react";

export type StatCardProps = HTMLAttributes<HTMLElement> & {
  label: ReactNode;
  value: ReactNode;
  detail?: ReactNode;
};

export function StatCard({ label, value, detail, className = "", ...props }: StatCardProps) {
  return (
    <article className={`rounded-[0.8125rem] border border-line bg-surface p-4 sm:p-5 ${className}`} {...props}>
      <span className="block text-[0.625rem] font-bold tracking-[0.05em] text-muted uppercase">{label}</span>
      <strong className="mt-2 block font-heading text-2xl leading-none font-bold text-ink sm:text-[1.75rem]">{value}</strong>
      {detail && <span className="mt-2 block text-xs text-muted">{detail}</span>}
    </article>
  );
}
