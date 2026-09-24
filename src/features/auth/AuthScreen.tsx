import { useEffect, useRef, useState, type FormEvent } from "react";
import { initializeOwner, login } from "../../api/auth";
import { errorMessage, isAppError } from "../../lib/app-error";
import type { User } from "../../types/user";
import { Brand, Button, Field } from "../../components/ui";

type AuthScreenProps = {
  mode: "login" | "setup";
  confirmation?: string;
  onAuthenticated: (user: User) => void;
  onOwnerInitialized: () => void;
  onOwnerAlreadyConfigured: () => void;
};

export function AuthScreen({ mode, confirmation, onAuthenticated, onOwnerInitialized, onOwnerAlreadyConfigured }: AuthScreenProps) {
  const firstFieldRef = useRef<HTMLInputElement>(null);
  const confirmPasswordRef = useRef<HTMLInputElement>(null);
  const passwordRef = useRef<HTMLInputElement>(null);
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<string>();
  const [confirmPasswordError, setConfirmPasswordError] = useState<string>();

  useEffect(() => {
    setMessage(undefined);
    setConfirmPasswordError(undefined);
    firstFieldRef.current?.focus();
  }, [mode]);

  useEffect(() => {
    if (confirmPasswordError) confirmPasswordRef.current?.focus();
  }, [confirmPasswordError]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (submitting) return;

    const values = new FormData(event.currentTarget);
    setSubmitting(true);
    setMessage(undefined);
    setConfirmPasswordError(undefined);

    try {
      const credentials = {
        email: String(values.get("email") ?? ""),
        password: String(values.get("password") ?? ""),
      };
      if (mode === "setup") {
        await initializeOwner({
            ...credentials,
            first_name: String(values.get("firstName") ?? ""),
            last_name: String(values.get("lastName") ?? ""),
             confirm_password: String(values.get("confirmPassword") ?? ""),
           });
        if (passwordRef.current) passwordRef.current.value = "";
        if (confirmPasswordRef.current) confirmPasswordRef.current.value = "";
        onOwnerInitialized();
      } else {
        const user: User = await login(credentials);
        onAuthenticated(user);
      }
    } catch (error: unknown) {
      if (isAppError(error) && error.code === "OWNER_ALREADY_CONFIGURED") {
        onOwnerAlreadyConfigured();
        return;
      }
      if (isAppError(error) && error.category === "validation" && error.field === "confirm_password") {
        setConfirmPasswordError(error.message);
        return;
      }
      setMessage(errorMessage(error, mode === "setup"
        ? "No se pudo configurar al propietario. Intenta nuevamente."
        : "No se pudo iniciar sesion. Intenta nuevamente."));
    } finally {
      setSubmitting(false);
    }
  }

  const isSetup = mode === "setup";

  return (
    <main className="relative grid h-dvh place-items-center overflow-hidden bg-canvas p-4 sm:p-6">
      <div className="pointer-events-none absolute -top-32 -right-24 size-96 rounded-full bg-brand/8 blur-3xl" />
      <div className="pointer-events-none absolute -bottom-40 -left-24 size-96 rounded-full bg-info/8 blur-3xl" />

      <section className="relative max-h-full w-full max-w-[480px] overflow-y-auto overscroll-contain rounded-card border border-line bg-surface px-6 py-6 shadow-panel sm:px-8" aria-labelledby="auth-title">
        <Brand />
        <div className={`${isSetup ? "mt-5 pt-4" : "mt-7 pt-6"} border-t border-line`}>
          <p className="text-[0.625rem] font-extrabold tracking-[0.14em] text-brand uppercase">
            {isSetup ? "Primer inicio" : "Acceso seguro"}
          </p>
          <h1 id="auth-title" className="mt-2 font-heading text-2xl font-bold tracking-[-0.03em] text-ink sm:text-3xl">
            {isSetup ? "Configura al propietario" : "Bienvenido de nuevo"}
          </h1>
          <p className="mt-2 text-sm leading-5 text-muted">
            {isSetup
              ? "Crea la cuenta que administrara este punto de venta."
              : "Ingresa tus credenciales para abrir el punto de venta."}
          </p>
        </div>

        <form className={`${isSetup ? "mt-5 gap-3" : "mt-7 gap-5"} grid`} onSubmit={handleSubmit} aria-busy={submitting}>
          {isSetup && (
            <div className="grid gap-3 sm:grid-cols-2">
              <Field ref={firstFieldRef} label="Nombre" name="firstName" autoComplete="given-name" required disabled={submitting} />
              <Field label="Apellido" name="lastName" autoComplete="family-name" required disabled={submitting} />
            </div>
          )}
          <Field
            ref={isSetup ? undefined : firstFieldRef}
            label="Correo electronico"
            name="email"
            type="email"
            inputMode="email"
            autoComplete="username"
            placeholder="tu@negocio.com"
            required
            disabled={submitting}
          />
          <Field
            ref={passwordRef}
            label="Contrasena"
            name="password"
            type="password"
            autoComplete={isSetup ? "new-password" : "current-password"}
            required
            disabled={submitting}
          />
          {isSetup && (
            <Field
              ref={confirmPasswordRef}
              label="Confirmar contrasena"
              name="confirmPassword"
              type="password"
              autoComplete="new-password"
              required
              disabled={submitting}
              error={confirmPasswordError}
            />
          )}

          {message && (
            <p role="alert" className="rounded-control border border-danger/20 bg-danger-soft px-3 py-2.5 text-sm leading-5 text-danger">
              {message}
            </p>
          )}

          {confirmation && !message && (
            <p role="status" className="rounded-control border border-brand/20 bg-brand-soft px-3 py-2.5 text-sm leading-5 text-brand-dark">
              {confirmation}
            </p>
          )}

          <Button type="submit" variant="primary" size="lg" loading={submitting} className="mt-1 w-full">
            {submitting ? (isSetup ? "Configurando..." : "Ingresando...") : (isSetup ? "Crear cuenta y continuar" : "Iniciar sesion")}
          </Button>
        </form>

        {import.meta.env.DEV && !isSetup && (
          <aside className="mt-6 rounded-control border border-dashed border-line-strong bg-surface-soft p-3 text-xs leading-5 text-muted" aria-label="Credenciales de desarrollo">
            <strong className="block text-ink">Acceso de desarrollo</strong>
            <span className="block">admin@pos.local</span>
            <span className="block">PosDemo123!</span>
          </aside>
        )}
      </section>
    </main>
  );
}
