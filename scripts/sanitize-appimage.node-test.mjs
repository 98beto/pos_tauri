import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmod,
  lstat,
  mkdtemp,
  mkdir,
  readFile,
  readdir,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./sanitize-appimage.sh", import.meta.url));
const forbidden = ["client", "cursor", "egl", "server"];

async function executable(file, contents) {
  await writeFile(file, contents, { mode: 0o755 });
  await chmod(file, 0o755);
}

async function fixture(t, { client = true, discover = false } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos appimage sanitize "));
  t.after(() => rm(root, { recursive: true, force: true }));
  const source = path.join(root, "source contents");
  const lib = path.join(source, "usr", "lib");
  const store = path.join(root, "rebuilt contents");
  const pluginRoot = path.join(root, "plugin search root");
  await mkdir(lib, { recursive: true });
  await mkdir(pluginRoot);
  await writeFile(path.join(source, "AppRun"), "Linea POS must not run\n");
  for (const family of forbidden) {
    if (family !== "client" || client) {
      await writeFile(path.join(lib, `libwayland-${family}.so.0`), family);
      await symlink(`libwayland-${family}.so.0`, path.join(lib, `libwayland-${family}.so`));
    }
  }
  await writeFile(path.join(root, "keep.deb"), "deb");
  await writeFile(path.join(root, "keep.rpm"), "rpm");

  const appimage = path.join(root, "artifacts with spaces", "Linea POS.AppImage");
  await mkdir(path.dirname(appimage));
  await executable(appimage, `#!/usr/bin/env bash
set -eu
[[ \${1-} == --appimage-extract ]] || { echo "application started" >&2; exit 90; }
mkdir squashfs-root
cp -a -- "$FAKE_APPIMAGE_SOURCE/." squashfs-root/
`);
  await chmod(appimage, 0o751);

  const plugin = path.join(pluginRoot, "linuxdeploy-plugin-appimage.AppImage");
  await executable(plugin, `#!/usr/bin/env bash
set -eu
[[ \${1-} == --appimage-extract-and-run && \${2-} == --appdir && -d \${3-} ]]
[[ -n \${LDAI_OUTPUT-} ]]
[[ \${ARCH-} == x86_64 ]]
[[ -z \${LDAI_SIGN-} && -z \${LDAI_SIGN_KEY-} && -z \${SIGN-} ]]
[[ \${LDAI_OUTPUT##*/} == repacked.AppImage ]]
[[ \$(stat -c '%a' -- "\$(dirname -- "$LDAI_OUTPUT")") == 700 ]]
rm -rf -- "$FAKE_REBUILT_STORE"
mkdir -p -- "$FAKE_REBUILT_STORE"
cp -a -- "\$3/." "$FAKE_REBUILT_STORE/"
if [[ \${FAKE_REINTRODUCE-} == 1 ]]; then
  touch "$FAKE_REBUILT_STORE/usr/lib/libwayland-server.so.0"
fi
cat >"$LDAI_OUTPUT" <<'IMAGE'
#!/usr/bin/env bash
set -eu
[[ \${1-} == --appimage-extract ]] || { echo "application started" >&2; exit 91; }
mkdir squashfs-root
cp -a -- "$FAKE_REBUILT_STORE/." squashfs-root/
IMAGE
chmod 755 "$LDAI_OUTPUT"
`);

  return { root, source, store, pluginRoot, plugin: discover ? undefined : plugin, appimage };
}

function run(f, args = [f.appimage], extraEnv = {}) {
  const env = {
    ...process.env,
    FAKE_APPIMAGE_SOURCE: f.source,
    FAKE_REBUILT_STORE: f.store,
    SANITIZE_APPIMAGE_PLUGIN_ROOT: f.pluginRoot,
    LDAI_SIGN: "must be removed",
    LDAI_SIGN_KEY: "must be removed",
    SIGN: "must be removed",
    ...extraEnv,
  };
  if (f.plugin) env.SANITIZE_APPIMAGE_PLUGIN = f.plugin;
  return spawnSync("bash", [script, ...args], { encoding: "utf8", env });
}

