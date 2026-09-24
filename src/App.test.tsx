import { StrictMode } from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { currentUser, logout, ownerExists } from "./api/auth";
import { deferred, user } from "./test/fixtures";
import type { User } from "./types/user";

const authHarness = vi.hoisted(() => ({ authenticate: undefined as ((authenticated: User) => void) | undefined }));

vi.mock("./api/auth", () => ({
  currentUser: vi.fn(),
  logout: vi.fn(),
  ownerExists: vi.fn(),
}));

vi.mock("./features/auth/AuthScreen", () => ({
  AuthScreen: ({ mode, confirmation, onAuthenticated, onOwnerInitialized, onOwnerAlreadyConfigured }: { mode: string; confirmation?: string; onAuthenticated: (authenticated: typeof user) => void; onOwnerInitialized: () => void; onOwnerAlreadyConfigured: () => void }) => {
    authHarness.authenticate = onAuthenticated;
    return <div>
      auth:{mode}
      {confirmation && <span>{confirmation}</span>}
      <button onClick={() => onAuthenticated(user)}>authenticate-test</button>
      <button onClick={onOwnerInitialized}>owner-initialized-test</button>
      <button onClick={onOwnerAlreadyConfigured}>owner-configured-test</button>
    </div>;
  },
}));

vi.mock("./layout/AppShell", () => ({
  AppShell: ({ user: shellUser, sessionId, onLogout, onSessionRequired }: { user: typeof user; sessionId: number; onLogout: () => void; onSessionRequired: () => void }) => (
    <div>
      shell:{shellUser.email}
      <span>shell-session:{sessionId}</span>
      <button onClick={onLogout}>logout-test</button>
      <button onClick={onSessionRequired}>expire-test</button>
    </div>
  ),
}));

const mockedOwnerExists = vi.mocked(ownerExists);
const mockedCurrentUser = vi.mocked(currentUser);
const mockedLogout = vi.mocked(logout);
const sessionRequired = { category: "authentication", code: "SESSION_REQUIRED", message: "Sesion requerida" };

beforeEach(() => {
  mockedOwnerExists.mockResolvedValue(true);
  mockedCurrentUser.mockResolvedValue(user);
  mockedLogout.mockResolvedValue();
});

describe("App bootstrap", () => {
  it("opens owner setup when no owner exists", async () => {
    mockedOwnerExists.mockResolvedValue(false);
    render(<App />);
    expect(await screen.findByText("auth:setup")).toBeTruthy();
  });

  it("moves setup to login without opening the shell", async () => {
    mockedOwnerExists.mockResolvedValue(false);
    render(<App />);
    expect(await screen.findByText("auth:setup")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "owner-initialized-test" }));
    expect(screen.getByText("auth:login")).toBeTruthy();
    expect(screen.getByText("Propietario creado, inicia sesión")).toBeTruthy();
    expect(screen.queryByText(`shell:${user.email}`)).toBeNull();
  });

  it("opens login when the owner exists but the session is required", async () => {
    mockedCurrentUser.mockRejectedValue(sessionRequired);
    render(<App />);
    expect(await screen.findByText("auth:login")).toBeTruthy();
  });

  it("restores an active session", async () => {
    render(<App />);
    expect(await screen.findByText(`shell:${user.email}`)).toBeTruthy();
  });

  it("logs out from the shell", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "logout-test" }));
    expect(await screen.findByText("auth:login")).toBeTruthy();
    expect(mockedLogout).toHaveBeenCalledOnce();
  });

  it("assigns a new session generation when the same user authenticates again", async () => {
    render(<App />);
    expect(await screen.findByText("shell-session:1")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "logout-test" }));
    expect(await screen.findByText("auth:login")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "authenticate-test" }));
    expect(screen.getByText("shell-session:2")).toBeTruthy();
  });

  it("returns to login when the shell reports SESSION_REQUIRED", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "expire-test" }));
    expect(screen.getByText("auth:login")).toBeTruthy();
    expect(screen.getByText("Tu sesion vencio. Inicia sesion para continuar.")).toBeTruthy();
  });

  it("does not expose a technical bootstrap error", async () => {
    mockedOwnerExists.mockRejectedValue(new Error("SQLITE_BUSY at users table"));
    render(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("No se pudo preparar la aplicacion.");
    expect(alert.textContent).not.toContain("SQLITE_BUSY");
  });

  it("shows a safe bootstrap error when checking the session fails", async () => {
    mockedCurrentUser.mockRejectedValue(new Error("IPC_NO_SESSION_RESPONSE stack"));
    render(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("No se pudo comprobar la sesion.");
    expect(alert.textContent).not.toContain("IPC_NO_SESSION_RESPONSE");
  });

  it("reports logout errors and keeps the current session", async () => {
    mockedLogout.mockRejectedValue(new Error("transport details"));
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "logout-test" }));
    expect(await screen.findByText("No se pudo cerrar la sesion.")).toBeTruthy();
    expect(screen.getByText(`shell:${user.email}`)).toBeTruthy();
  });

  it("ignores StrictMode bootstrap responses from an older generation", async () => {
    const firstOwner = deferred<boolean>();
    const firstUser = deferred<typeof user>();
    const secondOwner = deferred<boolean>();
    const newerUser = { ...user, id: 8, email: "new@example.com" };
    const secondUser = deferred<typeof user>();
    mockedOwnerExists.mockReturnValueOnce(firstOwner.promise).mockReturnValueOnce(secondOwner.promise);
    mockedCurrentUser.mockReturnValueOnce(firstUser.promise).mockReturnValueOnce(secondUser.promise);
    render(<StrictMode><App /></StrictMode>);

    await act(async () => {
      secondOwner.resolve(true);
      secondUser.resolve(newerUser);
    });
    expect(screen.getByText("shell:new@example.com")).toBeTruthy();
    await act(async () => {
      firstOwner.resolve(true);
      firstUser.resolve(user);
    });
    expect(screen.getByText("shell:new@example.com")).toBeTruthy();
    expect(screen.queryByText(`shell:${user.email}`)).toBeNull();
  });

  it("does not let a pending bootstrap overwrite a newly authenticated session", async () => {
    mockedOwnerExists.mockResolvedValueOnce(false);
    render(<App />);
    expect(await screen.findByText("auth:setup")).toBeTruthy();

    const ownerRequest = deferred<boolean>();
    const userRequest = deferred<typeof user>();
    mockedOwnerExists.mockReturnValueOnce(ownerRequest.promise);
    mockedCurrentUser.mockReturnValueOnce(userRequest.promise);
    fireEvent.click(screen.getByRole("button", { name: "owner-configured-test" }));
    act(() => authHarness.authenticate?.(user));
    expect(screen.getByText(`shell:${user.email}`)).toBeTruthy();
    await act(async () => {
      ownerRequest.resolve(true);
      userRequest.resolve({ ...user, email: "stale@example.com" });
    });
    expect(screen.getByText(`shell:${user.email}`)).toBeTruthy();
    expect(screen.queryByText("shell:stale@example.com")).toBeNull();
  });
});
