import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type AvailableUpdate = {
  currentVersion: string;
  version: string;
  notes?: string;
  downloadAndInstall: (onEvent: (event: DownloadEvent) => void) => Promise<void>;
  close: () => Promise<void>;
};

export type UpdaterErrorKind = "network" | "integrity" | "configuration" | "download" | "installation" | "unknown";

let startupCheck: Promise<AvailableUpdate | null> | undefined;
let checkInFlight: Promise<AvailableUpdate | null> | undefined;

async function performCheck() {
  const update = await check();
  if (!update) return null;
  return mapUpdate(update);
}

function mapUpdate(update: Update): AvailableUpdate {
  return {
    currentVersion: update.currentVersion,
    version: update.version,
    notes: update.body,
    downloadAndInstall: (onEvent) => update.downloadAndInstall(onEvent, { restartAfterInstall: false }),
    close: () => update.close(),
  };
}

function singleFlightCheck() {
  if (checkInFlight) return checkInFlight;
  const request = performCheck();
  checkInFlight = request;
  void request.finally(() => {
    if (checkInFlight === request) checkInFlight = undefined;
  }).catch(() => undefined);
  return request;
}

export function checkForUpdateAtStartup() {
  startupCheck ??= singleFlightCheck();
  return startupCheck;
}

export function checkForUpdate() {
  return singleFlightCheck();
}

export function relaunchApplication() {
  return relaunch();
}

export function classifyUpdaterError(error: unknown, phase: "check" | "download" | "installation"): UpdaterErrorKind {
  const detail = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  const normalized = detail.toLowerCase();
  if (/signature|public key|checksum|integrity|verify|verification/.test(normalized)) return "integrity";
  if (/network|fetch|timeout|timed out|dns|connection|offline|socket/.test(normalized)) return "network";
  if (/configuration|configured|updater config|endpoint|manifest/.test(normalized)) return "configuration";
  if (phase === "installation") return "installation";
  if (phase === "download") return "download";
  return "unknown";
}

export function resetUpdaterSessionForTests() {
  startupCheck = undefined;
  checkInFlight = undefined;
}
