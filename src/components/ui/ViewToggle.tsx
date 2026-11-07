import type { CollectionView } from "../../hooks/useViewPreference";

export function ViewToggle({ value, onChange, label }: { value: CollectionView; onChange: (value: CollectionView) => void; label: string }) {
  return <div className="inline-flex rounded-control border border-line-strong bg-surface-soft p-1" role="group" aria-label={`Vista de ${label}`}>
    {([ ["list", "Lista"], ["cards", "Tarjetas"] ] as const).map(([mode, text]) => <button key={mode} type="button" aria-pressed={value === mode} onClick={() => onChange(mode)} className={`rounded-[calc(var(--radius-control)-0.25rem)] px-3 py-2 text-sm font-bold outline-none focus-visible:ring-3 focus-visible:ring-brand/20 ${value === mode ? "bg-surface text-brand shadow-sm" : "text-muted hover:text-ink"}`}>{text}</button>)}
  </div>;
}
