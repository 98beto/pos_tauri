import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { decodeTauriPublicKey, decodeTauriSignature, verifyUpdaterSignatures } from "./verify-updater-signatures.mjs";

function available(command, args) {
  return spawnSync(command, args, { stdio: "ignore" }).status === 0;
}

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: "ignore" });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} failed (${signal ?? `exit ${code}`})`));
    });
  });
}

const minisignAvailable = available(process.env.MINISIGN_BIN || "minisign", ["-v"]);
const tauriAvailable = available("pnpm", ["exec", "tauri", "--version"]);
const required = process.env.REQUIRE_MINISIGN_INTEGRATION === "1";

test("Tauri CLI signatures verify with minisign", {
  skip: !required && (!minisignAvailable || !tauriAvailable),
}, async (t) => {
  assert.ok(minisignAvailable, "minisign is required for the publish integration test");
  assert.ok(tauriAvailable, "the installed Tauri CLI is required for the publish integration test");

  const root = await mkdtemp(path.join(os.tmpdir(), "pos-tauri-signature-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const key = path.join(root, "updater.key");
  const assets = path.join(root, "assets");
  await mkdir(assets);
  await run("pnpm", ["exec", "tauri", "signer", "generate", "--ci", "--password", "", "--write-keys", key]);

  const appImage = path.join(assets, "Linea POS_1.2.3_amd64.AppImage");
  const nsis = path.join(assets, "Linea POS_1.2.3_x64-setup.exe");
  await Promise.all([
    writeFile(appImage, "appimage integration artifact"),
    writeFile(nsis, "nsis integration artifact"),
    writeFile(path.join(assets, "Linea POS_1.2.3_amd64.deb"), "deb"),
    writeFile(path.join(assets, "Linea POS-1.2.3-1.x86_64.rpm"), "rpm"),
    writeFile(path.join(assets, "Linea POS_1.2.3_x64_en-US.msi"), "msi"),
  ]);
  await run("pnpm", ["exec", "tauri", "signer", "sign", "--private-key-path", key, "--password", "", appImage]);
  await run("pnpm", ["exec", "tauri", "signer", "sign", "--private-key-path", key, "--password", "", nsis]);

  const publicKey = await readFile(`${key}.pub`, "utf8");
  const [appImageSignature, nsisSignature] = await Promise.all([
    readFile(`${appImage}.sig`, "utf8"),
    readFile(`${nsis}.sig`, "utf8"),
  ]);
  decodeTauriPublicKey(publicKey);
  decodeTauriSignature(appImageSignature);
  decodeTauriSignature(nsisSignature);
  await verifyUpdaterSignatures({ assetsDirectory: assets, publicKey });
});