test("sanitizes all four families with spaces, preserves mode, and leaves deb/rpm alone", async (t) => {
  const f = await fixture(t);
  const result = run(f);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Sanitized AppImage:/);
  assert.equal((await lstat(f.appimage)).mode & 0o777, 0o751);
  assert.equal(await readFile(path.join(f.root, "keep.deb"), "utf8"), "deb");
  assert.equal(await readFile(path.join(f.root, "keep.rpm"), "utf8"), "rpm");
  const libs = await readdir(path.join(f.store, "usr", "lib"));
  assert.deepEqual(libs, []);
});

test("discovers exactly one plugin", async (t) => {
  const f = await fixture(t, { discover: true });
  const result = run(f);
  assert.equal(result.status, 0, result.stderr);
});

test("fails closed when plugin discovery find fails", async (t) => {
  const f = await fixture(t, { discover: true });
  const bin = path.join(f.root, "failing find bin");
  await mkdir(bin);
  await executable(path.join(bin, "find"), "#!/bin/sh\nexit 42\n");
  const result = run(f, undefined, { PATH: `${bin}:${process.env.PATH}` });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /plugin discovery failed/);
});

test("fails closed when AppImage inspection find fails", async (t) => {
  const f = await fixture(t);
  const before = await readFile(f.appimage, "utf8");
  const bin = path.join(f.root, "failing inspection bin");
  await mkdir(bin);
  await executable(path.join(bin, "find"), "#!/bin/sh\nexit 42\n");
  const result = run(f, undefined, { PATH: `${bin}:${process.env.PATH}` });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /failed to inspect original AppImage/);
  assert.equal(await readFile(f.appimage, "utf8"), before);
});

test("forces the repacker architecture to x86_64", async (t) => {
  const f = await fixture(t);
  const result = run(f, undefined, { ARCH: "aarch64" });
  assert.equal(result.status, 0, result.stderr);
});

test("rejects zero or multiple discovered plugins", async (t) => {
  const none = await fixture(t, { discover: true });
  await rm(path.join(none.pluginRoot, "linuxdeploy-plugin-appimage.AppImage"));
  const noPlugin = run(none);
  assert.notEqual(noPlugin.status, 0);
  assert.match(noPlugin.stderr, /found 0/);

  const multiple = await fixture(t, { discover: true });
  await executable(path.join(multiple.pluginRoot, "linuxdeploy-plugin-appimage-x86_64.AppImage"), "#!/bin/sh\nexit 1\n");
  const twoPlugins = run(multiple);
  assert.notEqual(twoPlugins.status, 0);
  assert.match(twoPlugins.stderr, /found 2/);
});

test("rejects an original without libwayland-client and preserves it", async (t) => {
  const f = await fixture(t, { client: false });
  const before = await readFile(f.appimage, "utf8");
  const result = run(f);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /contains no libwayland-client/);
  assert.equal(await readFile(f.appimage, "utf8"), before);
});

test("rejects a rebuilt image containing a forbidden library", async (t) => {
  const f = await fixture(t);
  const before = await readFile(f.appimage, "utf8");
  const result = run(f, undefined, { FAKE_REINTRODUCE: "1" });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /still contains forbidden library/);
  assert.equal(await readFile(f.appimage, "utf8"), before);
});

test("requires exactly one regular non-symlink AppImage", async (t) => {
  const f = await fixture(t);
  const noArguments = run(f, []);
  assert.notEqual(noArguments.status, 0);
  assert.match(noArguments.stderr, /exactly one AppImage argument/);
  const twoArguments = run(f, [f.appimage, f.appimage]);
  assert.notEqual(twoArguments.status, 0);
  assert.match(twoArguments.stderr, /exactly one AppImage argument/);
  const link = path.join(f.root, "linked.AppImage");
  await symlink(f.appimage, link);
  const symlinkInput = run(f, [link]);
  assert.notEqual(symlinkInput.status, 0);
  assert.match(symlinkInput.stderr, /regular file, not a symlink/);
});

test("cleans its temporary directory after failure", async (t) => {
  const f = await fixture(t);
  const temporary = path.join(f.root, "temporary files");
  await mkdir(temporary);
  const result = run(f, undefined, { FAKE_REINTRODUCE: "1", TMPDIR: temporary });
  assert.notEqual(result.status, 0);
  assert.deepEqual(await readdir(temporary), []);
  assert.deepEqual((await readdir(path.dirname(f.appimage))).sort(), [path.basename(f.appimage)]);
});
