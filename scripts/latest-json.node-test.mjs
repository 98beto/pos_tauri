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

const signatures = {
  appimage: signature(1),
  deb: signature(2),
  rpm: signature(3),
  nsis: signature(4),
};

const releaseAssets = {
  "Linea.POS_1.2.3_amd64.AppImage": "appimage",
  "Linea.POS_1.2.3_amd64.AppImage.sig": signatures.appimage,
  "Linea.POS_1.2.3_amd64.deb": "deb",
  "Linea.POS_1.2.3_amd64.deb.sig": signatures.deb,
  "Linea.POS-1.2.3-1.x86_64.rpm": "rpm",
  "Linea.POS-1.2.3-1.x86_64.rpm.sig": signatures.rpm,
  "Linea.POS_1.2.3_x64-setup.exe": "nsis",
  "Linea.POS_1.2.3_x64-setup.exe.sig": signatures.nsis,
  "Linea.POS_1.2.3_x64_en-US.msi": "msi",
};

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos-latest-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await Promise.all(Object.entries(releaseAssets).map(([name, contents]) => (
    writeFile(path.join(root, name), contents)
  )));
  return root;
}

test("generates one correctly paired entry per native updater format", async (t) => {
  const root = await fixture(t);
  const manifest = await generateLatestJson({
    tag: "v1.2.3",
    assetsDirectory: root,
    notes: "Release notes",
    pubDate: "2026-09-23T10:00:00Z",
  });

  assert.equal(manifest.version, "1.2.3");
  const expected = {
    "linux-x86_64-appimage": ["Linea.POS_1.2.3_amd64.AppImage", signatures.appimage],
    "linux-x86_64-deb": ["Linea.POS_1.2.3_amd64.deb", signatures.deb],
    "linux-x86_64-rpm": ["Linea.POS-1.2.3-1.x86_64.rpm", signatures.rpm],
    "windows-x86_64-nsis": ["Linea.POS_1.2.3_x64-setup.exe", signatures.nsis],
  };
  assert.deepEqual(Object.keys(manifest.platforms), Object.keys(expected));
  for (const [platform, [name, expectedSignature]] of Object.entries(expected)) {
    assert.equal(
      manifest.platforms[platform].url,
      `https://github.com/98beto/pos_tauri/releases/download/v1.2.3/${name}`,
    );
    assert.equal(manifest.platforms[platform].signature, expectedSignature);
  }
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
  manifest.platforms["windows-x86_64-nsis"].url =
    "https://github.com/98beto/pos_tauri/releases/download/v1.2.3/app.msi";

  await assert.rejects(
    validateLatestJson({ tag: "v1.2.3", assetsDirectory: root, manifest }),
    /expected release asset|MSI/,
  );
});

test("rejects missing and empty signatures", async (t) => {
  const root = await fixture(t);
  await writeFile(path.join(root, "Linea.POS_1.2.3_amd64.AppImage.sig"), "");

  await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /canonical Base64/);
});

test("rejects generic, missing, and extra platform keys", async (t) => {
  const root = await fixture(t);
  const original = await generateLatestJson({
    tag: "v1.2.3",
    assetsDirectory: root,
    pubDate: "2026-09-23T10:00:00Z",
  });
  for (const platforms of [
    { ...original.platforms, "linux-x86_64": original.platforms["linux-x86_64-appimage"] },
    Object.fromEntries(Object.entries(original.platforms).slice(1)),
    { ...original.platforms, unexpected: original.platforms["linux-x86_64-appimage"] },
  ]) {
    await assert.rejects(
      validateLatestJson({ tag: "v1.2.3", assetsDirectory: root, manifest: { ...original, platforms } }),
      /must contain only/,
    );
  }
});

test("rejects a URL or signature assigned to another nominal pair", async (t) => {
  const root = await fixture(t);
  const manifest = await generateLatestJson({
    tag: "v1.2.3",
    assetsDirectory: root,
    pubDate: "2026-09-23T10:00:00Z",
  });
  const appImage = manifest.platforms["linux-x86_64-appimage"];
  const deb = manifest.platforms["linux-x86_64-deb"];
  [appImage.signature, deb.signature] = [deb.signature, appImage.signature];
  await assert.rejects(
    validateLatestJson({ tag: "v1.2.3", assetsDirectory: root, manifest }),
    /signature does not match/,
  );
  [appImage.signature, deb.signature] = [deb.signature, appImage.signature];
  [appImage.url, deb.url] = [deb.url, appImage.url];
  await assert.rejects(
    validateLatestJson({ tag: "v1.2.3", assetsDirectory: root, manifest }),
    /URL does not reference/,
  );
});

test("rejects each individually missing required asset", async (t) => {
  for (const name of Object.keys(releaseAssets)) {
    await t.test(name, async (t) => {
      const root = await fixture(t);
      await rm(path.join(root, name));

      await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /exactly 9 release assets, found 8/);
    });
  }
});

test("rejects basenames with the wrong product, version, architecture, or locale", async (t) => {
  const cases = [
    ["product", "Linea.POS_1.2.3_amd64.AppImage", "Other.POS_1.2.3_amd64.AppImage"],
    ["version", "Linea.POS_1.2.3_amd64.deb", "Linea.POS_1.2.4_amd64.deb"],
    ["architecture", "Linea.POS-1.2.3-1.x86_64.rpm", "Linea.POS-1.2.3-1.aarch64.rpm"],
    ["NSIS architecture", "Linea.POS_1.2.3_x64-setup.exe", "Linea.POS_1.2.3_arm64-setup.exe"],
    ["locale", "Linea.POS_1.2.3_x64_en-US.msi", "Linea.POS_1.2.3_x64_es-ES.msi"],
  ];

  for (const [description, source, replacement] of cases) {
    await t.test(description, async (t) => {
      const root = await fixture(t);
      await rename(path.join(root, source), path.join(root, replacement));

      await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /basenames must exactly match tag v1\.2\.3/);
    });
  }
});

test("rejects an unexpected extra asset", async (t) => {
  const root = await fixture(t);
  await writeFile(path.join(root, "checksums.txt"), "extra");

  await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /exactly 9 release assets, found 10/);
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

  await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /Unsafe release asset filename.*must start and end/);
});

test("rejects asset filenames with a leading period", async (t) => {
  const root = await fixture(t);
  await rename(
    path.join(root, "Linea.POS_1.2.3_amd64.deb"),
    path.join(root, ".Linea.POS_1.2.3_amd64.deb"),
  );

  await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /Unsafe release asset filename/);
});

test("rejects asset filenames with a trailing period", async (t) => {
  const root = await fixture(t);
  await rename(
    path.join(root, "Linea.POS_1.2.3_amd64.deb"),
    path.join(root, "Linea.POS_1.2.3_amd64.deb."),
  );

  await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /Unsafe release asset filename/);
});

test("rejects legacy updater archives", async (t) => {
  const root = await fixture(t);
  await writeFile(path.join(root, "legacy.AppImage.tar.gz"), "legacy");

  await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /Legacy updater archives/);
});

test("rejects duplicate filenames downloaded from separate artifacts", async (t) => {
  const root = await fixture(t);
  await mkdir(path.join(root, "duplicate"));
  await cp(
    path.join(root, "Linea.POS_1.2.3_amd64.deb"),
    path.join(root, "duplicate", "Linea.POS_1.2.3_amd64.deb"),
  );

  await assert.rejects(inspectReleaseAssets(root, "v1.2.3"), /filenames must be unique/);
});
