import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { promisify } from "node:util";

const release = await readFile(new URL("../.github/workflows/release.yml", import.meta.url), "utf8");
const releaseRequest = await readFile(new URL("../.github/workflows/release-request.yml", import.meta.url), "utf8");
const ci = await readFile(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
const execFileAsync = promisify(execFile);

const expectedPins = new Map([
  ["actions/checkout", "fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09"],
  ["actions/setup-node", "a0853c24544627f65ddf259abe73b1d18a591444"],
  ["pnpm/action-setup", "fc06bc1257f339d1d5d8b3a19a8cae5388b55320"],
  ["actions/upload-artifact", "043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"],
  ["actions/download-artifact", "37930b1c2abaa49bbe596cd826c3c89aef350131"],
  ["Swatinem/rust-cache", "6323deb102c322ba6fcbdcafc7e3dddab59af2b6"],
  ["tauri-apps/tauri-action", "1deb371b0cd8bd54025b384f1cd735e725c4060f"],
]);

function occurrences(contents, pattern) {
  return [...contents.matchAll(pattern)].length;
}

function jobSection(contents, name) {
  const start = contents.indexOf(`\n  ${name}:\n`);
  assert.notEqual(start, -1, `${name} job must exist`);
  const remaining = contents.slice(start + 1);
  const next = remaining.slice(1).search(/^  [a-z][a-z-]*:$/m);
  return next === -1 ? remaining : remaining.slice(0, next + 1);
}

function stepRunBlock(contents, name) {
  const marker = `      - name: ${name}\n`;
  const start = contents.indexOf(marker);
  assert.notEqual(start, -1, `${name} step must exist`);
  const end = contents.indexOf("\n      - ", start + marker.length);
  const step = contents.slice(start, end === -1 ? undefined : end);
  const runMarker = "        run: |\n";
  const runStart = step.indexOf(runMarker);
  assert.notEqual(runStart, -1, `${name} step must have a literal run block`);
  return step
    .slice(runStart + runMarker.length)
    .split("\n")
    .map((line) => line.startsWith("          ") ? line.slice(10) : line)
    .join("\n");
}

const canonicalizeScript = stepRunBlock(release, "Canonicalize release asset filenames");

async function canonicalizationFixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos-release-workflow-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(path.join(root, "release-assets"));
  return root;
}

async function runCanonicalization(root) {
  return execFileAsync("bash", ["-c", canonicalizeScript], { cwd: root });
}

test("uses least privilege and disables checkout credential persistence", () => {
  assert.match(release, /^permissions:\n  contents: read$/m);
  assert.match(release, /^  publish:[\s\S]*?^    permissions:\n      contents: write$/m);
  assert.equal(occurrences(release, /uses: actions\/checkout@/g), 5);
  assert.equal(occurrences(release, /persist-credentials: false/g), 5);
  assert.equal(occurrences(ci, /uses: actions\/checkout@/g), 2);
  assert.equal(occurrences(ci, /persist-credentials: false/g), 2);
  assert.match(releaseRequest, /^permissions: \{\}$/m);
  assert.doesNotMatch(releaseRequest, /secrets\.|contents: write|actions\/checkout/);
});

test("loads privileged release logic from main after an unprivileged request", () => {
  assert.doesNotMatch(release, /^  push:/m);
  assert.match(release, /^  workflow_run:\n    workflows: \["Release request"\]/m);
  assert.match(release, /github\.event\.workflow_run\.conclusion == 'success'/);
  assert.match(release, /ref: main/);
  assert.match(release, /REQUEST_SHA: \$\{\{ github\.event\.workflow_run\.head_sha \}\}/);
  assert.match(release, /Expected exactly one vX\.Y\.Z tag at \$sha/);
  assert.match(release, /git merge-base --is-ancestor "\$sha" origin\/main/);
  assert.match(release, /DISPATCH_REF.*github\.ref/);
  assert.match(release, /refs\/heads\/main/);
});

test("builds and publishes only the validated immutable commit", () => {
  assert.match(release, /^      sha: \$\{\{ steps\.release-source\.outputs\.sha \}\}$/m);
  assert.equal(occurrences(release, /ref: \$\{\{ needs\.validate\.outputs\.sha \}\}/g), 4);
  assert.equal(occurrences(release, /args: .* -- --locked$/gm), 3);
  assert.equal(occurrences(release, /git diff --exit-code -- src-tauri\/Cargo\.lock/g), 3);
  assert.equal(occurrences(release, /Remote tag no longer resolves to the validated main commit/g), 3);
});

