import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { user } from "../test/fixtures";
import { AppShell } from "./AppShell";

vi.mock("../features/pos/PosView", () => ({
  PosView: ({ onSessionRequired }: { onSessionRequired: () => void }) => <button onClick={onSessionRequired}>POS activa</button>,
}));
vi.mock("../features/products/ProductsView", () => ({ ProductsView: () => <div>Vista productos</div> }));
vi.mock("../features/inventory/InventoryView", () => ({ InventoryView: ({ onNotice }: { onNotice: (notice: { tone: "success"; message: string }) => void }) => <button onClick={() => onNotice({ tone: "success", message: "Movimiento creado." })}>Vista inventario</button> }));
vi.mock("../features/sales/SalesView", () => ({ SalesView: () => <div>Vista ventas</div> }));

describe("AppShell", () => {
  it("navigates between views and delegates logout and session expiry", () => {
    const onLogout = vi.fn();
    const onSessionRequired = vi.fn();
    const onNotice = vi.fn();
    render(<AppShell user={user} sessionId={1} loggingOut={false} onLogout={onLogout} onNotice={onNotice} onSessionRequired={onSessionRequired} />);
    expect(screen.getByRole("button", { name: "Buscar actualizaciones" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Productos" }));
    expect(screen.getByText("Vista productos")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Productos" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Inventario" }));
    fireEvent.click(screen.getByRole("button", { name: "Vista inventario" }));
    expect(onNotice).toHaveBeenCalledWith({ tone: "success", message: "Movimiento creado." });

    fireEvent.click(screen.getByRole("button", { name: "Punto de venta" }));
    fireEvent.click(screen.getByRole("button", { name: "POS activa" }));
    fireEvent.click(screen.getByRole("button", { name: "Cerrar sesion" }));
    expect(onSessionRequired).toHaveBeenCalledOnce();
    expect(onLogout).toHaveBeenCalledOnce();
  });
});
