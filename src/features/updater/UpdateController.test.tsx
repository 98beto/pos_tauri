import { StrictMode, useState } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { resetUpdaterSessionForTests } from "../../api/updater";
import { getCart, incrementCartItem } from "../../api/cart";
import { listCategories } from "../../api/categories";
import { cart, deferred, emptyCart, user } from "../../test/fixtures";
import { PosView } from "../pos/PosView";
import { UpdateController } from "./UpdateController";
import { useUpdater } from "./UpdaterContext";

vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("../../api/cart", () => ({
  addCartItem: vi.fn(),
  checkoutCart: vi.fn(),
  clearCart: vi.fn(),
  decrementCartItem: vi.fn(),
  getCart: vi.fn(),
  incrementCartItem: vi.fn(),
  quoteCashChange: vi.fn(),
  removeCartItem: vi.fn(),
}));
vi.mock("../../api/categories", () => ({ listCategories: vi.fn() }));
vi.mock("../../api/products", () => ({ listProductSelector: vi.fn() }));

const mockedCheck = vi.mocked(check);
const mockedRelaunch = vi.mocked(relaunch);
const mockedGetCart = vi.mocked(getCart);
const mockedIncrementCartItem = vi.mocked(incrementCartItem);
let checkoutRequest: ReturnType<typeof deferred<void>>;

