import { useId, useRef, type MouseEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useModalLayer } from "./Modal";

export type DrawerProps = {
  open: boolean;
  onClose: () => void;
  title: string;
  id?: string;
  children: ReactNode;
  fallbackFocus?: () => HTMLElement | null;
};

export function Drawer({ open, onClose, title, id, children, fallbackFocus }: DrawerProps) {
  const backdropRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  useModalLayer(open, onClose, backdropRef, panelRef, fallbackFocus);

  if (!open) return null;

  function handleBackdropClick(event: MouseEvent<HTMLDivElement>) {
    if (event.target === event.currentTarget) onClose();
  }

  return createPortal(
    <div ref={backdropRef} className="fixed inset-0 z-50 bg-ink/35 min-[900px]:hidden" onMouseDown={handleBackdropClick}>
      <div
        ref={panelRef}
        id={id}
        role="dialog"
        aria-labelledby={titleId}
        tabIndex={-1}
        className="flex h-dvh w-[min(19rem,85vw)] flex-col overflow-hidden border-r border-line bg-surface shadow-panel outline-none"
      >
        <h2 id={titleId} className="sr-only">{title}</h2>
        <button
          type="button"
          onClick={onClose}
          className="absolute top-4 left-[min(calc(85vw-3rem),16rem)] grid size-10 place-items-center rounded-full bg-surface text-xl text-muted shadow-panel focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
          aria-label="Cerrar menu principal"
        >
          <span aria-hidden="true">×</span>
        </button>
        {children}
      </div>
    </div>,
    document.body,
  );
}
