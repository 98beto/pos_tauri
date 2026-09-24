import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  createReleaseConfig,
  UPDATER_ENDPOINT,
  writeReleaseConfig,
} from "./release-config.mjs";

const keyLine = Buffer.concat([Buffer.from("Ed"), Buffer.alloc(40, 3)]).toString("base64");
const publicKeyFile = `untrusted comment: minisign public key: 0303030303030303\n${keyLine}\n`;
const publicKey = Buffer.from(publicKeyFile).toString("base64");

test("keeps updater artifacts disabled in the base config", async () => {
  const base = JSON.parse(await readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(base.bundle.createUpdaterArtifacts, false);
});

test("uses the exact public latest.json endpoint", () => {
  assert.equal(
    UPDATER_ENDPOINT,
    "https://github.com/98beto/pos_tauri/releases/latest/download/latest.json",
  );
});

test("creates an updater overlay without changing the base config", () => {
  assert.deepEqual(createReleaseConfig(publicKey), {
    build: { beforeBuildCommand: "" },
    bundle: { createUpdaterArtifacts: true },
    plugins: {
      updater: {
        endpoints: [UPDATER_ENDPOINT],
        pubkey: publicKey,
      },
    },
  });
});

test("can disable updater artifacts for the manual MSI build", () => {
  const config = createReleaseConfig(publicKey, false);
  assert.equal(config.bundle.createUpdaterArtifacts, false);
  assert.equal(config.plugins.updater.pubkey, publicKey);
});

test("disables the Tauri frontend build hook after the explicit unprivileged build", () => {
  assert.equal(createReleaseConfig(publicKey).build.beforeBuildCommand, "");
});

test("rejects a missing public key", () => {
  assert.throws(() => createReleaseConfig("  "), /repository variable is required/);
});

test("writes the ephemeral JSON overlay", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos-release-config-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const output = path.join(root, "nested", "release.json");

  await writeReleaseConfig(output, publicKey);
  const contents = JSON.parse(await readFile(output, "utf8"));

  assert.equal(contents.plugins.updater.endpoints[0], UPDATER_ENDPOINT);
  assert.equal(contents.plugins.updater.pubkey, publicKey);
});

test("rejects non-canonical public key variants", () => {
  assert.throws(() => createReleaseConfig(keyLine), /canonical minisign public key file|canonical Base64/);
  assert.throws(() => createReleaseConfig(publicKeyFile), /canonical Base64/);
  assert.throws(() => createReleaseConfig(`${publicKey}\n`), /canonical Base64/);
});
