import { lstat, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import os from "node:os";
import path from "node:path";

import { inspectReleaseAssets } from "./latest-json.mjs";
import { decodeTauriPublicKey, decodeTauriSignature } from "./tauri-signature-format.mjs";

export { decodeTauriPublicKey, decodeTauriSignature } from "./tauri-signature-format.mjs";

async function requireNonEmptyRegularFile(file, description) {
  const stats = await lstat(file);
  if (stats.isSymbolicLink() || !stats.isFile() || stats.size === 0) {
    throw new Error(`${description} must be a non-empty regular file, not a symlink`);
  }
}

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

export async function verifyArtifactSignature({
  artifact,
  signature,
  publicKey,
  minisign = process.env.MINISIGN_BIN || "minisign",
}) {
  await Promise.all([
    requireNonEmptyRegularFile(artifact, "Artifact"),
    requireNonEmptyRegularFile(signature, "Signature"),
  ]);
  const temporaryDirectory = await mkdtemp(path.join(os.tmpdir(), "pos-minisign-"));
  const publicKeyFile = path.join(temporaryDirectory, "updater.pub");
  const signatureFile = path.join(temporaryDirectory, "artifact.sig");

  try {
    await Promise.all([
      writeFile(publicKeyFile, decodeTauriPublicKey(publicKey), { mode: 0o600 }),
      writeFile(signatureFile, decodeTauriSignature(await readFile(signature, "utf8")), { mode: 0o600 }),
    ]);
    await runMinisign(minisign, artifact, signatureFile, publicKeyFile);
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}

export async function verifyUpdaterSignatures({
  assetsDirectory,
  tag,
  publicKey,
  minisign = process.env.MINISIGN_BIN || "minisign",
}) {
  if (!tag) {
    const names = await readdir(assetsDirectory);
    const versions = names.flatMap((name) => {
      const match = /^Linea\.POS_(\d+\.\d+\.\d+)_amd64\.AppImage$/.exec(name);
      return match ? [match[1]] : [];
    });
    if (versions.length !== 1) throw new Error("Unable to infer one release tag from AppImage basename");
    tag = `v${versions[0]}`;
  }
  const assets = await inspectReleaseAssets(assetsDirectory, tag);
  const temporaryDirectory = await mkdtemp(path.join(os.tmpdir(), "pos-minisign-"));
  const publicKeyFile = path.join(temporaryDirectory, "updater.pub");

  try {
    await writeFile(publicKeyFile, decodeTauriPublicKey(publicKey), { mode: 0o600 });
    for (const [platform, updater] of Object.entries(assets.updaters)) {
      const signatureFile = path.join(temporaryDirectory, `${platform}.sig`);
      await writeFile(signatureFile, decodeTauriSignature(updater.signature), { mode: 0o600 });
      await runMinisign(minisign, updater.artifact, signatureFile, publicKeyFile);
    }
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}

function optionsFromArguments(args) {
  const allowed = new Set(["assets", "artifact", "signature", "tag", "minisign"]);
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    if (!args[index]?.startsWith("--") || args[index + 1] === undefined) {
      throw new Error(`Invalid argument ${args[index] ?? ""}`);
    }
    const name = args[index].slice(2);
    if (!allowed.has(name)) throw new Error(`Unknown option --${name}`);
    if (Object.hasOwn(options, name)) throw new Error(`Duplicate option --${name}`);
    options[name] = args[index + 1];
  }
  return options;
}

const isMain = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  try {
    const options = optionsFromArguments(process.argv.slice(2));
    if (options.assets && options.tag && !options.artifact && !options.signature) {
      await verifyUpdaterSignatures({
        assetsDirectory: options.assets,
        tag: options.tag,
        publicKey: process.env.TAURI_UPDATER_PUBLIC_KEY,
        minisign: options.minisign,
      });
      console.log("Updater signatures verified.");
    } else if (!options.assets && !options.tag && options.artifact && options.signature) {
      await verifyArtifactSignature({
        artifact: options.artifact,
        signature: options.signature,
        publicKey: process.env.TAURI_UPDATER_PUBLIC_KEY,
        minisign: options.minisign,
      });
      console.log("Artifact signature verified.");
    } else {
      throw new Error("Use --assets with --tag, or use --artifact and --signature together");
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
