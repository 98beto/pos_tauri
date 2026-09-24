import type { HTMLAttributes } from "react";

export type BrandProps = HTMLAttributes<HTMLDivElement> & {
  compact?: boolean;
};

export function Brand({ compact = false, className = "", ...props }: BrandProps) {
  return (
    <div className={`flex items-center gap-3 ${className}`} {...props}>
      <span
        className="grid size-11 shrink-0 place-items-center rounded-xl bg-brand text-white shadow-sm"
        aria-hidden="true"
      >
        <svg viewBox="0 0 24 24" className="size-6" fill="none" stroke="currentColor" strokeWidth="1.8">
          <path strokeLinecap="round" strokeLinejoin="round" d="M3.5 4.5h2l1.7 9.1a2 2 0 0 0 2 1.6h7.6a2 2 0 0 0 1.9-1.5L20 8H6.3M9 19a.5.5 0 1 0 0-1 .5.5 0 0 0 0 1Zm8 0a.5.5 0 1 0 0-1 .5.5 0 0 0 0 1Z" />
        </svg>
      </span>
      {!compact ? (
        <span>
          <span className="block font-heading text-2xl leading-none font-bold tracking-[-0.02em] text-ink">
            Linea <span className="text-brand">POS</span>
          </span>
          <span className="mt-1 block text-[0.625rem] leading-none font-extrabold tracking-[0.16em] text-muted uppercase">
            Punto de venta
          </span>
        </span>
      ) : (
        <span className="sr-only">Linea POS</span>
      )}
    </div>
  );
}
