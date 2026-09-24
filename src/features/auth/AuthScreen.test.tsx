import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initializeOwner, login } from "../../api/auth";
import { user } from "../../test/fixtures";
import { AuthScreen } from "./AuthScreen";

vi.mock("../../api/auth", () => ({ initializeOwner: vi.fn(), login: vi.fn() }));

const mockedLogin = vi.mocked(login);
const mockedInitializeOwner = vi.mocked(initializeOwner);

beforeEach(() => {
  mockedLogin.mockResolvedValue(user);
  mockedInitializeOwner.mockResolvedValue(user);
});

function renderAuth(mode: "login" | "setup" = "login") {
  const onAuthenticated = vi.fn();
  const onOwnerInitialized = vi.fn();
  const onOwnerAlreadyConfigured = vi.fn();
  render(<AuthScreen mode={mode} onAuthenticated={onAuthenticated} onOwnerInitialized={onOwnerInitialized} onOwnerAlreadyConfigured={onOwnerAlreadyConfigured} />);
  return { onAuthenticated, onOwnerInitialized, onOwnerAlreadyConfigured };
}

describe("AuthScreen", () => {
  it("submits login with Enter and reports successful authentication", async () => {
    const events = userEvent.setup();
    const { onAuthenticated } = renderAuth();
    await events.type(screen.getByLabelText(/^Correo electronico/), "ada@example.com");
    await events.type(screen.getByLabelText(/^Contrasena/), "secret{Enter}");

    await waitFor(() => expect(mockedLogin).toHaveBeenCalledWith({ email: "ada@example.com", password: "secret" }));
    expect(onAuthenticated).toHaveBeenCalledWith(user);
  });

  it("disables the form while login is pending and ignores a second submit", async () => {
    let resolve!: (value: typeof user) => void;
    mockedLogin.mockReturnValue(new Promise((done) => { resolve = done; }));
    renderAuth();
    fireEvent.change(screen.getByLabelText(/^Correo electronico/), { target: { value: "ada@example.com" } });
    fireEvent.change(screen.getByLabelText(/^Contrasena/), { target: { value: "secret" } });
    const button = screen.getByRole("button", { name: "Iniciar sesion" }) as HTMLButtonElement;
    fireEvent.click(button);
    fireEvent.submit(button.closest("form")!);

    expect(button.disabled).toBe(true);
    expect(mockedLogin).toHaveBeenCalledOnce();
    resolve(user);
    await waitFor(() => expect(screen.queryByText("Ingresando...")).toBeNull());
  });

  it("uses a safe message for an unknown login error", async () => {
    mockedLogin.mockRejectedValue(new Error("stack: sqlite auth query"));
    renderAuth();
    fireEvent.change(screen.getByLabelText(/^Correo electronico/), { target: { value: "ada@example.com" } });
    fireEvent.change(screen.getByLabelText(/^Contrasena/), { target: { value: "bad" } });
    fireEvent.click(screen.getByRole("button", { name: "Iniciar sesion" }));

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("No se pudo iniciar sesion. Intenta nuevamente.");
    expect(alert.textContent).not.toContain("sqlite");
  });

  it("maps the backend confirm_password error to the confirmation field", async () => {
    mockedInitializeOwner.mockRejectedValue({
      category: "validation",
      code: "PASSWORD_MISMATCH",
      field: "confirm_password",
      message: "Las contrasenas no coinciden.",
    });
    renderAuth("setup");
    fireEvent.change(screen.getByLabelText(/^Nombre/), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText(/^Apellido/), { target: { value: "Lovelace" } });
    fireEvent.change(screen.getByLabelText(/^Correo electronico/), { target: { value: "ada@example.com" } });
    fireEvent.change(screen.getByLabelText(/^Contrasena/), { target: { value: "one" } });
    const confirmation = screen.getByLabelText(/^Confirmar contrasena/) as HTMLInputElement;
    fireEvent.change(confirmation, { target: { value: "two" } });
    fireEvent.click(screen.getByRole("button", { name: "Crear cuenta y continuar" }));

    await screen.findByText("Las contrasenas no coinciden.");
    expect(mockedInitializeOwner).toHaveBeenCalledWith(expect.objectContaining({ confirm_password: "two" }));
    expect(confirmation.getAttribute("aria-invalid")).toBe("true");
    expect(document.activeElement).toBe(confirmation);
  });

  it("creates the owner without logging in and clears both password fields", async () => {
    const { onAuthenticated, onOwnerInitialized } = renderAuth("setup");
    fireEvent.change(screen.getByLabelText(/^Nombre/), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText(/^Apellido/), { target: { value: "Lovelace" } });
    fireEvent.change(screen.getByLabelText(/^Correo electronico/), { target: { value: "ada@example.com" } });
    fireEvent.change(screen.getByLabelText(/^Contrasena/), { target: { value: "secret" } });
    fireEvent.change(screen.getByLabelText(/^Confirmar contrasena/), { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Crear cuenta y continuar" }));

    await waitFor(() => expect(mockedInitializeOwner).toHaveBeenCalledWith({
      first_name: "Ada",
      last_name: "Lovelace",
      email: "ada@example.com",
      password: "secret",
      confirm_password: "secret",
    }));
    expect(mockedLogin).not.toHaveBeenCalled();
    expect(onAuthenticated).not.toHaveBeenCalled();
    expect(onOwnerInitialized).toHaveBeenCalledOnce();
    expect((screen.getByLabelText(/^Contrasena/) as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText(/^Confirmar contrasena/) as HTMLInputElement).value).toBe("");
  });

  it("shows the owner-created confirmation in login mode", () => {
    render(<AuthScreen mode="login" confirmation="Propietario creado, inicia sesión" onAuthenticated={vi.fn()} onOwnerInitialized={vi.fn()} onOwnerAlreadyConfigured={vi.fn()} />);
    expect(screen.getByRole("status").textContent).toBe("Propietario creado, inicia sesión");
  });

  it("delegates OWNER_ALREADY_CONFIGURED to bootstrap", async () => {
    mockedInitializeOwner.mockRejectedValue({ category: "conflict", code: "OWNER_ALREADY_CONFIGURED", message: "already configured" });
    const { onOwnerAlreadyConfigured } = renderAuth("setup");
    fireEvent.change(screen.getByLabelText(/^Nombre/), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText(/^Apellido/), { target: { value: "Lovelace" } });
    fireEvent.change(screen.getByLabelText(/^Correo electronico/), { target: { value: "ada@example.com" } });
    fireEvent.change(screen.getByLabelText(/^Contrasena/), { target: { value: "secret" } });
    fireEvent.change(screen.getByLabelText(/^Confirmar contrasena/), { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Crear cuenta y continuar" }));

    await waitFor(() => expect(onOwnerAlreadyConfigured).toHaveBeenCalledOnce());
  });
});
