import { useCallback, useEffect, useRef, useState } from "react";
import { currentUser, logout, ownerExists } from "./api/auth";
import { Spinner, Toast } from "./components/ui";
import { AuthScreen } from "./features/auth/AuthScreen";
import { UpdateController } from "./features/updater/UpdateController";
import { AppShell } from "./layout/AppShell";
import { errorMessage, isSessionRequired } from "./lib/app-error";
import type { User } from "./types/user";

type AppState =
  | { screen: "booting" }
  | { screen: "error"; message: string }
  | { screen: "auth"; mode: "login" | "setup"; confirmation?: string }
  | { screen: "shell"; user: User; sessionId: number };

type Notice = { tone: "success" | "error" | "info"; message: string };

function App() {
  const [state, setState] = useState<AppState>({ screen: "booting" });
  const [notice, setNotice] = useState<Notice>();
  const [loggingOut, setLoggingOut] = useState(false);
  const active = useRef(false);
  const bootstrapGeneration = useRef(0);
  const sessionGeneration = useRef(0);

  const bootstrap = useCallback(async () => {
    const generation = ++bootstrapGeneration.current;
    if (active.current) setState({ screen: "booting" });
    const [ownerResult, userResult] = await Promise.allSettled([ownerExists(), currentUser()]);
    if (!active.current || bootstrapGeneration.current !== generation) return;

    if (ownerResult.status === "rejected") {
      setState({ screen: "error", message: errorMessage(ownerResult.reason, "No se pudo preparar la aplicacion.") });
      return;
    }
    if (!ownerResult.value) {
      setState({ screen: "auth", mode: "setup" });
      return;
    }
    if (userResult.status === "fulfilled") {
      setState({ screen: "shell", user: userResult.value, sessionId: ++sessionGeneration.current });
      return;
    }
    if (isSessionRequired(userResult.reason)) {
      setState({ screen: "auth", mode: "login" });
      return;
    }
    setState({ screen: "error", message: errorMessage(userResult.reason, "No se pudo comprobar la sesion.") });
  }, []);

  useEffect(() => {
    active.current = true;
    void bootstrap();
    return () => {
      active.current = false;
      bootstrapGeneration.current += 1;
    };
  }, [bootstrap]);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(undefined), 3500);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  async function handleLogout() {
    setLoggingOut(true);
    try {
      await logout();
      setState({ screen: "auth", mode: "login" });
      setNotice({ tone: "info", message: "La sesion se cerro correctamente." });
    } catch (error: unknown) {
      if (isSessionRequired(error)) {
        setState({ screen: "auth", mode: "login" });
      } else {
        setNotice({ tone: "error", message: errorMessage(error, "No se pudo cerrar la sesion.") });
      }
    } finally {
      setLoggingOut(false);
    }
  }

  function handleSessionRequired() {
    setState({ screen: "auth", mode: "login" });
    setNotice({ tone: "error", message: "Tu sesion vencio. Inicia sesion para continuar." });
  }

  let content;
  if (state.screen === "booting") {
    content = (
      <main className="grid min-h-dvh place-items-center bg-canvas" aria-label="Iniciando aplicacion">
        <div className="text-center text-sm font-semibold text-muted"><Spinner className="mx-auto mb-3 size-6 text-brand" />Preparando Linea POS...</div>
      </main>
    );
  } else if (state.screen === "error") {
    content = (
      <main className="grid min-h-dvh place-items-center bg-canvas p-6">
        <section className="w-full max-w-md rounded-card border border-line bg-surface p-8 text-center shadow-panel" role="alert">
          <h1 className="font-heading text-2xl font-bold">No pudimos iniciar Linea POS</h1>
          <p className="mt-3 text-sm leading-6 text-muted">{state.message}</p>
          <button type="button" className="mt-6 rounded-control bg-brand px-5 py-3 text-sm font-bold text-white focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand" onClick={() => void bootstrap()}>Reintentar</button>
        </section>
      </main>
    );
  } else if (state.screen === "auth") {
    content = (
      <AuthScreen
        mode={state.mode}
        confirmation={state.confirmation}
        onOwnerAlreadyConfigured={() => void bootstrap()}
        onOwnerInitialized={() => {
          bootstrapGeneration.current += 1;
          setState({ screen: "auth", mode: "login", confirmation: "Propietario creado, inicia sesión" });
        }}
        onAuthenticated={(user) => {
          bootstrapGeneration.current += 1;
          setState({ screen: "shell", user, sessionId: ++sessionGeneration.current });
          setNotice({ tone: "success", message: `Bienvenido, ${user.first_name}.` });
        }}
      />
    );
  } else {
    content = (
      <AppShell
        user={state.user}
        sessionId={state.sessionId}
        loggingOut={loggingOut}
        onLogout={() => void handleLogout()}
        onNotice={setNotice}
        onSessionRequired={handleSessionRequired}
      />
    );
  }

  return (
    <UpdateController sessionId={state.screen === "shell" ? state.sessionId : undefined}>
      {content}
      <Toast open={Boolean(notice)} tone={notice?.tone} message={notice?.message ?? ""} />
    </UpdateController>
  );
}

export default App;
