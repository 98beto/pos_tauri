import { useEffect } from "react";
import type { User } from "../types/user";
import { Brand, Button, Drawer } from "../components/ui";
import { navigation, type ViewId } from "./navigation";

type SidebarProps = {
  user: User;
  activeView: ViewId;
  mobileOpen: boolean;
  loggingOut: boolean;
  onNavigate: (view: ViewId) => void;
  onClose: () => void;
  onLogout: () => void;
};

function initials(user: User) {
  return `${user.first_name.charAt(0)}${user.last_name.charAt(0)}`.toUpperCase();
}

function mainContent() {
  return document.getElementById("main-content");
}

export function Sidebar({ user, activeView, mobileOpen, loggingOut, onNavigate, onClose, onLogout }: SidebarProps) {
  useEffect(() => {
    if (!mobileOpen) return;
    const desktop = window.matchMedia("(min-width: 900px)");
    function closeOnDesktop(event: MediaQueryListEvent) {
      if (event.matches) onClose();
    }
    desktop.addEventListener("change", closeOnDesktop);
    return () => desktop.removeEventListener("change", closeOnDesktop);
  }, [mobileOpen, onClose]);

  function content(autofocus: boolean) {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-4 py-6">
        <Brand className="px-2 pb-8" />
        <p className="px-3 pb-2 text-[0.625rem] font-extrabold tracking-[0.14em] text-muted/80 uppercase">Principal</p>
        <nav className="grid gap-1" aria-label="Secciones">
          {navigation.map((item, index) => (
            <button
              data-autofocus={autofocus && index === 0 ? "true" : undefined}
              key={item.id}
              type="button"
              aria-current={activeView === item.id ? "page" : undefined}
              onClick={() => onNavigate(item.id)}
              className={`flex min-h-11 items-center gap-3 rounded-[0.6875rem] px-3 text-left text-sm font-semibold transition focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand ${activeView === item.id ? "bg-brand-soft text-brand" : "text-muted hover:bg-surface-soft hover:text-ink"}`}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </nav>

        <div className="mt-auto border-t border-line pt-4">
          <div className="flex min-w-0 items-center gap-3 px-2">
            <span className="grid size-9 shrink-0 place-items-center rounded-full bg-brand-soft text-xs font-extrabold text-brand-dark" aria-hidden="true">
              {initials(user)}
            </span>
            <span className="min-w-0">
              <strong className="block truncate text-xs text-ink">{user.first_name} {user.last_name}</strong>
              <span className="mt-0.5 block truncate text-[0.625rem] text-muted">{user.email}</span>
            </span>
          </div>
          <Button variant="ghost" size="sm" className="mt-3 w-full justify-start" loading={loggingOut} onClick={onLogout}>
            <svg viewBox="0 0 24 24" className="size-4" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><path strokeLinecap="round" strokeLinejoin="round" d="M10 5H5v14h5M14 8l4 4-4 4M8 12h10" /></svg>
            Cerrar sesion
          </Button>
        </div>
      </div>
    );
  }

  return (
    <>
      <aside aria-label="Navegacion principal" className="sticky top-0 hidden h-dvh w-[244px] flex-col border-r border-line bg-surface min-[900px]:flex">
        {content(false)}
      </aside>
      <Drawer
        open={mobileOpen}
        onClose={onClose}
        title="Navegacion principal"
        id="main-navigation"
        fallbackFocus={mainContent}
      >
        {content(true)}
      </Drawer>
    </>
  );
}
