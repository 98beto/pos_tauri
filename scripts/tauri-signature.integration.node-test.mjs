import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  decodeTauriPublicKey,
  decodeTauriSignature,
  verifyUpdaterSignatures,
} from "./verify-updater-signatures.mjs";

function available(command, args) {
  return spawnSync(command, args, { stdio: "ignore" }).status === 0;
}

function commandAvailable(command) {
  return !spawnSync(command, ["-h"], { stdio: "ignore" }).error;
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

const minisignAvailable = commandAvailable(process.env.MINISIGN_BIN || "minisign");
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

  const appImage = path.join(assets, "Linea.POS_1.2.3_amd64.AppImage");
  const deb = path.join(assets, "Linea.POS_1.2.3_amd64.deb");
  const rpm = path.join(assets, "Linea.POS-1.2.3-1.x86_64.rpm");
  const nsis = path.join(assets, "Linea.POS_1.2.3_x64-setup.exe");
  await Promise.all([
    writeFile(appImage, "appimage integration artifact"),
    writeFile(deb, "deb integration artifact"),
    writeFile(rpm, "rpm integration artifact"),
    writeFile(nsis, "nsis integration artifact"),
    writeFile(path.join(assets, "Linea.POS_1.2.3_x64_en-US.msi"), "msi"),
  ]);
  for (const artifact of [appImage, deb, rpm, nsis]) {
    await run("pnpm", [
      "exec", "tauri", "signer", "sign",
      "--private-key-path", key,
      "--password", "",
      artifact,
    ]);
  }

  const publicKey = await readFile(`${key}.pub`, "utf8");
  const artifacts = [appImage, deb, rpm, nsis];
  const signatures = await Promise.all(artifacts.map((artifact) => readFile(`${artifact}.sig`, "utf8")));
  decodeTauriPublicKey(publicKey);
  signatures.forEach(decodeTauriSignature);
  await verifyUpdaterSignatures({ assetsDirectory: assets, publicKey });

  await Promise.all([
    writeFile(`${appImage}.sig`, signatures[1]),
    writeFile(`${deb}.sig`, signatures[0]),
  ]);
  await assert.rejects(
    verifyUpdaterSignatures({ assetsDirectory: assets, publicKey }),
    /minisign rejected/,
  );

  await Promise.all([
    writeFile(`${appImage}.sig`, signatures[0]),
    writeFile(`${deb}.sig`, signatures[1]),
    writeFile(rpm, "rpm integration artifact altered"),
  ]);
  await assert.rejects(
    verifyUpdaterSignatures({ assetsDirectory: assets, publicKey }),
    /minisign rejected .*\.rpm/,
  );
});