test("builds frontend before exposing signing secrets and protects privileged jobs", () => {
  assert.equal(occurrences(release, /^    environment: release$/gm), 3);
  assert.equal(occurrences(release, /pnpm build/g), 4);
  assert.equal(occurrences(release, /test -d dist|Test-Path -Path dist/g), 3);
  for (const jobName of ["linux", "windows-nsis"]) {
    const job = jobSection(release, jobName);
    assert.ok(job.indexOf("pnpm build") < job.indexOf("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets."));
    assert.ok(job.indexOf("release-config.mjs") < job.indexOf("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets."));
  }
  assert.match(release, /node scripts\/release-config\.mjs[\s\S]*?tauri-apps\/tauri-action/);
});

test("runs locked release-mode Rust tests in CI and the release gate", () => {
  const command = /cargo test --locked --release --manifest-path src-tauri\/Cargo\.toml/g;
  assert.equal(occurrences(`${ci}\n${release}`, command), 2);
});

test("verifies local and downloaded signatures before atomic publication", () => {
  assert.equal(occurrences(release, /node scripts\/verify-updater-signatures\.mjs/g), 2);
  assert.match(release, /REQUIRE_MINISIGN_INTEGRATION: "1"[\s\S]*?pnpm test:signature-integration/);
  assert.match(release, /gh release upload "\$TAG" "\$\{assets\[@\]\}" --clobber/);
  assert.match(release, /gh release edit "\$TAG" --draft=false --latest/);
});

test("canonicalizes downloaded asset names before verification and publication", () => {
  const publish = jobSection(release, "publish");
  const download = publish.indexOf("uses: actions/download-artifact@");
  const canonicalize = publish.indexOf("- name: Canonicalize release asset filenames");
  const verify = publish.indexOf("- name: Verify signatures before changing the draft");
  const manifest = publish.indexOf("- name: Generate and validate latest.json");
  const upload = publish.indexOf("- name: Upload draft assets and verify the remote release");

  assert.ok(download < canonicalize);
  assert.ok(canonicalize < verify);
  assert.ok(canonicalize < manifest);
  assert.ok(canonicalize < upload);
  assert.match(
    publish.slice(download, canonicalize).trimEnd(),
    /merge-multiple: true$/,
  );

  const step = publish.slice(canonicalize, publish.indexOf("      - name:", canonicalize + 1));
  assert.match(step, /find release-assets -type f -name '\* \*' -print0/);
  assert.match(step, /canonical="\$\{basename\/\/ \/\.\}"/);
  assert.match(step, /\[\[ -e "\$target" \|\| -L "\$target" \]\]/);
  assert.match(step, /for claimed_target in "\$\{targets\[@\]\}"/);
  assert.match(step, /\[\[ "\$claimed_target" == "\$target" \]\]/);
  assert.ok(step.indexOf("done < <(find") < step.indexOf("mv --"));
  assert.ok(step.lastIndexOf("Canonical release asset target collision") < step.indexOf("mv --"));
});

test("canonicalization renames nested asset basenames containing spaces", async (t) => {
  const root = await canonicalizationFixture(t);
  const nested = path.join(root, "release-assets", "nested artifacts");
  await mkdir(nested);
  await writeFile(path.join(nested, "Linea POS setup.exe"), "asset");

  await runCanonicalization(root);

  assert.equal(await readFile(path.join(nested, "Linea.POS.setup.exe"), "utf8"), "asset");
  await assert.rejects(readFile(path.join(nested, "Linea POS setup.exe")), { code: "ENOENT" });
});

test("canonicalization rerun is a no-op", async (t) => {
  const root = await canonicalizationFixture(t);
  const source = path.join(root, "release-assets", "Linea POS.AppImage");
  const target = path.join(root, "release-assets", "Linea.POS.AppImage");
  await writeFile(source, "asset");

  await runCanonicalization(root);
  await runCanonicalization(root);

  assert.equal(await readFile(target, "utf8"), "asset");
});

