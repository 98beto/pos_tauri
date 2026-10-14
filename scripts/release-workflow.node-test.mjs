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
  return execFileAsync("bash", ["-c", canonicalizeScript], {
    cwd: root,
    env: { ...process.env, RUNNER_TEMP: root },
  });
}

test("uses least privilege and disables checkout credential persistence", () => {
  assert.match(release, /^permissions:\n  contents: read$/m);
  assert.match(release, /^  assemble:[\s\S]*?^    permissions:\n      contents: read$/m);
  assert.match(release, /^  publish:[\s\S]*?^    permissions:\n      contents: write$/m);
  assert.equal(occurrences(release, /uses: actions\/checkout@/g), 6);
  assert.equal(occurrences(release, /persist-credentials: false/g), 6);
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
  assert.match(release, /sha="\$\(git rev-parse origin\/main\^\{commit\}\)"/);
  assert.match(release, /version="\$\(git show "\$sha:package\.json" \| jq -er \.version\)"/);
});

test("exposes validate and publish modes while deriving validate metadata without releases", () => {
  assert.match(release, /mode:\n        description:[\s\S]*?default: validate[\s\S]*?type: choice[\s\S]*?- validate[\s\S]*?- publish/);
  assert.match(release, /tag:[\s\S]*?required: false/);
  assert.match(release, /^      mode: \$\{\{ steps\.release-source\.outputs\.mode \}\}$/m);
  assert.match(release, /^      tag: \$\{\{ steps\.release-source\.outputs\.tag \}\}$/m);
  assert.match(release, /^      sha: \$\{\{ steps\.release-source\.outputs\.sha \}\}$/m);
  assert.match(release, /else\n            mode=publish/);
  const validate = jobSection(release, "validate");
  assert.doesNotMatch(validate, /gh release|\/releases\/tags\/|contents: write/);
});

test("builds and publishes only the validated immutable commit", () => {
  assert.match(release, /^      sha: \$\{\{ steps\.release-source\.outputs\.sha \}\}$/m);
  assert.equal(occurrences(release, /ref: \$\{\{ needs\.validate\.outputs\.sha \}\}/g), 5);
  assert.equal(occurrences(release, /args: .* -- --locked$/gm), 3);
  assert.equal(occurrences(release, /git diff --exit-code -- src-tauri\/Cargo\.lock/g), 3);
  assert.equal(occurrences(release, /Remote tag no longer resolves to the validated main commit/g), 3);
});

