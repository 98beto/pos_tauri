import { useEffect, useState } from "react";
import { useUpdater } from "../features/updater/UpdaterContext";
import type { NavigationItem } from "./navigation";

type TopbarProps = {
  view: NavigationItem;
  menuOpen: boolean;
  onOpenMenu: () => void;
};

const dateFormatter = new Intl.DateTimeFormat("es", {
  weekday: "short",
  day: "2-digit",
  month: "short",
  year: "numeric",
});

function localDateValue(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function Topbar({ view, menuOpen, onOpenMenu }: TopbarProps) {
  const [date, setDate] = useState(() => new Date());
  const { manualCheck, checking, busy } = useUpdater();

  useEffect(() => {
    let timeout: number;
    function scheduleNextDay() {
      const now = new Date();
      const nextDay = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
      timeout = window.setTimeout(() => {
        setDate(new Date());
        scheduleNextDay();
      }, nextDay.getTime() - now.getTime());
    }
    scheduleNextDay();
    return () => window.clearTimeout(timeout);
  }, []);

  return (
    <header className="flex min-h-[88px] items-center justify-between gap-4 border-b border-line bg-canvas/95 px-4 py-4 sm:px-6 min-[900px]:px-8">
      <div className="flex min-w-0 items-center gap-3">
        <button
          type="button"
          data-menu-trigger
          className="grid size-10 shrink-0 place-items-center rounded-control border border-line bg-surface text-muted hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand min-[900px]:hidden"
          aria-label="Abrir menu principal"
          aria-controls="main-navigation"
          aria-expanded={menuOpen}
          onClick={onOpenMenu}
        >
          <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path strokeLinecap="round" d="M4 7h16M4 12h16M4 17h16" /></svg>
        </button>
        <div className="min-w-0">
          <h1 className="truncate font-heading text-xl font-bold tracking-[-0.02em] text-ink sm:text-2xl">{view.title}</h1>
          <p className="mt-1 hidden truncate text-xs text-muted sm:block">{view.description}</p>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2 text-[0.6875rem] font-semibold text-muted">
        <button type="button" onClick={manualCheck} disabled={checking || busy} className="inline-flex min-h-9 items-center gap-2 rounded-control border border-line bg-surface/70 px-3 font-bold text-muted transition hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed disabled:opacity-45" aria-label={checking ? "Buscando actualizaciones" : "Buscar actualizaciones"}>
          <svg viewBox="0 0 24 24" className={`size-4 ${checking ? "animate-spin" : ""}`} fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path strokeLinecap="round" strokeLinejoin="round" d="M20 11a8 8 0 1 0-2.34 5.66M20 4v7h-7" /></svg>
          <span className="hidden lg:inline">{checking ? "Buscando…" : "Buscar actualizaciones"}</span>
        </button>
        <span className="hidden items-center rounded-control border border-line bg-surface/70 px-3 py-2 sm:inline-flex">
          <span className="mr-2 size-2 rounded-full bg-[#3b9b6d]" aria-hidden="true" />
          Sistema listo
        </span>
        <time className="hidden rounded-control border border-line bg-surface/70 px-3 py-2 md:block" dateTime={localDateValue(date)}>
          {dateFormatter.format(date)}
        </time>
      </div>
    </header>
  );
}