test("canonicalization rejects a pre-existing target without partial moves", async (t) => {
  const root = await canonicalizationFixture(t);
  const assets = path.join(root, "release-assets");
  await writeFile(path.join(assets, "First Asset.deb"), "first");
  await writeFile(path.join(assets, "Blocked Asset.rpm"), "source");
  await writeFile(path.join(assets, "Blocked.Asset.rpm"), "target");

  await assert.rejects(runCanonicalization(root), /Canonical release asset target collision/);
  assert.equal(await readFile(path.join(assets, "First Asset.deb"), "utf8"), "first");
  assert.equal(await readFile(path.join(assets, "Blocked Asset.rpm"), "utf8"), "source");
  assert.equal(await readFile(path.join(assets, "Blocked.Asset.rpm"), "utf8"), "target");
  await assert.rejects(readFile(path.join(assets, "First.Asset.deb")), { code: "ENOENT" });
});

test("canonicalization rejects two sources claiming one target without partial moves", async (t) => {
  const root = await canonicalizationFixture(t);
  const assets = path.join(root, "release-assets");
  await writeFile(path.join(assets, "First Asset.deb"), "first");
  await writeFile(path.join(assets, "Same Name.sig"), "one");
  await writeFile(path.join(assets, "Same.Name sig"), "two");

  await assert.rejects(runCanonicalization(root), /Canonical release asset target collision/);
  assert.equal(await readFile(path.join(assets, "First Asset.deb"), "utf8"), "first");
  assert.equal(await readFile(path.join(assets, "Same Name.sig"), "utf8"), "one");
  assert.equal(await readFile(path.join(assets, "Same.Name sig"), "utf8"), "two");
  await assert.rejects(readFile(path.join(assets, "First.Asset.deb")), { code: "ENOENT" });
  await assert.rejects(readFile(path.join(assets, "Same.Name.sig")), { code: "ENOENT" });
});

test("pins reviewed actions to the expected commit objects", () => {
  for (const [action, sha] of expectedPins) {
    const references = [...`${ci}\n${release}`.matchAll(new RegExp(`uses: ${action.replace("/", "\\/")}@([0-9a-f]+)`, "g"))];
    assert.ok(references.length > 0, `${action} must be used`);
    assert.deepEqual(new Set(references.map((match) => match[1])), new Set([sha]));
  }
  for (const match of `${ci}\n${release}\n${releaseRequest}`.matchAll(/uses: [^@\s]+@([^\s]+)/g)) {
    assert.match(match[1], /^[0-9a-f]{40}$/, `action is not pinned to a full SHA: ${match[0]}`);
  }
  assert.doesNotMatch(`${ci}\n${release}`, /f40ffcd9367d9f12939873eb1018b921a783ffaa|49a0bdc70d2e1b713ca9e2869b211fcce03d3c1c|944946e3e4cac6603d1fe8f514171e9ecd3c78aa|11d5960a326750d5838078e36cf38b85af677262|49933ea5288caeca8642d1e84afbd3f7d6820020|b906affcce14559ad1aafd4ab0e942779e9f58b1|ea165f8d65b6e75b540449e92b4886f43607fa02|d3f86a106a0bac45b974a628896c90dbdf5c8093/);
});

test("reconciles residual draft assets before clobbering the exact local set", () => {
  const reconcile = release.match(/- name: Upload draft assets[\s\S]*?(?=      - name: Publish only)/)?.[0];
  assert.ok(reconcile);
  assert.match(reconcile, /gh release view "\$TAG" --json assets --jq '\.assets\[\]\.name'/);
  assert.match(reconcile, /! -v 'expected_assets\[\$remote_asset\]'/);
  assert.match(reconcile, /gh release delete-asset "\$TAG" "\$remote_asset" --yes/);
  assert.ok(reconcile.indexOf("gh release delete-asset") < reconcile.indexOf("gh release upload"));
  assert.match(reconcile, /\[\[ "\$local_assets" == "\$remote_assets" \]\]/);
});

test("scopes GH_TOKEN only to steps that invoke gh or the GitHub API", () => {
  assert.doesNotMatch(release, /^    env:\n      GH_TOKEN:/m);
  assert.equal(occurrences(release, /GH_TOKEN: \$\{\{ github\.token \}\}/g), 4);
});

test("revalidates draft state immediately around release mutations", () => {
  assert.equal(occurrences(release, /--json isDraft --jq \.isDraft/g), 3);
  assert.equal(occurrences(release, /git fetch --force origin "refs\/tags\/\$TAG"/g), 3);
});
