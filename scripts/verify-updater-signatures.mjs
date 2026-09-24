import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import os from "node:os";
import path from "node:path";

import { inspectReleaseAssets } from "./latest-json.mjs";
import { decodeTauriPublicKey, decodeTauriSignature } from "./tauri-signature-format.mjs";

export { decodeTauriPublicKey, decodeTauriSignature } from "./tauri-signature-format.mjs";

function runMinisign(executable, artifact, signature, publicKeyFile) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, [
      "-Vm", artifact,
      "-x", signature,
      "-p", publicKeyFile,
      "-q",
    ], { stdio: "ignore" });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(
        `minisign rejected ${path.basename(artifact)} (${signal ?? `exit ${code}`})`,
      ));
    });
  });
}

export async function verifyUpdaterSignatures({
  assetsDirectory,
  publicKey,
  minisign = process.env.MINISIGN_BIN || "minisign",
}) {
  const assets = await inspectReleaseAssets(assetsDirectory);
  const temporaryDirectory = await mkdtemp(path.join(os.tmpdir(), "pos-minisign-"));
  const publicKeyFile = path.join(temporaryDirectory, "updater.pub");
  const appImageSignature = path.join(temporaryDirectory, "appimage.sig");
  const nsisSignature = path.join(temporaryDirectory, "nsis.sig");

  try {
    await Promise.all([
      writeFile(publicKeyFile, decodeTauriPublicKey(publicKey), { mode: 0o600 }),
      writeFile(appImageSignature, decodeTauriSignature(assets.linuxSignature), { mode: 0o600 }),
      writeFile(nsisSignature, decodeTauriSignature(assets.windowsSignature), { mode: 0o600 }),
    ]);
    await runMinisign(minisign, assets.appImage, appImageSignature, publicKeyFile);
    await runMinisign(minisign, assets.nsis, nsisSignature, publicKeyFile);
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}

function optionsFromArguments(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    if (!args[index]?.startsWith("--") || args[index + 1] === undefined) {
      throw new Error(`Invalid argument ${args[index] ?? ""}`);
    }
    options[args[index].slice(2)] = args[index + 1];
  }
  return options;
}

const isMain = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  try {
    const options = optionsFromArguments(process.argv.slice(2));
    if (!options.assets) throw new Error("--assets is required");
    await verifyUpdaterSignatures({
      assetsDirectory: options.assets,
      publicKey: process.env.TAURI_UPDATER_PUBLIC_KEY,
      minisign: options.minisign,
    });
    console.log("Updater signatures verified.");
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
