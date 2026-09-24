import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { validateVersion } from "./validate-version.mjs";

const scriptPath = fileURLToPath(new URL("./validate-version.mjs", import.meta.url));

async function fixture(t, versions = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos-version-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(path.join(root, "src-tauri"));
  await writeFile(
    path.join(root, "package.json"),
    JSON.stringify({ version: versions.package ?? "1.2.3" }),
  );
  await writeFile(
    path.join(root, "src-tauri", "Cargo.toml"),
    `[package]\nname = "fixture"\nversion = "${versions.cargo ?? "1.2.3"}"\n\n[dependencies]\n`,
  );
  await writeFile(
    path.join(root, "src-tauri", "tauri.conf.json"),
    JSON.stringify({ version: versions.tauri ?? "1.2.3" }),
  );
  return root;
}

test("accepts a valid tag passed as an argument", async (t) => {
  const root = await fixture(t);
  const result = spawnSync(process.execPath, [scriptPath, "v1.2.3"], {
    cwd: root,
    encoding: "utf8",
  });

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Release version 1\.2\.3 is consistent/);
});

test("accepts GITHUB_REF_NAME when no argument is passed", async (t) => {
  const root = await fixture(t);
  const result = spawnSync(process.execPath, [scriptPath], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, GITHUB_REF_NAME: "v1.2.3" },
  });

  assert.equal(result.status, 0, result.stderr);
});

test("the CLI exits unsuccessfully for an invalid tag", async (t) => {
  const root = await fixture(t);
  const result = spawnSync(process.execPath, [scriptPath, "1.2.3"], {
    cwd: root,
    encoding: "utf8",
  });

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /expected vX\.Y\.Z/);
});

for (const tag of [undefined, "1.2.3", "v1.2", "v01.2.3", "v1.2.3-beta"]) {
  test(`rejects invalid tag ${JSON.stringify(tag)}`, async (t) => {
    const root = await fixture(t);

    await assert.rejects(validateVersion(tag, root), /expected vX\.Y\.Z/);
  });
}

for (const source of ["package", "cargo", "tauri"]) {
  test(`rejects a mismatch in ${source}`, async (t) => {
    const root = await fixture(t, { [source]: "1.2.4" });

    await assert.rejects(validateVersion("v1.2.3", root), /requires version 1\.2\.3/);
  });
}
