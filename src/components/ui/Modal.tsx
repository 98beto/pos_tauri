import { useEffect, useId, useRef, type MouseEvent, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";

export type ModalProps = {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  size?: "sm" | "md" | "lg";
  closeDisabled?: boolean;
};

const sizeClasses = {
  sm: "max-w-lg",
  md: "max-w-3xl",
  lg: "max-w-5xl",
};

const focusableSelector = [
  "a[href]",
  "area[href]",
  "button",
  "input",
  "select",
  "textarea",
  "iframe",
  "object",
  "embed",
  '[contenteditable="true"]',
  "[tabindex]",
].join(",");

type ModalEntry = {
  backdrop: HTMLDivElement;
  panel: HTMLDivElement;
  onCloseRef: RefObject<() => void>;
  closeDisabledRef: RefObject<boolean>;
  returnFocus: HTMLElement | null;
  fallbackFocus?: () => HTMLElement | null;
  focusFrame: number | null;
};

type IsolatedElementState = {
  inert: boolean;
  ariaHidden: string | null;
};

const modalStack: ModalEntry[] = [];
const isolatedElements = new Map<HTMLElement, IsolatedElementState>();
let bodyHadScrollLock = false;

function getTopModal() {
  return modalStack[modalStack.length - 1];
}

function isFocusable(element: HTMLElement) {
  if (
    element.matches(":disabled") ||
    element.tabIndex < 0 ||
    (element instanceof HTMLInputElement && element.type === "hidden") ||
    element.closest('[hidden], [inert], [aria-hidden="true"]')
  ) {
    return false;
  }

  const style = window.getComputedStyle(element);
  return style.display !== "none" && style.visibility !== "hidden" && style.visibility !== "collapse" && element.getClientRects().length > 0;
}

function canRestoreFocus(element: HTMLElement | null): element is HTMLElement {
  if (!element?.isConnected || element.matches(":disabled") || element.closest('[hidden], [inert], [aria-hidden="true"]')) {
    return false;
  }

  const style = window.getComputedStyle(element);
  return style.display !== "none" && style.visibility !== "hidden" && style.visibility !== "collapse" && element.getClientRects().length > 0;
}

function getFocusableElements(panel: HTMLElement) {
  return Array.from(panel.querySelectorAll<HTMLElement>(focusableSelector)).filter(isFocusable);
}

function focusModal(entry: ModalEntry) {
  const preferred = entry.panel.querySelector<HTMLElement>("[autofocus], [data-autofocus]");
  const initialFocus = preferred && isFocusable(preferred) ? preferred : getFocusableElements(entry.panel)[0] ?? entry.panel;
  initialFocus.focus({ preventScroll: true });
}

function restoreElement(element: HTMLElement) {
  const previous = isolatedElements.get(element);
  if (!previous) return;

  element.inert = previous.inert;
  if (previous.ariaHidden === null) element.removeAttribute("aria-hidden");
  else element.setAttribute("aria-hidden", previous.ariaHidden);
  isolatedElements.delete(element);
}

function syncIsolation() {
  const top = getTopModal();

  for (const entry of modalStack) {
    if (entry === top) entry.panel.setAttribute("aria-modal", "true");
    else entry.panel.removeAttribute("aria-modal");
  }

  if (!top) {
    for (const element of isolatedElements.keys()) restoreElement(element);
    document.body.classList.toggle("modal-scroll-lock", bodyHadScrollLock);
    document.removeEventListener("keydown", handleKeyDown);
    return;
  }

  const bodyChildren = new Set(Array.from(document.body.children).filter((element): element is HTMLElement => element instanceof HTMLElement));
  for (const element of isolatedElements.keys()) {
    if (!bodyChildren.has(element) || element === top.backdrop) restoreElement(element);
  }

  for (const element of bodyChildren) {
    if (element === top.backdrop || isolatedElements.has(element)) continue;
    isolatedElements.set(element, {
      inert: element.inert,
      ariaHidden: element.getAttribute("aria-hidden"),
    });
    element.inert = true;
    element.setAttribute("aria-hidden", "true");
  }
}

function handleKeyDown(event: KeyboardEvent) {
  const top = getTopModal();
  if (!top) return;

  if (event.key === "Escape") {
    event.preventDefault();
    if (!top.closeDisabledRef.current) top.onCloseRef.current();
    return;
  }

  if (event.key !== "Tab") return;
  const focusable = getFocusableElements(top.panel);
  if (focusable.length === 0) {
    event.preventDefault();
    top.panel.focus();
    return;
  }

  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const activeElement = document.activeElement;
  if (!top.panel.contains(activeElement) || activeElement === top.panel) {
    event.preventDefault();
    (event.shiftKey ? last : first).focus();
  } else if (event.shiftKey && activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

function registerModal(entry: ModalEntry) {
  if (modalStack.length === 0) {
    bodyHadScrollLock = document.body.classList.contains("modal-scroll-lock");
    document.body.classList.add("modal-scroll-lock");
    document.addEventListener("keydown", handleKeyDown);
  } else {
    const previousTop = getTopModal();
    if (previousTop && previousTop.focusFrame !== null) {
      window.cancelAnimationFrame(previousTop.focusFrame);
      previousTop.focusFrame = null;
    }
  }

  modalStack.push(entry);
  syncIsolation();
  entry.focusFrame = window.requestAnimationFrame(() => {
    entry.focusFrame = null;
    if (getTopModal() === entry) focusModal(entry);
  });
}

function unregisterModal(entry: ModalEntry) {
  const index = modalStack.indexOf(entry);
  if (index === -1) return;

  const wasTop = index === modalStack.length - 1;
  modalStack.splice(index, 1);
  if (entry.focusFrame !== null) window.cancelAnimationFrame(entry.focusFrame);
  syncIsolation();

  if (!wasTop) return;
  const top = getTopModal();
  if (top) {
    if (entry.returnFocus?.isConnected && top.panel.contains(entry.returnFocus)) {
      entry.returnFocus.focus({ preventScroll: true });
    } else {
      focusModal(top);
    }
  } else {
    const focusTarget = canRestoreFocus(entry.returnFocus) ? entry.returnFocus : entry.fallbackFocus?.() ?? null;
    if (canRestoreFocus(focusTarget)) focusTarget.focus({ preventScroll: true });
  }
}

export function useModalLayer(
  open: boolean,
  onClose: () => void,
  backdropRef: RefObject<HTMLDivElement | null>,
  panelRef: RefObject<HTMLDivElement | null>,
  fallbackFocus?: () => HTMLElement | null,
  closeDisabled = false,
) {
  const onCloseRef = useRef(onClose);
  const closeDisabledRef = useRef(closeDisabled);
  onCloseRef.current = onClose;
  closeDisabledRef.current = closeDisabled;

  useEffect(() => {
    if (!open) return;

    const panel = panelRef.current;
    const backdrop = backdropRef.current;
    if (!panel || !backdrop) return;

    const entry: ModalEntry = {
      backdrop,
      panel,
      onCloseRef,
      closeDisabledRef,
      returnFocus: document.activeElement instanceof HTMLElement ? document.activeElement : null,
      fallbackFocus,
      focusFrame: null,
    };
    registerModal(entry);
    return () => unregisterModal(entry);
  }, [open, backdropRef, panelRef, fallbackFocus]);
}

export function Modal({ open, onClose, title, description, children, footer, size = "md", closeDisabled = false }: ModalProps) {
  const backdropRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const descriptionId = useId();
  useModalLayer(open, onClose, backdropRef, panelRef, undefined, closeDisabled);

  if (!open) return null;

  function handleBackdropClick(event: MouseEvent<HTMLDivElement>) {
    if (!closeDisabled && event.target === event.currentTarget && getTopModal()?.backdrop === event.currentTarget) onClose();
  }

  return createPortal(
    <div
      ref={backdropRef}
      className={`fixed inset-0 z-50 flex items-center justify-center p-4 backdrop-blur-[3px] sm:p-8 ${closeDisabled ? "cursor-wait bg-[#141b30]/50" : "bg-[#141b30]/42"}`}
      onMouseDown={handleBackdropClick}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-labelledby={titleId}
         aria-describedby={description ? descriptionId : undefined}
         aria-busy={closeDisabled || undefined}
        tabIndex={-1}
        className={`flex max-h-[calc(100dvh-2rem)] w-full flex-col overflow-hidden rounded-modal border border-white/70 bg-surface shadow-panel outline-none sm:max-h-[88vh] ${sizeClasses[size]}`}
      >
        <header className="flex items-start justify-between gap-5 border-b border-line px-5 py-4 sm:px-6 sm:py-5">
          <div>
            <h2 id={titleId} className="font-heading text-xl font-bold tracking-[-0.02em] text-ink sm:text-2xl">{title}</h2>
            {description && <p id={descriptionId} className="mt-1 text-xs leading-5 text-muted">{description}</p>}
          </div>
          <button
             type="button"
             onClick={onClose}
             disabled={closeDisabled}
             className="grid size-8 shrink-0 place-items-center rounded-lg border border-line bg-surface-soft text-lg leading-none text-muted transition hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed disabled:opacity-40"
            aria-label="Cerrar dialogo"
          >
            <span aria-hidden="true">×</span>
          </button>
        </header>
        <div className="min-h-0 overflow-y-auto p-5 sm:p-6">{children}</div>
        {footer && <footer className="flex flex-wrap justify-end gap-2 border-t border-line px-5 py-4 sm:px-6">{footer}</footer>}
      </div>
    </div>,
    document.body,
  );
}
