import { useState } from "react";
import type { User } from "../types/user";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { navigation, type ViewId } from "./navigation";
import { PosView } from "../features/pos/PosView";
import { ProductsView } from "../features/products/ProductsView";
import { InventoryView } from "../features/inventory/InventoryView";
import { SalesView } from "../features/sales/SalesView";

type Notice = { tone: "success" | "error" | "info"; message: string };

type AppShellProps = {
  user: User;
  sessionId: number;
  loggingOut: boolean;
  onLogout: () => void;
  onNotice: (notice: Notice) => void;
  onSessionRequired: () => void;
};

export function AppShell({ user, sessionId, loggingOut, onLogout, onNotice, onSessionRequired }: AppShellProps) {
  const [activeView, setActiveView] = useState<ViewId>("pos");
  const [historyActivation, setHistoryActivation] = useState(0);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const view = navigation.find((item) => item.id === activeView) ?? navigation[0];

  function navigate(nextView: ViewId) {
    if (nextView === "inventory" || nextView === "sales") setHistoryActivation((activation) => activation + 1);
    setActiveView(nextView);
    setMobileMenuOpen(false);
    window.requestAnimationFrame(() => document.getElementById("main-content")?.focus());
  }

  function closeMobileMenu() {
    setMobileMenuOpen(false);
  }

  return (
    <div className="min-h-dvh bg-canvas min-[900px]:grid min-[900px]:grid-cols-[244px_minmax(0,1fr)]">
      <Sidebar
        user={user}
        activeView={activeView}
        mobileOpen={mobileMenuOpen}
        loggingOut={loggingOut}
        onNavigate={navigate}
        onClose={closeMobileMenu}
        onLogout={onLogout}
      />
      <div className="min-w-0">
        <Topbar view={view} menuOpen={mobileMenuOpen} onOpenMenu={() => setMobileMenuOpen(true)} />
        {activeView === "pos" && <PosView user={user} sessionId={sessionId} onNotice={onNotice} onSessionRequired={onSessionRequired} />}
        {activeView === "products" && <ProductsView onNotice={onNotice} onSessionRequired={onSessionRequired} />}
        {activeView === "inventory" && <InventoryView key={`inventory-${historyActivation}`} user={user} onNotice={onNotice} onSessionRequired={onSessionRequired} />}
        {activeView === "sales" && <SalesView key={`sales-${historyActivation}`} onSessionRequired={onSessionRequired} />}
      </div>
    </div>
  );
}