function createUpdate(overrides: Record<string, unknown> = {}) {
  return {
    currentVersion: "0.1.0",
    version: "0.2.0",
    body: "Mejoras de inventario\nCorrecciones de seguridad",
    downloadAndInstall: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

function Harness() {
  const { manualCheck, beginCartReporting, beginCartOperation, reportCart, runCheckout } = useUpdater();
  const [posVisible, setPosVisible] = useState(true);
  const [cartGeneration] = useState(() => beginCartReporting(7));
  return <div>
    <button onClick={manualCheck}>manual-test</button>
    <button onClick={() => setPosVisible((visible) => !visible)}>navigate-test</button>
    {posVisible && <>
      <button onClick={() => {
        const version = { generation: cartGeneration, operation: 1 };
        beginCartOperation(7, version);
        reportCart(7, version, true);
      }}>cart-test</button>
      <button onClick={() => void runCheckout(7, () => checkoutRequest.promise)}>checkout-test</button>
    </>}
  </div>;
}

function controllerTree(sessionId: number | null = 7) {
  return <UpdateController sessionId={sessionId ?? undefined}><Harness /></UpdateController>;
}

function PosNavigationHarness() {
  const [instance, setInstance] = useState(1);
  const [visible, setVisible] = useState(true);
  return <div>
    <button onClick={() => setInstance((value) => value + 1)}>second-pos</button>
    <button onClick={() => setVisible(false)}>leave-pos</button>
    {visible && <PosView key={instance} user={user} sessionId={7} onNotice={vi.fn()} onSessionRequired={vi.fn()} />}
  </div>;
}

function integratedPosTree() {
  return <UpdateController sessionId={7}><PosNavigationHarness /></UpdateController>;
}

function renderController(strict = false) {
  const tree = controllerTree();
  return render(strict ? <StrictMode>{tree}</StrictMode> : tree);
}

beforeEach(() => {
  resetUpdaterSessionForTests();
  checkoutRequest = deferred<void>();
  mockedCheck.mockReset();
  mockedRelaunch.mockReset().mockResolvedValue(undefined);
  mockedGetCart.mockReset().mockResolvedValue(cart);
  mockedIncrementCartItem.mockReset().mockResolvedValue(cart);
  vi.mocked(listCategories).mockReset().mockResolvedValue([]);
});

describe("UpdateController", () => {
  it("performs exactly one silent startup check under StrictMode", async () => {
    mockedCheck.mockResolvedValue(null);
    renderController(true);
    await act(async () => undefined);
    expect(mockedCheck).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows no UI when the application is current or the startup network check fails", async () => {
    mockedCheck.mockResolvedValueOnce(null);
    const first = renderController();
    await act(async () => undefined);
    expect(screen.queryByRole("dialog")).toBeNull();
    first.unmount();

    resetUpdaterSessionForTests();
    mockedCheck.mockRejectedValueOnce(new Error("network socket host.internal"));
    renderController();
    await act(async () => undefined);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByText(/host\.internal/)).toBeNull();
  });

  it.each([
    ["integrity", "invalid signature from /home/albert/private-key.pem", "firma o integridad"],
    ["configuration", "updater configuration contains secret-endpoint", "configuración de actualizaciones"],
  ])("shows a sanitized startup %s failure", async (_kind, detail, expected) => {
    mockedCheck.mockRejectedValueOnce(new Error(detail));
    renderController();
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain(expected);
    expect(alert.textContent).not.toContain(detail);
    expect(screen.getByRole("dialog", { name: "No se pudo actualizar" })).toBeTruthy();
  });

  it("keeps an unknown startup failure silent because it may be transient", async () => {
    mockedCheck.mockRejectedValueOnce(new Error("unexpected internal failure secret-value"));
    renderController();
    await act(async () => undefined);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByText(/secret-value/)).toBeNull();
  });

  it("presents versions, notes and actions without downloading before confirmation", async () => {
    const update = createUpdate();
    mockedCheck.mockResolvedValue(update as never);
    renderController();
    const dialog = await screen.findByRole("dialog", { name: "Actualización disponible" });
    expect(dialog.textContent).toContain("0.1.0");
    expect(dialog.textContent).toContain("0.2.0");
    expect(dialog.textContent).toContain("Mejoras de inventario");
    expect(screen.getByRole("button", { name: "Actualizar ahora" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Después" })).toBeTruthy();
    expect(update.downloadAndInstall).not.toHaveBeenCalled();
  });

  it("retains and closes an available update exactly once under StrictMode", async () => {
    const update = createUpdate();
    mockedCheck.mockResolvedValue(update as never);
    const view = renderController(true);
    await screen.findByRole("dialog", { name: "Actualización disponible" });
    expect(update.close).not.toHaveBeenCalled();
    view.unmount();
    expect(update.close).toHaveBeenCalledOnce();
  });

  it("closes a late update result after unmount exactly once", async () => {
    const request = deferred<ReturnType<typeof createUpdate>>();
    const update = createUpdate();
    mockedCheck.mockReturnValue(request.promise as never);
    const view = renderController();
    view.unmount();
    await act(async () => request.resolve(update));
    expect(update.close).toHaveBeenCalledOnce();
  });

  it("postpones for the session and a manual check recovers the flow", async () => {
    const first = createUpdate();
    const second = createUpdate({ version: "0.2.1" });
    mockedCheck.mockResolvedValueOnce(first as never).mockResolvedValueOnce(second as never);
    renderController();
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Después" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(first.close).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "manual-test" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("0.2.1");
    expect(mockedCheck).toHaveBeenCalledTimes(2);
  });

  it("reports a current application and a recoverable network error on manual checks", async () => {
    mockedCheck.mockResolvedValueOnce(null).mockResolvedValueOnce(null).mockRejectedValueOnce(new Error("network timeout secret-host"));
    renderController();
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: "manual-test" }));
    expect(await screen.findByText("Línea POS ya está actualizada.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "manual-test" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Revisa tu conexión");
    expect(alert.textContent).not.toContain("secret-host");
  });

  it("renders known download progress from Started, Progress and Finished", async () => {
    const install = deferred<void>();
    const update = createUpdate({
      downloadAndInstall: vi.fn((onEvent: (event: unknown) => void) => {
        onEvent({ event: "Started", data: { contentLength: 1000 } });
        onEvent({ event: "Progress", data: { chunkLength: 250 } });
        return install.promise;
      }),
    });
    mockedCheck.mockResolvedValue(update as never);
    renderController();
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect(await screen.findByText("25% · 250 B de 1000 B")).toBeTruthy();
    await act(async () => install.resolve());
  });

  it("renders indeterminate progress when ContentLength is unknown", async () => {
    const install = deferred<void>();
    const update = createUpdate({
      downloadAndInstall: vi.fn((onEvent: (event: unknown) => void) => {
        onEvent({ event: "Started", data: {} });
        onEvent({ event: "Progress", data: { chunkLength: 2048 } });
        return install.promise;
      }),
    });
    mockedCheck.mockResolvedValue(update as never);
    renderController();
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect(await screen.findByText("Descargando… 2.0 KB")).toBeTruthy();
    expect(screen.queryByRole("progressbar")).toBeNull();
    await act(async () => install.resolve());
  });

  it("keeps focus in the dialog while downloading, installing and becoming ready", async () => {
    const events = userEvent.setup();
    const install = deferred<void>();
    let progress: ((event: unknown) => void) | undefined;
    const update = createUpdate({ downloadAndInstall: vi.fn((onEvent: (event: unknown) => void) => { progress = onEvent; return install.promise; }) });
    mockedCheck.mockResolvedValue(update as never);
    renderController();

    await events.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    await screen.findByText("Descargando… 0 B");
    let dialog = screen.getByRole("dialog");
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));

    act(() => progress?.({ event: "Finished" }));
    screen.getByText("Verificando e instalando…");
    dialog = screen.getByRole("dialog");
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));

    await act(async () => install.resolve());
    const ready = await screen.findByRole("status");
    expect(ready.getAttribute("aria-live")).toBe("polite");
    dialog = screen.getByRole("dialog");
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
  });

  it("keeps focus on the sanitized error when installation fails", async () => {
    const events = userEvent.setup();
    const install = deferred<void>();
    const update = createUpdate({ downloadAndInstall: vi.fn(() => install.promise) });
    mockedCheck.mockResolvedValue(update as never);
    renderController();

    await events.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    await screen.findByText("Descargando… 0 B");
    await act(async () => install.reject(new Error("invalid signature /private/key.pem")));
    const alert = await screen.findByRole("alert");
    const dialog = screen.getByRole("dialog");
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    expect(alert.textContent).not.toContain("key.pem");
  });

  it("cannot be closed while downloading or installing", async () => {
    const install = deferred<void>();
    let progress: ((event: unknown) => void) | undefined;
    const update = createUpdate({ downloadAndInstall: vi.fn((onEvent: (event: unknown) => void) => { progress = onEvent; return install.promise; }) });
    mockedCheck.mockResolvedValue(update as never);
    renderController();
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    const close = screen.getByRole("button", { name: "Cerrar dialogo" }) as HTMLButtonElement;
    expect(close.disabled).toBe(true);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeTruthy();
    act(() => progress?.({ event: "Finished" }));
    expect(screen.getByRole("dialog", { name: "Instalando actualización" })).toBeTruthy();
    expect(screen.getByText(/En Windows, el instalador puede cerrar/)).toBeTruthy();
    expect(close.disabled).toBe(true);
    await act(async () => install.resolve());
  });

  it("distinguishes integrity, network and interrupted download errors without leaking details", async () => {
    const signature = createUpdate({ downloadAndInstall: vi.fn().mockRejectedValue(new Error("invalid signature /home/albert/key.pem")) });
    mockedCheck.mockResolvedValueOnce(signature as never);
    renderController();
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    const signatureAlert = await screen.findByRole("alert");
    expect(signatureAlert.textContent).toContain("firma o integridad");
    expect(signatureAlert.textContent).not.toContain("key.pem");

    const network = createUpdate({ downloadAndInstall: vi.fn().mockRejectedValue(new Error("connection reset at secret-host while writing /tmp/pkg")) });
    mockedCheck.mockResolvedValueOnce(network as never);
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    const networkAlert = await screen.findByRole("alert");
    expect(networkAlert.textContent).toContain("Revisa tu conexión");
    expect(networkAlert.textContent).not.toContain("secret-host");

    const interrupted = createUpdate({ downloadAndInstall: vi.fn().mockRejectedValue(new Error("unexpected end of stream /tmp/pkg")) });
    mockedCheck.mockResolvedValueOnce(interrupted as never);
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    const downloadAlert = await screen.findByRole("alert");
    expect(downloadAlert.textContent).toContain("descarga se interrumpió");
    expect(downloadAlert.textContent).not.toContain("/tmp/pkg");
  });

  it("reports an installation failure after Finished without leaking details", async () => {
    const update = createUpdate({
      downloadAndInstall: vi.fn((onEvent: (event: unknown) => void) => {
        onEvent({ event: "Finished" });
        return Promise.reject(new Error("installer failed at /tmp/private-package"));
      }),
    });
    mockedCheck.mockResolvedValue(update as never);
    renderController();
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("descarga terminó, pero no se pudo instalar");
    expect(alert.textContent).not.toContain("private-package");
  });

  it("reports a network failure after Finished as network while preserving integrity priority", async () => {
    const network = createUpdate({
      downloadAndInstall: vi.fn((onEvent: (event: unknown) => void) => {
        onEvent({ event: "Finished" });
        return Promise.reject(new Error("network connection reset during install"));
      }),
    });
    mockedCheck.mockResolvedValueOnce(network as never);
    renderController();
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Revisa tu conexión");

    const integrity = createUpdate({
      downloadAndInstall: vi.fn((onEvent: (event: unknown) => void) => {
        onEvent({ event: "Finished" });
        return Promise.reject(new Error("network checksum verification failed"));
      }),
    });
    mockedCheck.mockResolvedValueOnce(integrity as never);
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect((await screen.findByRole("alert")).textContent).toContain("firma o integridad");
  });

  it("recovers manually after a network error", async () => {
    const failed = createUpdate({ downloadAndInstall: vi.fn().mockRejectedValue(new Error("network offline")) });
    const recovered = createUpdate();
    mockedCheck.mockResolvedValueOnce(failed as never).mockResolvedValueOnce(recovered as never);
    renderController();
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Revisa tu conexión");
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect(await screen.findByRole("dialog", { name: "Actualización instalada" })).toBeTruthy();
    expect(failed.close).toHaveBeenCalledOnce();
    expect(recovered.downloadAndInstall).toHaveBeenCalledOnce();
  });

  it("does not relaunch before completion and only relaunches after explicit confirmation", async () => {
    const update = createUpdate();
    mockedCheck.mockResolvedValue(update as never);
    renderController();
    expect(mockedRelaunch).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect(await screen.findByRole("dialog", { name: "Actualización instalada" })).toBeTruthy();
    expect(mockedRelaunch).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Reiniciar y completar actualización" }));
    expect(mockedRelaunch).toHaveBeenCalledOnce();
  });

  it("warns about session, cart loss and possible Windows closure", async () => {
    mockedCheck.mockResolvedValue(createUpdate() as never);
    renderController();
    fireEvent.click(screen.getByRole("button", { name: "cart-test" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("iniciar sesión nuevamente");
    expect(dialog.textContent).toContain("carrito no cobrado");
    fireEvent.click(screen.getByRole("button", { name: "Actualizar ahora" }));
    expect(await screen.findByRole("dialog", { name: "Actualización instalada" })).toBeTruthy();
  });

  it("blocks installation while checkout is pending and gives feedback", async () => {
    const update = createUpdate();
    mockedCheck.mockResolvedValue(update as never);
    renderController();
    fireEvent.click(screen.getByRole("button", { name: "checkout-test" }));
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect(update.downloadAndInstall).not.toHaveBeenCalled();
    expect(await screen.findByText(/cobro pendiente o en curso/)).toBeTruthy();
    expect(screen.getByRole("dialog", { name: "Actualización disponible" })).toBeTruthy();
    await act(async () => checkoutRequest.resolve());
  });

  it("keeps a live checkout blocked across navigation and logout, then releases it", async () => {
    const update = createUpdate();
    mockedCheck.mockResolvedValue(update as never);
    const view = render(controllerTree());
    fireEvent.click(screen.getByRole("button", { name: "checkout-test" }));
    fireEvent.click(screen.getByRole("button", { name: "navigate-test", hidden: true }));

    view.rerender(controllerTree(null));
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect(update.downloadAndInstall).not.toHaveBeenCalled();

    await act(async () => checkoutRequest.resolve());
    fireEvent.click(screen.getByRole("button", { name: "Actualizar ahora" }));
    expect(await screen.findByRole("dialog", { name: "Actualización instalada" })).toBeTruthy();
  });

  it("retains cart ownership on navigation and resets it on logout or a new session", async () => {
    mockedCheck.mockResolvedValue(createUpdate() as never);
    const view = render(controllerTree());
    fireEvent.click(screen.getByRole("button", { name: "cart-test" }));
    expect((await screen.findByRole("dialog")).textContent).toContain("carrito no cobrado");

    fireEvent.click(screen.getByRole("button", { name: "navigate-test", hidden: true }));
    expect(screen.getByRole("dialog").textContent).toContain("carrito no cobrado");
    await act(async () => view.rerender(controllerTree(null)));
    expect(screen.getByRole("dialog").textContent).not.toContain("carrito no cobrado");
    await act(async () => view.rerender(controllerTree(8)));
    expect(screen.getByRole("dialog").textContent).not.toContain("carrito no cobrado");
  });

  it("rejects an older PosView cart result when a second instance resolves first", async () => {
    const first = deferred<typeof emptyCart>();
    const second = deferred<typeof cart>();
    mockedGetCart.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    mockedCheck.mockResolvedValue(createUpdate() as never);
    render(integratedPosTree());
    await act(async () => undefined);

    fireEvent.click(screen.getByRole("button", { name: "second-pos", hidden: true }));
    await act(async () => second.resolve(cart));
    expect((await screen.findByRole("dialog", { name: "Actualización disponible" })).textContent).toContain("carrito no cobrado");

    await act(async () => first.resolve(emptyCart));
    expect(screen.getByRole("dialog").textContent).toContain("carrito no cobrado");
  });

  it("keeps updater blocked by a cart mutation after leaving POS and releases its exact token", async () => {
    const mutation = deferred<typeof cart>();
    mockedIncrementCartItem.mockReturnValueOnce(mutation.promise);
    mockedCheck.mockResolvedValue(createUpdate() as never);
    render(integratedPosTree());
    await screen.findByRole("heading", { name: "Carrito actual" });

    fireEvent.click(screen.getByRole("button", { name: "Aumentar Cafe molido", hidden: true }));
    fireEvent.click(screen.getByRole("button", { name: "leave-pos", hidden: true }));
    fireEvent.click(await screen.findByRole("button", { name: "Actualizar ahora" }));
    expect(screen.getByText(/cambio pendiente del carrito/)).toBeTruthy();

    await act(async () => mutation.resolve(cart));
    fireEvent.click(screen.getByRole("button", { name: "Actualizar ahora" }));
    expect(await screen.findByRole("dialog", { name: "Actualización instalada" })).toBeTruthy();
  });

  it("is safe against double clicks on download and relaunch", async () => {
    const install = deferred<void>();
    const update = createUpdate({ downloadAndInstall: vi.fn(() => install.promise) });
    mockedCheck.mockResolvedValue(update as never);
    renderController();
    const button = await screen.findByRole("button", { name: "Actualizar ahora" });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(update.downloadAndInstall).toHaveBeenCalledOnce();
    expect(update.downloadAndInstall).toHaveBeenCalledWith(expect.any(Function), { restartAfterInstall: false });
    await act(async () => install.resolve());
    const restart = screen.getByRole("button", { name: "Reiniciar y completar actualización" });
    fireEvent.click(restart);
    fireEvent.click(restart);
    expect(mockedRelaunch).toHaveBeenCalledOnce();
  });
});
