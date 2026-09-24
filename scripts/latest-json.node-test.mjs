import assert from "node:assert/strict";
import { cp, mkdtemp, mkdir, rename, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  generateLatestJson,
  inspectReleaseAssets,
  validateLatestJson,
} from "./latest-json.mjs";

function signature(seed) {
  const packet = Buffer.concat([Buffer.from("ED"), Buffer.alloc(72, seed)]).toString("base64");
  const global = Buffer.alloc(64, seed).toString("base64");
  const file = `untrusted comment: signature from tauri secret key\n${packet}\ntrusted comment: timestamp:1\n${global}\n`;
  return Buffer.from(file).toString("base64");
}

const linuxSignature = signature(1);
const windowsSignature = signature(2);

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos-latest-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const files = {
    "Linea.POS_1.2.3_amd64.AppImage": "appimage",
    "Linea.POS_1.2.3_amd64.AppImage.sig": linuxSignature,
    "Linea.POS_1.2.3_amd64.deb": "deb",
    "Linea.POS-1.2.3-1.x86_64.rpm": "rpm",
    "Linea.POS_1.2.3_x64-setup.exe": "nsis",
    "Linea.POS_1.2.3_x64-setup.exe.sig": windowsSignature,
    "Linea.POS_1.2.3_x64_en-US.msi": "msi",
  };
  await Promise.all(Object.entries(files).map(([name, contents]) => (
    writeFile(path.join(root, name), contents)
  )));
  return root;
}

test("generates one native updater entry per supported platform", async (t) => {
  const root = await fixture(t);
  const manifest = await generateLatestJson({
    tag: "v1.2.3",
    assetsDirectory: root,
    notes: "Release notes",
    pubDate: "2026-09-23T10:00:00Z",
  });

  assert.equal(manifest.version, "1.2.3");
  assert.deepEqual(Object.keys(manifest.platforms), ["linux-x86_64", "windows-x86_64"]);
  assert.equal(
    manifest.platforms["linux-x86_64"].url,
    "https://github.com/98beto/pos_tauri/releases/download/v1.2.3/Linea.POS_1.2.3_amd64.AppImage",
  );
  assert.equal(
    manifest.platforms["windows-x86_64"].url,
    "https://github.com/98beto/pos_tauri/releases/download/v1.2.3/Linea.POS_1.2.3_x64-setup.exe",
  );
  assert.equal(manifest.platforms["linux-x86_64"].signature, linuxSignature);
  assert.equal(manifest.platforms["windows-x86_64"].signature, windowsSignature);
});

test("uses a safe fallback when release notes are empty", async (t) => {
  const root = await fixture(t);
  const manifest = await generateLatestJson({
    tag: "v1.2.3",
    assetsDirectory: root,
    notes: "  ",
    pubDate: "2026-09-23T10:00:00Z",
  });

  assert.equal(manifest.notes, "Release v1.2.3");
});

test("rejects an MSI URL as the Windows updater", async (t) => {
  const root = await fixture(t);
  const manifest = await generateLatestJson({
    tag: "v1.2.3",
    assetsDirectory: root,
    pubDate: "2026-09-23T10:00:00Z",
  });
  manifest.platforms["windows-x86_64"].url =
    "https://github.com/98beto/pos_tauri/releases/download/v1.2.3/app.msi";

  await assert.rejects(
    validateLatestJson({ tag: "v1.2.3", assetsDirectory: root, manifest }),
    /expected release asset|MSI/,
  );
});

test("rejects missing and empty signatures", async (t) => {
  const root = await fixture(t);
  await writeFile(path.join(root, "Linea.POS_1.2.3_amd64.AppImage.sig"), "");

  await assert.rejects(inspectReleaseAssets(root), /canonical Base64/);
});

test("rejects a missing required asset", async (t) => {
  const root = await fixture(t);
  await rm(path.join(root, "Linea.POS_1.2.3_x64_en-US.msi"));

  await assert.rejects(inspectReleaseAssets(root), /exactly 7 release assets, found 6/);
});

test("rejects an unexpected extra asset", async (t) => {
  const root = await fixture(t);
  await writeFile(path.join(root, "checksums.txt"), "extra");

  await assert.rejects(inspectReleaseAssets(root), /exactly 7 release assets, found 8/);
});

test("rejects asset filenames outside the stable GitHub-safe set", async (t) => {
  const root = await fixture(t);
  await Promise.all([
    cp(
      path.join(root, "Linea.POS_1.2.3_amd64.AppImage"),
      path.join(root, "Linea.POS's_1.2.3_amd64.AppImage"),
    ),
    cp(
      path.join(root, "Linea.POS_1.2.3_amd64.AppImage.sig"),
      path.join(root, "Linea.POS's_1.2.3_amd64.AppImage.sig"),
    ),
  ]);
  await Promise.all([
    rm(path.join(root, "Linea.POS_1.2.3_amd64.AppImage")),
    rm(path.join(root, "Linea.POS_1.2.3_amd64.AppImage.sig")),
  ]);

  await assert.rejects(inspectReleaseAssets(root), /Unsafe release asset filename.*must start and end/);
});

test("rejects asset filenames with a leading period", async (t) => {
  const root = await fixture(t);
  await rename(
    path.join(root, "Linea.POS_1.2.3_amd64.deb"),
    path.join(root, ".Linea.POS_1.2.3_amd64.deb"),
  );

  await assert.rejects(inspectReleaseAssets(root), /Unsafe release asset filename/);
});

test("rejects asset filenames with a trailing period", async (t) => {
  const root = await fixture(t);
  await rename(
    path.join(root, "Linea.POS_1.2.3_amd64.deb"),
    path.join(root, "Linea.POS_1.2.3_amd64.deb."),
  );

  await assert.rejects(inspectReleaseAssets(root), /Unsafe release asset filename/);
});

test("rejects legacy updater archives", async (t) => {
  const root = await fixture(t);
  await writeFile(path.join(root, "legacy.AppImage.tar.gz"), "legacy");

  await assert.rejects(inspectReleaseAssets(root), /Legacy updater archives/);
});

test("rejects duplicate filenames downloaded from separate artifacts", async (t) => {
  const root = await fixture(t);
  await mkdir(path.join(root, "duplicate"));
  await cp(
    path.join(root, "Linea.POS_1.2.3_amd64.deb"),
    path.join(root, "duplicate", "Linea.POS_1.2.3_amd64.deb"),
  );

  await assert.rejects(inspectReleaseAssets(root), /filenames must be unique/);
});
