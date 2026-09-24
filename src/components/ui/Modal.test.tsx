import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Drawer } from "./Drawer";
import { Modal } from "./Modal";

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "getClientRects").mockReturnValue({ length: 1 } as DOMRectList);
});

afterEach(() => vi.restoreAllMocks());

describe("Modal", () => {
  it("moves focus in, traps it, closes with Escape, and restores focus", async () => {
    const onClose = vi.fn();
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const { rerender } = render(
      <Modal open onClose={onClose} title="Editor">
        <input aria-label="Primero" data-autofocus />
        <button>Ultimo</button>
      </Modal>,
    );

    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Primero")));
    const last = screen.getByRole("button", { name: "Ultimo" });
    last.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cerrar dialogo" }));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();

    rerender(<Modal open={false} onClose={onClose} title="Editor"><span>contenido</span></Modal>);
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
  });

  it("blocks Escape, backdrop and close button while closeDisabled", () => {
    const onClose = vi.fn();
    render(<Modal open closeDisabled onClose={onClose} title="Guardando"><span>contenido</span></Modal>);
    const dialog = screen.getByRole("dialog");
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.mouseDown(dialog.parentElement!);
    fireEvent.click(screen.getByRole("button", { name: "Cerrar dialogo" }));
    expect(onClose).not.toHaveBeenCalled();
    expect((screen.getByRole("button", { name: "Cerrar dialogo" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("wraps Shift+Tab and isolates the application tree", async () => {
    const events = userEvent.setup();
    const { container } = render(<Modal open onClose={vi.fn()} title="Editor"><button>Accion</button></Modal>);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cerrar dialogo" })));
    expect(container.inert).toBe(true);
    expect(container.getAttribute("aria-hidden")).toBe("true");

    await events.tab({ shift: true });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Accion" }));
  });

  it("keeps only the top stacked modal active and restores the previous layer", async () => {
    const events = userEvent.setup();
    function StackedModals() {
      const [nested, setNested] = useState(false);
      return <Modal open onClose={vi.fn()} title="Primero">
        <button onClick={() => setNested(true)}>Abrir segundo</button>
        <Modal open={nested} onClose={() => setNested(false)} title="Segundo"><button>Accion secundaria</button></Modal>
      </Modal>;
    }
    render(<StackedModals />);
    await events.click(await screen.findByRole("button", { name: "Abrir segundo" }));
    const dialogs = screen.getAllByRole("dialog", { hidden: true });
    const firstDialog = screen.getByRole("dialog", { name: "Primero", hidden: true });
    const secondDialog = screen.getByRole("dialog", { name: "Segundo", hidden: true });
    expect(dialogs).toHaveLength(2);
    expect(firstDialog.hasAttribute("aria-modal")).toBe(false);
    expect(secondDialog.getAttribute("aria-modal")).toBe("true");
    expect(firstDialog.parentElement?.inert).toBe(true);
    expect(firstDialog.parentElement?.getAttribute("aria-hidden")).toBe("true");

    await events.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Segundo" })).toBeNull());
    expect(screen.getByRole("dialog", { name: "Primero" }).getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Abrir segundo" }));
  });
});

describe("Drawer", () => {
  it("closes with Escape and restores fallback focus", async () => {
    const onClose = vi.fn();
    const trigger = document.createElement("button");
    const fallback = document.createElement("main");
    fallback.tabIndex = -1;
    document.body.append(trigger, fallback);
    trigger.focus();
    const { rerender } = render(<Drawer open onClose={onClose} title="Menu" fallbackFocus={() => fallback}><button data-autofocus>Destino</button></Drawer>);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Destino" })));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
    trigger.remove();
    rerender(<Drawer open={false} onClose={onClose} title="Menu" fallbackFocus={() => fallback}><span>menu</span></Drawer>);
    expect(document.activeElement).toBe(fallback);
    fallback.remove();
  });

  it("traps reverse tab navigation and exposes modal semantics", async () => {
    const events = userEvent.setup();
    render(<Drawer open onClose={vi.fn()} title="Menu"><button data-autofocus>Destino</button></Drawer>);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Destino" })));
    const dialog = screen.getByRole("dialog", { name: "Menu" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    await events.tab({ shift: true });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cerrar menu principal" }));
  });
});