test("captures git, find, and gh producer output before consuming it", () => {
  assert.doesNotMatch(release, /<\s*<\s*\(/);
  assert.match(release, /git for-each-ref[^\n]+> "\$tag_refs_file"[\s\S]*?done < "\$tag_refs_file"/);

  const nulCaptures = [
    ["find \"$directory\" -maxdepth 1 -name \"$pattern\" -print0", "matches_file", "done"],
    ["find release-linux -maxdepth 1 -type f -print0", "staged_file", "mapfile -d '' staged"],
    ["find release-assets -type f -name '* *' -print0", "spaced_assets_file", "done"],
    ["find release-assets -mindepth 1 -maxdepth 1 -print0", "assets_file", "mapfile -d '' assets"],
    ["find release-candidate -maxdepth 1 -type f -print0", "assets_file", "mapfile -d '' assets"],
  ];
  for (const [producer, capture, consumer] of nulCaptures) {
    const producerIndex = release.indexOf(`${producer} > "$${capture}"`);
    assert.notEqual(producerIndex, -1, `${producer} must write a checked NUL capture`);
    const consumerIndex = release.indexOf(`${consumer} < "$${capture}"`, producerIndex);
    assert.ok(consumerIndex > producerIndex, `${producer} must finish before its capture is consumed`);
  }

  const ghCapture = 'gh release view "$TAG" --json assets --jq \'\.assets\[\]\.name\' > "$remote_assets_file"';
  const ghIndex = release.indexOf(ghCapture);
  assert.notEqual(ghIndex, -1, "gh asset listing must write a checked text capture");
  assert.ok(release.indexOf('done < "$remote_assets_file"', ghIndex) > ghIndex);
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

test("verifies candidate and downloaded signatures before atomic publication", () => {
  assert.equal(occurrences(release, /node scripts\/verify-updater-signatures\.mjs/g), 5);
  assert.match(release, /REQUIRE_MINISIGN_INTEGRATION: "1"[\s\S]*?pnpm test:signature-integration/);
  assert.match(release, /gh release upload "\$TAG" "\$\{assets\[@\]\}" --clobber/);
  assert.match(release, /gh release edit "\$TAG" --draft=false --latest/);
});

test("sanitizes, signs, verifies, and uploads exactly six Linux assets in order", () => {
  const linux = jobSection(release, "linux");
  const build = linux.indexOf("- name: Build Linux installers");
  const sanitize = linux.indexOf("- name: Sanitize final AppImage bytes");
  const sign = linux.indexOf("- name: Sign final Linux installer bytes");
  const verify = linux.indexOf("- name: Verify final Linux installer signatures");
  const upload = linux.indexOf("- name: Upload exactly six Linux release assets");
  assert.ok(build < sanitize && sanitize < sign && sign < verify && verify < upload);
  assert.equal(stepRunBlock(linux, "Sanitize final AppImage bytes").trim(), 'scripts/sanitize-appimage.sh "$APPIMAGE"');
  assert.deepEqual(
    stepRunBlock(linux, "Sign final Linux installer bytes").trim().split("\n"),
    [
      "set -euo pipefail",
      'pnpm tauri signer sign "$APPIMAGE"',
      'pnpm tauri signer sign "$DEB"',
      'pnpm tauri signer sign "$RPM"',
    ],
  );
  const verifyBlock = stepRunBlock(linux, "Verify final Linux installer signatures");
  assert.match(verifyBlock, /for asset in "\$APPIMAGE" "\$DEB" "\$RPM"; do/);
  assert.match(verifyBlock, /node scripts\/verify-updater-signatures\.mjs --artifact "\$asset" --signature "\$asset\.sig"/);
  assert.match(linux, /while IFS= read -r -d '' candidate/);
  assert.match(linux, /find "\$directory" -maxdepth 1 -name "\$pattern" -print0/);
  assert.match(linux, /\[\[ "\$basename" == "\$expected" \]\]/);
  assert.match(linux, /"Linea POS_\$\{version\}_amd64\.AppImage"/);
  assert.match(linux, /"Linea POS_\$\{version\}_amd64\.deb"/);
  assert.match(linux, /"Linea POS-\$\{version\}-1\.x86_64\.rpm"/);
  assert.equal(occurrences(linux, /locate_one .* (?:AppImage|DEB|RPM) /g), 3);
  assert.equal(occurrences(linux, /pnpm tauri signer sign "\$(?:APPIMAGE|DEB|RPM)"/g), 3);
  assert.match(linux, /rm -f -- "\$asset\.sig"/);
  assert.match(linux, /\[\[ -f "\$asset" && ! -L "\$asset" && -s "\$asset" \]\]/);
  assert.match(linux, /\[\[ -f "\$asset\.sig" && ! -L "\$asset\.sig" && -s "\$asset\.sig" \]\]/);
  assert.match(linux, /\[\[ "\$\{#staged\[@\]\}" -eq 6 \]\]/);
  assert.match(linux, /sudo apt-get install[^\n]*minisign/);
  assert.match(linux, /release-config\.mjs[^\n]*--disable-updater-artifacts/);
  assert.match(linux, /path: release-linux\/\*/);
});

test("canonicalizes downloaded asset names before verification and publication", () => {
  const assemble = jobSection(release, "assemble");
  const download = assemble.indexOf("uses: actions/download-artifact@");
  const canonicalize = assemble.indexOf("- name: Canonicalize release asset filenames");
  const verify = assemble.indexOf("- name: Verify all four candidate updater signatures");
  const manifest = assemble.indexOf("- name: Generate and validate latest.json");
  const upload = assemble.indexOf("- name: Upload verified release candidate");

  assert.ok(download < canonicalize);
  assert.ok(canonicalize < verify);
  assert.ok(canonicalize < manifest);
  assert.ok(canonicalize < upload);
  assert.match(
    assemble.slice(download, canonicalize).trimEnd(),
    /merge-multiple: true$/,
  );

  const step = assemble.slice(canonicalize, assemble.indexOf("      - name:", canonicalize + 1));
  assert.match(step, /find release-assets -type f -name '\* \*' -print0/);
  assert.match(step, /canonical="\$\{basename\/\/ \/\.\}"/);
  assert.match(step, /\[\[ -e "\$target" \|\| -L "\$target" \]\]/);
  assert.match(step, /for claimed_target in "\$\{targets\[@\]\}"/);
  assert.match(step, /\[\[ "\$claimed_target" == "\$target" \]\]/);
  assert.ok(step.indexOf('done < "$spaced_assets_file"') < step.indexOf("mv --"));
  assert.ok(step.lastIndexOf("Canonical release asset target collision") < step.indexOf("mv --"));
});

test("assembles nine native assets plus latest.json in both modes", () => {
  const assemble = jobSection(release, "assemble");
  assert.match(assemble, /needs: \[validate, linux, windows-nsis, windows-msi\]/);
  assert.doesNotMatch(assemble, /^    if:/m);
  assert.match(assemble, /node scripts\/verify-updater-signatures\.mjs --assets release-assets/);
  assert.match(assemble, /node scripts\/latest-json\.mjs generate/);
  assert.match(assemble, /git log --format='- %s \(`%h`\)' "\$previous_tag\.\.\$SHA"/);
  assert.match(assemble, /candidate" =~ \^v\(0\|\[1-9\]\[0-9\]\*\)/);
  assert.match(assemble, /--notes-file "\$RUNNER_TEMP\/updater-release-notes\.md"/);
  assert.match(assemble, /node scripts\/latest-json\.mjs validate/);
  assert.match(assemble, /\[\[ "\$\{#assets\[@\]\}" -eq 10 \]\]/);
  assert.match(assemble, /find release-assets -mindepth 1 -maxdepth 1 -print0/);
  assert.match(assemble, /\[\[ -f "\$asset" && ! -L "\$asset" && -s "\$asset" \]\]/);
  assert.match(assemble, /name: release-candidate/);
  assert.doesNotMatch(assemble, /GH_TOKEN|gh release|contents: write|environment: release/);
});

test("keeps all release mutations in the publish-only job", () => {
  const publish = jobSection(release, "publish");
  assert.match(publish, /if: needs\.validate\.outputs\.mode == 'publish'/);
  assert.match(publish, /environment: release/);
  assert.match(publish, /contents: write/);
  for (const command of ["create", "upload", "edit", "delete-asset"]) {
    assert.match(publish, new RegExp(`gh release ${command}`));
  }
  const outsidePublish = release.replace(publish, "");
  assert.doesNotMatch(outsidePublish, /gh release (?:create|upload|edit|delete)/);
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
  const reconcile = release.match(/- name: Reconcile draft assets[\s\S]*?(?=      - name: Publish only)/)?.[0];
  assert.ok(reconcile);
  assert.match(reconcile, /gh release view "\$TAG" --json assets --jq '\.assets\[\]\.name'/);
  assert.match(reconcile, /! -v 'expected_assets\[\$remote_asset\]'/);
  assert.match(reconcile, /gh release delete-asset "\$TAG" "\$remote_asset" --yes/);
  assert.ok(reconcile.indexOf("gh release delete-asset") < reconcile.indexOf("gh release upload"));
  assert.match(reconcile, /\[\[ "\$local_assets" == "\$remote_assets" \]\]/);
  assert.match(reconcile, /verify-updater-signatures\.mjs --assets "\$RUNNER_TEMP\/remote-assets"/);
  assert.match(reconcile, /latest-json\.mjs validate/);
  assert.match(reconcile, /\(cd release-candidate && sha256sum -- \* \| sort\)/);
  assert.match(reconcile, /\(cd "\$RUNNER_TEMP\/remote-assets" && sha256sum -- \* \| sort\)/);
  assert.match(reconcile, /wc -l < "\$RUNNER_TEMP\/local-sha256"\)" -eq 10/);
  assert.match(reconcile, /diff -u "\$RUNNER_TEMP\/local-sha256" "\$RUNNER_TEMP\/remote-sha256"/);
});

test("scopes GH_TOKEN only to steps that invoke gh or the GitHub API", () => {
  assert.doesNotMatch(release, /^    env:\n      GH_TOKEN:/m);
  assert.equal(occurrences(release, /GH_TOKEN: \$\{\{ github\.token \}\}/g), 3);
  assert.doesNotMatch(jobSection(release, "assemble"), /GH_TOKEN/);
});

test("refreshes main and the tag and revalidates before every release mutation", () => {
  const publish = jobSection(release, "publish");
  const refresh = /git fetch --no-tags origin "\+refs\/heads\/main:refs\/remotes\/origin\/main" "\+refs\/tags\/\$TAG:refs\/tags\/\$TAG"/g;
  assert.equal(occurrences(publish, refresh), 3);
  assert.equal(occurrences(publish, /git merge-base --is-ancestor "\$remote_sha" origin\/main/g), 3);
  assert.match(publish, /else\n            verify_release_source\n            gh release create/);
  assert.match(publish, /verify_release_source\n              \[\[ "\$\(gh release view[\s\S]*?gh release delete-asset/);
  assert.match(publish, /verify_release_source\n          \[\[ "\$\(gh release view[\s\S]*?gh release upload/);
  assert.match(publish, /git merge-base --is-ancestor[\s\S]*?gh release edit "\$TAG" --draft=false --latest/);
});

test("limits private signing secrets to the final Linux and NSIS signers", () => {
  assert.doesNotMatch(jobSection(release, "validate"), /secrets\.TAURI_SIGNING/);
  assert.doesNotMatch(jobSection(release, "assemble"), /secrets\.TAURI_SIGNING/);
  assert.doesNotMatch(jobSection(release, "publish"), /secrets\.TAURI_SIGNING/);
  const linux = jobSection(release, "linux");
  const linuxBuild = linux.slice(
    linux.indexOf("- name: Build Linux installers"),
    linux.indexOf("      - name:", linux.indexOf("- name: Build Linux installers") + 1),
  );
  assert.doesNotMatch(linuxBuild, /TAURI_SIGNING_PRIVATE_KEY/);
  assert.equal(occurrences(linux, /secrets\.TAURI_SIGNING_PRIVATE_KEY(?!_PASSWORD)/g), 1);
  assert.equal(occurrences(linux, /secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD/g), 1);
  assert.match(linux, /- name: Sign final Linux installer bytes[\s\S]*?secrets\.TAURI_SIGNING_PRIVATE_KEY/);
  assert.doesNotMatch(linux, /- name: Require updater signing secrets/);

  const nsis = jobSection(release, "windows-nsis");
  assert.equal(occurrences(nsis, /secrets\.TAURI_SIGNING_PRIVATE_KEY(?!_PASSWORD)/g), 1);
  assert.equal(occurrences(nsis, /secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD/g), 1);
  const nsisBuild = nsis.slice(
    nsis.indexOf("- name: Build NSIS installer without signing secrets"),
    nsis.indexOf("      - name:", nsis.indexOf("- name: Build NSIS installer without signing secrets") + 1),
  );
  assert.doesNotMatch(nsisBuild, /TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(nsis, /- name: Sign final NSIS installer bytes[\s\S]*?secrets\.TAURI_SIGNING_PRIVATE_KEY/);
  assert.doesNotMatch(nsis, /- name: Require updater signing secrets/);
});

test("enables updater UI only for updater-capable bundles", () => {
  const linux = jobSection(release, "linux");
  const nsis = jobSection(release, "windows-nsis");
  const msi = jobSection(release, "windows-msi");
  assert.match(linux, /VITE_UPDATER_ENABLED: "true"[\s\S]*?pnpm build/);
  assert.match(nsis, /VITE_UPDATER_ENABLED: "true"[\s\S]*?pnpm build/);
  assert.doesNotMatch(msi, /VITE_UPDATER_ENABLED/);
  assert.match(msi, /release-config\.mjs[^\n]*--disable-updater/);
});

test("revalidates the downloaded remote bytes immediately before publication", () => {
  const final = stepRunBlock(jobSection(release, "publish"), "Publish only after all remote checks pass");
  assert.match(final, /gh release download "\$TAG" --dir "\$RUNNER_TEMP\/final-remote-assets"/);
  assert.match(final, /verify-updater-signatures\.mjs --assets "\$RUNNER_TEMP\/final-remote-assets"/);
  assert.match(final, /latest-json\.mjs validate/);
  assert.match(final, /diff -u "\$RUNNER_TEMP\/final-local-sha256" "\$RUNNER_TEMP\/final-remote-sha256"/);
  assert.ok(final.indexOf("diff -u") < final.indexOf('gh release edit "$TAG" --draft=false --latest'));
});
