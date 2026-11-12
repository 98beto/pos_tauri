import type { ReactNode } from "react";

export type MovementTone = "positive" | "negative" | "neutral";

export const movementAppearance: Record<MovementTone, { card: string; badge: string }> = {
  positive: { card: "border-l-success bg-success-soft", badge: "border-success/30 bg-surface text-success" },
  negative: { card: "border-l-danger bg-danger-soft", badge: "border-danger/30 bg-surface text-danger" },
  neutral: { card: "border-l-info bg-info-soft", badge: "border-info/30 bg-surface text-ink" },
};

export function MovementIndicator({ tone, children }: { tone: MovementTone; children: ReactNode }) {
  return <span className={`inline-flex max-w-full items-center gap-1.5 rounded-full border px-2 py-1 text-xs font-bold ${movementAppearance[tone].badge}`}>
    <svg aria-hidden="true" className="size-3 shrink-0" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      {tone === "positive" ? <path d="M8 13V3M4 7l4-4 4 4" /> : tone === "negative" ? <path d="M8 3v10M4 9l4 4 4-4" /> : <path d="M3 8h10M9 4l4 4-4 4" />}
    </svg>
    <span>{children}</span>
  </span>;
}
