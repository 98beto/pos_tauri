import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const release = await readFile(new URL("../.github/workflows/release.yml", import.meta.url), "utf8");
const releaseRequest = await readFile(new URL("../.github/workflows/release-request.yml", import.meta.url), "utf8");
const ci = await readFile(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");

const expectedPins = new Map([
  ["pnpm/action-setup", "b906affcce14559ad1aafd4ab0e942779e9f58b1"],
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

test("pins reviewed actions to the expected commit objects", () => {
  for (const [action, sha] of expectedPins) {
    const references = [...`${ci}\n${release}`.matchAll(new RegExp(`uses: ${action.replace("/", "\\/")}@([0-9a-f]+)`, "g"))];
    assert.ok(references.length > 0, `${action} must be used`);
    assert.deepEqual(new Set(references.map((match) => match[1])), new Set([sha]));
  }
  for (const match of `${ci}\n${release}\n${releaseRequest}`.matchAll(/uses: [^@\s]+@([^\s]+)/g)) {
    assert.match(match[1], /^[0-9a-f]{40}$/, `action is not pinned to a full SHA: ${match[0]}`);
  }
  assert.doesNotMatch(`${ci}\n${release}`, /f40ffcd9367d9f12939873eb1018b921a783ffaa|49a0bdc70d2e1b713ca9e2869b211fcce03d3c1c|944946e3e4cac6603d1fe8f514171e9ecd3c78aa/);
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
