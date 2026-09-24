import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  decodeTauriPublicKey,
  decodeTauriSignature,
  verifyUpdaterSignatures,
} from "./verify-updater-signatures.mjs";

const keyBytes = Buffer.concat([Buffer.from("Ed"), Buffer.alloc(40, 7)]);
const keyLine = keyBytes.toString("base64");
const keyFile = `untrusted comment: minisign public key: 0707070707070707\n${keyLine}\n`;
const publicKey = Buffer.from(keyFile).toString("base64");

function signature(seed) {
  const packet = Buffer.concat([Buffer.from("ED"), Buffer.alloc(72, seed)]).toString("base64");
  const global = Buffer.alloc(64, seed).toString("base64");
  return Buffer.from(`untrusted comment: signature from tauri secret key\n${packet}\ntrusted comment: timestamp:1\n${global}\n`).toString("base64");
}

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos-signatures-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const assets = path.join(root, "assets");
  await mkdir(assets);
  const files = {
    "Linea POS_1.2.3_amd64.AppImage": "appimage",
    "Linea POS_1.2.3_amd64.AppImage.sig": signature(1),
    "Linea POS_1.2.3_amd64.deb": "deb",
    "Linea POS-1.2.3-1.x86_64.rpm": "rpm",
    "Linea POS_1.2.3_x64-setup.exe": "nsis",
    "Linea POS_1.2.3_x64-setup.exe.sig": signature(2),
    "Linea POS_1.2.3_x64_en-US.msi": "msi",
  };
  await Promise.all(Object.entries(files).map(([name, contents]) => (
    writeFile(path.join(assets, name), contents)
  )));
  const verifier = path.join(root, "minisign-test-double.mjs");
  await writeFile(verifier, `#!/usr/bin/env node
import { readFileSync } from "node:fs";
const args = process.argv.slice(2);
const artifact = args[args.indexOf("-Vm") + 1];
const signature = args[args.indexOf("-x") + 1];
const publicKey = args[args.indexOf("-p") + 1];
if (!args.includes("-q") || readFileSync(publicKey, "utf8") !== ${JSON.stringify(keyFile)}) process.exit(2);
if (readFileSync(signature, "utf8").startsWith("bad") || !readFileSync(artifact).length) process.exit(1);
`, { mode: 0o700 });
  return { assets, verifier };
}

test("decodes only Tauri's canonical public key format", () => {
  assert.deepEqual(decodeTauriPublicKey(publicKey), Buffer.from(keyFile));
  assert.throws(() => decodeTauriPublicKey(keyLine), /canonical minisign public key file/);
  assert.throws(() => decodeTauriPublicKey(keyFile), /canonical Base64/);
});

test("rejects malformed public keys without including their value", () => {
  const value = "definitely-not-a-public-key";
  assert.throws(
    () => decodeTauriPublicKey(value),
    (error) => !error.message.includes(value) && /canonical Base64/.test(error.message),
  );
});

test("decodes only complete canonical Tauri signatures", () => {
  const value = signature(3);
  assert.match(decodeTauriSignature(value).toString("utf8"), /^untrusted comment:/);
  assert.throws(() => decodeTauriSignature(`${value}\n`), /canonical Base64/);
  assert.throws(() => decodeTauriSignature(Buffer.alloc(74).toString("base64")), /canonical minisign/);
});

test("verifies both native updater artifacts", async (t) => {
  const { assets, verifier } = await fixture(t);
  await verifyUpdaterSignatures({ assetsDirectory: assets, publicKey, minisign: verifier });
});

test("fails when minisign reports a signature mismatch", async (t) => {
  const { assets, verifier } = await fixture(t);
  const badSignature = signature(4);
  await writeFile(path.join(assets, "Linea POS_1.2.3_x64-setup.exe.sig"), badSignature);
  await writeFile(path.join(assets, "Linea POS_1.2.3_x64-setup.exe"), "");
  await assert.rejects(
    verifyUpdaterSignatures({ assetsDirectory: assets, publicKey, minisign: verifier }),
    /minisign rejected .*setup\.exe/,
  );
});
