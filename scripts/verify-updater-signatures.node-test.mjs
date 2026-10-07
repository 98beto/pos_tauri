import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { promisify } from "node:util";

import {
  decodeTauriPublicKey,
  decodeTauriSignature,
  verifyArtifactSignature,
  verifyUpdaterSignatures,
} from "./verify-updater-signatures.mjs";

const keyBytes = Buffer.concat([Buffer.from("Ed"), Buffer.alloc(40, 7)]);
const keyLine = keyBytes.toString("base64");
const keyFile = `untrusted comment: minisign public key: 0707070707070707\n${keyLine}\n`;
const publicKey = Buffer.from(keyFile).toString("base64");
const execFileAsync = promisify(execFile);
const entrypoint = new URL("./verify-updater-signatures.mjs", import.meta.url);

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
    "Linea.POS_1.2.3_amd64.AppImage": "appimage",
    "Linea.POS_1.2.3_amd64.AppImage.sig": signature(1),
    "Linea.POS_1.2.3_amd64.deb": "deb",
    "Linea.POS_1.2.3_amd64.deb.sig": signature(2),
    "Linea.POS-1.2.3-1.x86_64.rpm": "rpm",
    "Linea.POS-1.2.3-1.x86_64.rpm.sig": signature(3),
    "Linea.POS_1.2.3_x64-setup.exe": "nsis",
    "Linea.POS_1.2.3_x64-setup.exe.sig": signature(4),
    "Linea.POS_1.2.3_x64_en-US.msi": "msi",
  };
  await Promise.all(Object.entries(files).map(([name, contents]) => (
    writeFile(path.join(assets, name), contents)
  )));
  const verifier = path.join(root, "minisign-test-double.mjs");
  const calls = path.join(root, "calls.jsonl");
  await writeFile(verifier, `#!/usr/bin/env node
import { appendFileSync, readFileSync } from "node:fs";
const args = process.argv.slice(2);
const artifact = args[args.indexOf("-Vm") + 1];
const signature = args[args.indexOf("-x") + 1];
const publicKey = args[args.indexOf("-p") + 1];
if (!args.includes("-q") || readFileSync(publicKey, "utf8") !== ${JSON.stringify(keyFile)}) process.exit(2);
appendFileSync(${JSON.stringify(calls)}, JSON.stringify({ artifact, signature, publicKey }) + "\\n");
const artifactContents = readFileSync(artifact, "utf8");
const signaturePacket = Buffer.from(readFileSync(signature, "utf8").split("\\n")[1] ?? "", "base64");
const expectedSignatureSeeds = { appimage: 1, deb: 2, rpm: 3, nsis: 4 };
if (expectedSignatureSeeds[artifactContents] !== signaturePacket[2]) process.exit(1);
`, { mode: 0o700 });
  return { assets, calls, verifier };
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

test("verifies all four native updater artifacts and cleans temporary files", async (t) => {
  const { assets, calls, verifier } = await fixture(t);
  await verifyUpdaterSignatures({ assetsDirectory: assets, tag: "v1.2.3", publicKey, minisign: verifier });
  const invocations = (await readFile(calls, "utf8")).trim().split("\n").map(JSON.parse);
  assert.deepEqual(
    invocations.map(({ artifact }) => path.basename(artifact)),
    [
      "Linea.POS_1.2.3_amd64.AppImage",
      "Linea.POS_1.2.3_amd64.deb",
      "Linea.POS-1.2.3-1.x86_64.rpm",
      "Linea.POS_1.2.3_x64-setup.exe",
    ],
  );
  for (const invocation of invocations) {
    await assert.rejects(readFile(invocation.signature), { code: "ENOENT" });
    await assert.rejects(readFile(invocation.publicKey), { code: "ENOENT" });
  }
});

test("fails when the signature fixture does not match the artifact content", async (t) => {
  const { assets, calls, verifier } = await fixture(t);
  const badSignature = signature(1);
  await writeFile(path.join(assets, "Linea.POS_1.2.3_x64-setup.exe.sig"), badSignature);
  await assert.rejects(
    verifyUpdaterSignatures({ assetsDirectory: assets, tag: "v1.2.3", publicKey, minisign: verifier }),
    /minisign rejected .*setup\.exe/,
  );
  const invocations = (await readFile(calls, "utf8")).trim().split("\n").map(JSON.parse);
  await assert.rejects(readFile(invocations.at(-1).signature), { code: "ENOENT" });
});

test("verifies one explicit artifact and Tauri signature pair", async (t) => {
  const { assets, calls, verifier } = await fixture(t);
  const artifact = path.join(assets, "Linea.POS_1.2.3_amd64.deb");
  await verifyArtifactSignature({
    artifact,
    signature: `${artifact}.sig`,
    publicKey,
    minisign: verifier,
  });
  const [invocation] = (await readFile(calls, "utf8")).trim().split("\n").map(JSON.parse);
  assert.equal(invocation.artifact, artifact);
  await assert.rejects(readFile(invocation.signature), { code: "ENOENT" });
  await assert.rejects(readFile(invocation.publicKey), { code: "ENOENT" });
});

test("rejects empty, non-regular, and symlink artifact or signature inputs", async (t) => {
  const { assets, verifier } = await fixture(t);
  const artifact = path.join(assets, "Linea.POS_1.2.3_amd64.deb");
  const signatureFile = `${artifact}.sig`;
  const empty = path.join(assets, "empty");
  const directory = path.join(assets, "directory");
  const link = path.join(assets, "link");
  await writeFile(empty, "");
  await mkdir(directory);
  await symlink(artifact, link);

  for (const [badArtifact, badSignature] of [
    [empty, signatureFile],
    [directory, signatureFile],
    [link, signatureFile],
    [artifact, empty],
    [artifact, directory],
    [artifact, link],
  ]) {
    await assert.rejects(
      verifyArtifactSignature({
        artifact: badArtifact,
        signature: badSignature,
        publicKey,
        minisign: verifier,
      }),
      /must be a non-empty regular file, not a symlink/,
    );
  }
});

test("entrypoint accepts only the documented modes and options", async (t) => {
  const { assets, verifier } = await fixture(t);
  const artifact = path.join(assets, "Linea.POS_1.2.3_amd64.deb");
  const environment = {
    ...process.env,
    TAURI_UPDATER_PUBLIC_KEY: publicKey,
  };
  await execFileAsync(process.execPath, [
    entrypoint.pathname,
    "--artifact", artifact,
    "--signature", `${artifact}.sig`,
    "--minisign", verifier,
  ], { env: environment });
  await execFileAsync(process.execPath, [
    entrypoint.pathname,
    "--assets", assets,
    "--tag", "v1.2.3",
    "--minisign", verifier,
  ], { env: environment });

  for (const args of [
    ["--unknown", "value"],
    ["--artifact", artifact, "--artifact", artifact, "--signature", `${artifact}.sig`],
    ["--artifact", artifact],
    ["--signature", `${artifact}.sig`],
    ["--assets", assets],
    ["--assets", assets, "--tag", "v1.2.3", "--artifact", artifact, "--signature", `${artifact}.sig`],
    ["--artifact", artifact, "--signature", `${artifact}.sig`, "--tag", "v1.2.3"],
  ]) {
    await assert.rejects(
      execFileAsync(process.execPath, [entrypoint.pathname, ...args], { env: environment }),
    );
  }
});
