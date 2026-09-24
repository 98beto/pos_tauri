import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  checkForUpdate,
  checkForUpdateAtStartup,
  classifyUpdaterError,
  relaunchApplication,
  type AvailableUpdate,
  type UpdaterErrorKind,
} from "../../api/updater";
import { Button, Modal, Spinner, Toast } from "../../components/ui";
import { UpdaterProvider, type CartReportVersion, type PosUpdateSafety } from "./UpdaterContext";

type Stage = "idle" | "available" | "downloading" | "installing" | "ready" | "error";
type Notice = { tone: "success" | "error" | "info"; message: string };

const safeErrors: Record<UpdaterErrorKind, string> = {
  network: "No se pudo conectar para completar la actualización. Revisa tu conexión e intenta de nuevo.",
  integrity: "La actualización no superó la verificación de firma o integridad. No se instaló ningún cambio.",
  configuration: "La configuración de actualizaciones no es válida. Contacta a soporte antes de intentar actualizar.",
  download: "La descarga se interrumpió antes de completar la actualización. Puedes intentarlo de nuevo.",
  installation: "La descarga terminó, pero no se pudo instalar la actualización. Puedes intentarlo de nuevo.",
  unknown: "No se pudo completar la actualización de forma segura. Intenta de nuevo.",
};

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function UpdateController({ sessionId, children }: { sessionId?: number; children: ReactNode }) {
  const [stage, setStage] = useState<Stage>("idle");
  const [update, setUpdate] = useState<AvailableUpdate>();
  const [checking, setChecking] = useState(false);
  const [notice, setNotice] = useState<Notice>();
  const [errorKind, setErrorKind] = useState<UpdaterErrorKind>("unknown");
  const [downloaded, setDownloaded] = useState(0);
  const [downloadTotal, setDownloadTotal] = useState<number>();
  const [safety, setSafety] = useState<PosUpdateSafety>({ cartNonEmpty: false, cartMutationInFlight: false, checkoutInFlight: false });
  const active = useRef(false);
  const actionInFlight = useRef(false);
  const posSafety = useRef<PosUpdateSafety>({ cartNonEmpty: false, cartMutationInFlight: false, checkoutInFlight: false });
  const sessionIdRef = useRef(sessionId);
  const cartOwner = useRef<{ sessionId: number; nonEmpty: boolean } | undefined>(undefined);
  const cartGeneration = useRef(0);
  const latestCartOperation = useRef<{ sessionId: number; version: CartReportVersion } | undefined>(undefined);
  const cartMutations = useRef(new Map<symbol, number>());
  const checkouts = useRef(new Map<symbol, number>());
  const updateResource = useRef<AvailableUpdate | undefined>(undefined);
  const closedUpdates = useRef(new WeakSet<AvailableUpdate>());
  const stageFocus = useRef<HTMLElement | null>(null);
  const setStageFocus = useCallback((node: HTMLElement | null) => {
    stageFocus.current = node;
  }, []);
  sessionIdRef.current = sessionId;

  const closeUpdate = useCallback((resource: AvailableUpdate | undefined) => {
    if (!resource || closedUpdates.current.has(resource)) return;
    closedUpdates.current.add(resource);
    void resource.close().catch(() => undefined);
  }, []);

  const refreshSafety = useCallback(() => {
    const owner = cartOwner.current;
    const next = {
      cartNonEmpty: Boolean(owner && owner.sessionId === sessionIdRef.current && owner.nonEmpty),
      cartMutationInFlight: cartMutations.current.size > 0,
      checkoutInFlight: checkouts.current.size > 0,
    };
    posSafety.current = next;
    if (active.current) {
      setSafety((current) => current.cartNonEmpty === next.cartNonEmpty && current.cartMutationInFlight === next.cartMutationInFlight && current.checkoutInFlight === next.checkoutInFlight ? current : next);
    }
  }, []);

  const presentUpdate = useCallback((available: AvailableUpdate) => {
    const previous = updateResource.current;
    updateResource.current = available;
    setUpdate(available);
    if (previous !== available) closeUpdate(previous);
    setErrorKind("unknown");
    setDownloaded(0);
    setDownloadTotal(undefined);
    setStage("available");
  }, [closeUpdate]);

  useEffect(() => {
    active.current = true;
    let cancelled = false;
    void checkForUpdateAtStartup()
      .then((available) => {
        if (!available) return;
        if (!cancelled) presentUpdate(available);
        else if (!active.current) closeUpdate(available);
      })
      .catch((error: unknown) => {
        if (cancelled || !active.current) return;
        const kind = classifyUpdaterError(error, "check");
        if (kind === "network" || kind === "unknown") return;
        setErrorKind(kind);
        setStage("error");
      });
    return () => {
      cancelled = true;
      active.current = false;
      const retained = updateResource.current;
      updateResource.current = undefined;
      closeUpdate(retained);
    };
  }, [closeUpdate, presentUpdate]);

  useEffect(() => {
    if (cartOwner.current && cartOwner.current.sessionId !== sessionId) cartOwner.current = undefined;
    if (latestCartOperation.current && latestCartOperation.current.sessionId !== sessionId) latestCartOperation.current = undefined;
    refreshSafety();
  }, [sessionId, refreshSafety]);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(undefined), 4000);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  useEffect(() => {
    if (stage === "idle" || stage === "available") return;
    const frame = window.requestAnimationFrame(() => {
      const target = stageFocus.current;
      const dialog = target?.closest('[role="dialog"]');
      if (target && dialog && !dialog.contains(document.activeElement)) {
        target.focus({ preventScroll: true });
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [stage]);

  const manualCheck = useCallback(() => {
    if (checking || actionInFlight.current) return;
    setChecking(true);
    setNotice(undefined);
    void checkForUpdate()
      .then((available) => {
        if (!active.current) {
          closeUpdate(available ?? undefined);
          return;
        }
        if (available) presentUpdate(available);
        else setNotice({ tone: "success", message: "Línea POS ya está actualizada." });
      })
      .catch((error: unknown) => {
        if (!active.current) return;
        const kind = classifyUpdaterError(error, "check");
        setNotice({
          tone: "error",
          message: kind === "network"
            ? "No se pudo buscar actualizaciones. Revisa tu conexión e intenta de nuevo."
            : "No se pudo comprobar si hay actualizaciones. Intenta de nuevo.",
        });
      })
      .finally(() => {
        if (active.current) setChecking(false);
      });
  }, [checking, closeUpdate, presentUpdate]);

  const beginCartReporting = useCallback((ownerSessionId: number) => {
    const generation = ++cartGeneration.current;
    if (sessionIdRef.current === ownerSessionId) {
      latestCartOperation.current = { sessionId: ownerSessionId, version: { generation, operation: 0 } };
    }
    return generation;
  }, []);

  const beginCartOperation = useCallback((ownerSessionId: number, version: CartReportVersion) => {
    if (sessionIdRef.current !== ownerSessionId) return false;
    const latest = latestCartOperation.current;
    if (latest && latest.sessionId === ownerSessionId && (
      version.generation < latest.version.generation ||
      (version.generation === latest.version.generation && version.operation < latest.version.operation)
    )) return false;
    latestCartOperation.current = { sessionId: ownerSessionId, version };
    return true;
  }, []);

  const reportCart = useCallback((ownerSessionId: number, version: CartReportVersion, cartNonEmpty: boolean) => {
    const latest = latestCartOperation.current;
    if (sessionIdRef.current !== ownerSessionId || !latest || latest.sessionId !== ownerSessionId ||
      latest.version.generation !== version.generation || latest.version.operation !== version.operation) return false;
    cartOwner.current = { sessionId: ownerSessionId, nonEmpty: cartNonEmpty };
    refreshSafety();
    return true;
  }, [refreshSafety]);

  const runCartMutation = useCallback(<T,>(ownerSessionId: number, version: CartReportVersion, operation: () => Promise<T>) => {
    beginCartOperation(ownerSessionId, version);
    const token = Symbol(ownerSessionId);
    cartMutations.current.set(token, ownerSessionId);
    refreshSafety();
    let request: Promise<T>;
    try {
      request = operation();
    } catch (error) {
      cartMutations.current.delete(token);
      refreshSafety();
      return Promise.reject(error);
    }
    return request.finally(() => {
      cartMutations.current.delete(token);
      refreshSafety();
    });
  }, [beginCartOperation, refreshSafety]);

  const runCheckout = useCallback(<T,>(ownerSessionId: number, operation: () => Promise<T>) => {
    const token = Symbol(ownerSessionId);
    checkouts.current.set(token, ownerSessionId);
    refreshSafety();
    let request: Promise<T>;
    try {
      request = operation();
    } catch (error) {
      checkouts.current.delete(token);
      refreshSafety();
      return Promise.reject(error);
    }
    return request.finally(() => {
      checkouts.current.delete(token);
      refreshSafety();
    });
  }, [refreshSafety]);

  function releaseCurrentUpdate() {
    const current = updateResource.current;
    updateResource.current = undefined;
    setUpdate(undefined);
    closeUpdate(current);
  }

  function postpone() {
    if (actionInFlight.current) return;
    setStage("idle");
    releaseCurrentUpdate();
  }

  async function install() {
    if (!update || actionInFlight.current) return;
    if (posSafety.current.checkoutInFlight) {
      setNotice({ tone: "error", message: "Espera a que el cobro pendiente o en curso termine antes de actualizar." });
      return;
    }
    if (posSafety.current.cartMutationInFlight) {
      setNotice({ tone: "error", message: "Espera a que termine el cambio pendiente del carrito antes de actualizar." });
      return;
    }

    actionInFlight.current = true;
    setDownloaded(0);
    setDownloadTotal(undefined);
    setStage("downloading");
    let downloadFinished = false;
    try {
      await update.downloadAndInstall((event) => {
        if (!active.current) return;
        if (event.event === "Started") {
          setDownloadTotal(event.data.contentLength);
          setDownloaded(0);
          setStage("downloading");
        } else if (event.event === "Progress") {
          setDownloaded((value) => value + event.data.chunkLength);
        } else {
          downloadFinished = true;
          setStage("installing");
        }
      });
      if (active.current) setStage("ready");
    } catch (error: unknown) {
      if (active.current) {
        setErrorKind(classifyUpdaterError(error, downloadFinished ? "installation" : "download"));
        setStage("error");
      }
    } finally {
      actionInFlight.current = false;
    }
  }

  async function restart() {
    if (actionInFlight.current) return;
    actionInFlight.current = true;
    try {
      await relaunchApplication();
    } catch {
      actionInFlight.current = false;
      setErrorKind("unknown");
      setStage("error");
    }
  }

  function retry() {
    if (actionInFlight.current) return;
    setStage("idle");
    releaseCurrentUpdate();
    manualCheck();
  }

  const busy = stage === "downloading" || stage === "installing";
  const progress = downloadTotal && downloadTotal > 0 ? Math.min(100, Math.round((downloaded / downloadTotal) * 100)) : undefined;
  const title = stage === "available" ? "Actualización disponible"
    : stage === "downloading" ? "Descargando actualización"
      : stage === "installing" ? "Instalando actualización"
        : stage === "ready" ? "Actualización instalada"
          : "No se pudo actualizar";

  return (
    <UpdaterProvider value={{ manualCheck, checking, busy, beginCartReporting, beginCartOperation, reportCart, runCartMutation, runCheckout }}>
      {children}
      <Modal
        open={stage !== "idle"}
        onClose={stage === "available" || stage === "ready" || stage === "error" ? postpone : () => undefined}
        closeDisabled={busy}
        title={title}
        description={update ? `Versión ${update.currentVersion} → ${update.version}` : undefined}
        size="sm"
        footer={stage === "available" ? <><Button onClick={postpone}>Después</Button><Button variant="primary" onClick={() => void install()}>Actualizar ahora</Button></>
          : stage === "ready" ? <><Button onClick={postpone}>Más tarde</Button><Button variant="primary" onClick={() => void restart()}>Reiniciar y completar actualización</Button></>
            : stage === "error" ? <><Button onClick={postpone}>Cerrar</Button><Button variant="primary" onClick={retry}>Reintentar</Button></>
              : undefined}
      >
        {stage === "available" && <div className="grid gap-4 text-sm leading-6">
          <dl className="grid grid-cols-2 gap-3 rounded-card bg-surface-soft p-4"><div><dt className="text-xs font-bold text-muted">Versión actual</dt><dd className="mt-1 font-heading text-lg font-bold">{update?.currentVersion}</dd></div><div><dt className="text-xs font-bold text-muted">Nueva versión</dt><dd className="mt-1 font-heading text-lg font-bold text-brand">{update?.version}</dd></div></dl>
          <section><h3 className="font-bold text-ink">Notas de la versión</h3><p className="mt-1 whitespace-pre-wrap text-muted">{update?.notes?.trim() || "Esta versión no incluye notas adicionales."}</p></section>
          <div className="rounded-control border border-warning/30 bg-warning-soft p-3 text-warning" role="note">La aplicación puede cerrarse durante la instalación. Tendrás que iniciar sesión nuevamente.{safety.cartNonEmpty && " El carrito no cobrado está en memoria y se perderá."}</div>
        </div>}
        {stage === "downloading" && <div ref={setStageFocus} tabIndex={-1} className="py-4 text-center outline-none" aria-live="polite"><Spinner className="mx-auto size-7 text-brand" /><p className="mt-4 font-semibold">{progress === undefined ? `Descargando… ${formatBytes(downloaded)}` : `${progress}% · ${formatBytes(downloaded)} de ${formatBytes(downloadTotal ?? 0)}`}</p>{progress !== undefined && <progress className="mt-4 h-2 w-full accent-brand" max={100} value={progress}>{progress}%</progress>}<p className="mt-3 text-xs text-muted">No cierres la aplicación mientras se descarga e instala.</p></div>}
        {stage === "installing" && <div ref={setStageFocus} tabIndex={-1} className="py-4 text-center outline-none" aria-live="polite"><Spinner className="mx-auto size-7 text-brand" /><p className="mt-4 font-semibold">Verificando e instalando…</p><p className="mt-2 text-xs leading-5 text-muted">En Windows, el instalador puede cerrar la aplicación al continuar.</p></div>}
        {stage === "ready" && <div ref={setStageFocus} tabIndex={-1} className="rounded-control bg-brand-soft p-4 text-sm leading-6 text-brand-dark outline-none" role="status" aria-live="polite"><p>La instalación terminó y requiere relanzar la aplicación.</p><p className="mt-2 font-semibold">El relanzamiento solo ocurrirá cuando confirmes.</p></div>}
        {stage === "error" && <p ref={setStageFocus} tabIndex={-1} className="rounded-control bg-danger-soft p-4 text-sm leading-6 text-danger outline-none" role="alert">{safeErrors[errorKind]}</p>}
      </Modal>
      <Toast open={Boolean(notice)} tone={notice?.tone} message={notice?.message ?? ""} />
    </UpdaterProvider>
  );
}
