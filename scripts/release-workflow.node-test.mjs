import assert from "node:assert/strict";
import { execFile, spawnSync } from "node:child_process";
import { chmod, mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
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
  assert.match(release, /^permissions:\n  actions: read\n  contents: read$/m);
  assert.match(release, /^  assemble:[\s\S]*?^    permissions:\n      contents: read$/m);
  assert.match(release, /^  publish:[\s\S]*?^    permissions:\n      actions: read\n      contents: write$/m);
  assert.equal(occurrences(release, /uses: actions\/checkout@/g), 7);
  assert.equal(occurrences(release, /persist-credentials: false/g), 7);
  assert.equal(occurrences(ci, /uses: actions\/checkout@/g), 2);
  assert.equal(occurrences(ci, /persist-credentials: false/g), 2);
  assert.match(releaseRequest, /^permissions: \{\}$/m);
  assert.doesNotMatch(releaseRequest, /secrets\.|contents: write|actions\/checkout/);
});

test("loads privileged release logic from main after an unprivileged request", () => {
  assert.doesNotMatch(release, /^  push:/m);
  assert.match(release, /^  workflow_run:\n    workflows: \["Release request"\]/m);
  assert.match(release, /github\.event\.workflow_run\.conclusion == 'success'/);
  assert.match(release, /ref: \$\{\{ github\.event_name == 'workflow_dispatch' && github\.sha \|\| 'main' \}\}/);
  assert.match(release, /REQUEST_SHA: \$\{\{ github\.event\.workflow_run\.head_sha \}\}/);
  assert.match(release, /Expected exactly one vX\.Y\.Z tag at \$sha/);
  assert.match(release, /git merge-base --is-ancestor "\$sha" origin\/main/);
  assert.match(release, /DISPATCH_REF.*github\.ref/);
  assert.match(release, /refs\/heads\/main/);
  assert.match(release, /DISPATCH_SHA: \$\{\{ github\.sha \}\}/);
  assert.match(release, /sha="\$DISPATCH_SHA"/);
  assert.doesNotMatch(jobSection(release, "release-source"), /sha=.*git rev-parse origin\/main/);
  assert.match(release, /version="\$\(git show "\$sha:package\.json" \| jq -er \.version\)"/);
});

test("validate pins the dispatch SHA after main advances and keeps candidate_run_id selectable", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos-release-dispatch-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const origin = path.join(root, "origin.git");
  const writer = path.join(root, "writer");
  const checkout = path.join(root, "checkout");
  const env = {
    ...process.env,
    GIT_AUTHOR_NAME: "Release test",
    GIT_AUTHOR_EMAIL: "release@example.invalid",
    GIT_COMMITTER_NAME: "Release test",
    GIT_COMMITTER_EMAIL: "release@example.invalid",
    RUNNER_TEMP: root,
    GITHUB_REPOSITORY: "fixture/pos",
    GITHUB_OUTPUT: path.join(root, "source-output"),
    GITHUB_RUN_ID: "123",
    EVENT_NAME: "workflow_dispatch",
    DISPATCH_REF: "refs/heads/main",
    DISPATCH_MODE: "validate",
    DISPATCH_TAG: "",
    DISPATCH_CANDIDATE_RUN_ID: "",
  };
  const git = async (cwd, ...args) => (await execFileAsync("git", args, { cwd, env })).stdout.trim();
  await git(root, "init", "--bare", "--initial-branch=main", origin);
  await git(root, "clone", origin, writer);
  await writeFile(path.join(writer, "package.json"), JSON.stringify({ version: "1.2.3" }));
  await git(writer, "add", "package.json");
  await git(writer, "commit", "-m", "Dispatched release");
  await git(writer, "push", "origin", "main");
  const dispatchSha = await git(writer, "rev-parse", "HEAD");
  await git(root, "clone", origin, checkout);
  await git(checkout, "checkout", "--detach", dispatchSha);

  // Advance remote main after dispatch, including metadata that must not leak into the candidate.
  await writeFile(path.join(writer, "package.json"), JSON.stringify({ version: "1.2.4" }));
  await git(writer, "commit", "-am", "Advance main after dispatch");
  await git(writer, "push", "origin", "main");
  const advancedSha = await git(writer, "rev-parse", "HEAD");
  const resolve = (sha) => execFileAsync("bash", ["-c", stepRunBlock(jobSection(release, "release-source"), "Resolve trusted release mode, tag, and immutable commit")], {
    cwd: checkout, env: { ...env, DISPATCH_SHA: sha },
  });
  await resolve(dispatchSha);
  assert.equal(await git(checkout, "rev-parse", "origin/main"), advancedSha);
  assert.equal(await git(checkout, "rev-parse", "HEAD"), dispatchSha);
  assert.equal(await readFile(env.GITHUB_OUTPUT, "utf8"), `mode=validate\ntag=v1.2.3\nsha=${dispatchSha}\n`);

  await mkdir(path.join(checkout, "release-assets"));
  for (let index = 0; index < 10; index++) {
    await writeFile(path.join(checkout, "release-assets", `asset-${index}`), `approved bytes ${index}`);
  }
  await execFileAsync("bash", ["-c", stepRunBlock(release, "Record candidate provenance and exact publication hashes")], {
    cwd: checkout, env: { ...env, SHA: dispatchSha, TAG: "v1.2.3" },
  });
  const provenance = JSON.parse(await readFile(path.join(checkout, "release-assets", "candidate-provenance.json"), "utf8"));
  assert.equal(provenance.sha, dispatchSha);
  assert.equal(provenance.tag, "v1.2.3");
  assert.equal(provenance.artifact_name, `release-candidate-${dispatchSha}`);
  assert.equal(provenance.validate_run_id, 123);

  const bin = path.join(root, "bin");
  await mkdir(bin);
  // Mock only the API boundary; execute the workflow's actual candidate selector and jq predicates.
  await writeFile(path.join(bin, "gh"), `#!${process.execPath}
import assert from "node:assert/strict";
const args = process.argv.slice(2);
if (args[1] === "--paginate") {
  assert.equal(args[2], "/repos/fixture/pos/actions/artifacts?name=" + process.env.ARTIFACT_NAME + "&per_page=100");
  process.stdout.write("456\\t123\\n789\\t124\\n");
} else {
  assert.deepEqual(args, ["api", "/repos/fixture/pos/actions/runs/123"]);
  process.stdout.write(JSON.stringify({ name: "Release", event: "workflow_dispatch", conclusion: "success", head_sha: process.env.DISPATCH_SHA }));
}
`, { mode: 0o755 });
  const select = (sha, runId = "123") => execFileAsync("bash", ["-c", stepRunBlock(release, "Resolve the QA-approved validate artifact")], {
    cwd: checkout,
    env: {
      ...env, PATH: `${bin}:${process.env.PATH}`, DISPATCH_SHA: dispatchSha,
      EXPECTED_SHA: sha, ARTIFACT_NAME: provenance.artifact_name, REQUESTED_RUN_ID: runId,
      GITHUB_OUTPUT: path.join(root, "candidate-output"),
    },
  });
  await select(dispatchSha);
  assert.equal(await readFile(path.join(root, "candidate-output"), "utf8"), "artifact_id=456\nvalidate_run_id=123\n");
  await assert.rejects(select(advancedSha), /found 0/);
  await assert.rejects(select(dispatchSha, "999"), /found 0/);

  await assert.rejects(resolve("not-a-sha"), /Validate dispatch has an invalid SHA/);
  await git(writer, "checkout", "--orphan", "outside-main");
  await git(writer, "commit", "-m", "Unrelated history");
  const outsideSha = await git(writer, "rev-parse", "HEAD");
  await git(writer, "push", "origin", "outside-main");
  await git(checkout, "fetch", "origin", "outside-main");
  await assert.rejects(resolve(outsideSha));
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
  assert.equal(occurrences(release, /ref: \$\{\{ needs\.validate\.outputs\.sha \}\}/g), 4);
  assert.equal(occurrences(release, /ref: \$\{\{ needs\.release-source\.outputs\.sha \}\}/g), 2);
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
    ["find publication-assets -maxdepth 1 -type f -print0", "assets_file", "mapfile -d '' assets"],
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
  assert.equal(occurrences(release, /^    environment: release$/gm), 2);
  assert.equal(occurrences(release, /^    environment: release-publication$/gm), 1);
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
  assert.equal(occurrences(`${ci}\n${release}`, command), 3);
  assert.match(jobSection(ci, "windows-rust"), command);
});

test("proves maintenance mode is present in every installer payload", () => {
  const linux = jobSection(release, "linux");
  const nsis = jobSection(release, "windows-nsis");
  const msi = jobSection(release, "windows-msi");

  assert.match(linux, /verify-linux-maintenance-bundles\.sh "\$APPIMAGE" "\$DEB" "\$RPM"/);
  const sanitize = linux.indexOf("- name: Sanitize final AppImage bytes");
  const maintenance = linux.indexOf("- name: Prove maintenance mode is executable from final Linux bundles");
  const sign = linux.indexOf("- name: Sign final Linux installer bytes");
  assert.ok(sanitize < maintenance && maintenance < sign);
  assert.match(linux, /sudo apt-get install[^\n]*rpm cpio/);
  for (const windowsJob of [nsis, msi]) {
    assert.doesNotMatch(windowsJob, /target\/release\/pos_tauri\.exe[^\n]*maintenance-info/);
    assert.match(windowsJob, /Get-ChildItem[^\n]*\$payload[^\n]*-Recurse[^\n]*pos_tauri\.exe/);
    assert.match(windowsJob, /\$executables\[0\]\.Length -eq 0/);
    assert.doesNotMatch(windowsJob, /& \$executables|\$LASTEXITCODE/);
    assert.match(windowsJob, /\$maintenance = Start-Process -FilePath \$executables\[0\]\.FullName -ArgumentList "maintenance-info" -Wait -PassThru -RedirectStandardOutput \$stdoutPath -RedirectStandardError \$stderrPath/);
    assert.match(windowsJob, /Join-Path \$env:RUNNER_TEMP \("(?:nsis|msi)-maintenance-" \+ \[guid\]::NewGuid\(\)\.ToString\("N"\)\)/);
    assert.match(windowsJob, /\$stdoutPath = "\$capture\.stdout"\n\s+\$stderrPath = "\$capture\.stderr"/);
    assert.match(windowsJob, /\$stdout = \[System\.IO\.File\]::ReadAllText\(\$stdoutPath\)/);
    assert.match(windowsJob, /\$stderr = \[System\.IO\.File\]::ReadAllText\(\$stderrPath\)/);
    assert.match(windowsJob, /\$maintenance\.ExitCode -ne 0 -or \$stdout -cne "Linea POS 0\.2\.1: mantenimiento local disponible`n" -or \$stderr\.Length -ne 0/);
    assert.match(windowsJob, /throw "[^"]*exit=\$\(\$maintenance\.ExitCode\)[^\n]*stdout=\[\$stdout\] stderr=\[\$stderr\]/);
    assert.doesNotMatch(windowsJob, /\.Trim\(|Get-Content/);
  }
  assert.ok(nsis.indexOf("Prove maintenance mode") < nsis.indexOf("Sign final NSIS installer bytes"));
  assert.ok(msi.indexOf("Prove maintenance mode") < msi.indexOf("actions/upload-artifact@"));
  assert.match(nsis, /Start-Process[^\n]*\$installer\.FullName[^\n]*\/S[^\n]*\/D=\$payload[^\n]*-Wait -PassThru/);
  assert.match(msi, /Start-Process[^\n]*msiexec\.exe[^\n]*-Wait -PassThru/);
  assert.match(msi, /@\("\/a"[^\n]*"\/qn"[^\n]*"\/norestart"[^\n]*TARGETDIR/);
});

const maintenanceVerifier = fileURLToPath(new URL("./verify-linux-maintenance-bundles.sh", import.meta.url));
const maintenanceOutput = "Linea POS 0.2.1: mantenimiento local disponible\n";

async function maintenanceFixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos maintenance bundles "));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bin = path.join(root, "tools");
  await mkdir(bin);
  async function executable(file, contents) {
    await writeFile(file, `#!/usr/bin/env bash\nset -euo pipefail\n${contents}\n`, { mode: 0o755 });
  }
  for (const format of ["appimage", "deb", "rpm"]) {
    await mkdir(path.join(root, format, "usr", "bin"), { recursive: true });
    await executable(path.join(root, format, "usr", "bin", "pos_tauri"), `[[ $# == 1 && $1 == maintenance-info ]]
printf '%s\\n' 'Linea POS 0.2.1: mantenimiento local disponible'`);
  }
  await executable(path.join(root, "appimage", "AppRun"), `printf 'AppRun\\n' >> "$FIXTURE_ROOT/launches"
printf '%s' "\${LAUNCHER_PREFIX-}"
"$FIXTURE_ROOT/appimage/usr/bin/pos_tauri" "$@"
printf '%s' "\${LAUNCHER_SUFFIX-}"
exit "\${LAUNCHER_STATUS-0}"`);
  const appimage = path.join(root, "Linea POS.AppImage");
  await executable(appimage, `case "\${1-}" in
  --appimage-extract-and-run) shift; exec "$FIXTURE_ROOT/appimage/AppRun" "$@" ;;
  --appimage-extract) [[ \${APPIMAGE_EXTRACT_STATUS-0} == 0 ]] || exit "$APPIMAGE_EXTRACT_STATUS"; mkdir squashfs-root; cp -a "$FIXTURE_ROOT/appimage/." squashfs-root/ ;;
  *) exit 90 ;;
esac`);
  const deb = path.join(root, "Linea POS.deb");
  const rpm = path.join(root, "Linea POS.rpm");
  await writeFile(deb, "deb");
  await writeFile(rpm, "rpm");
  await executable(path.join(bin, "dpkg-deb"), `[[ $# == 3 && $1 == --extract && $2 == "$FIXTURE_ROOT/Linea POS.deb" ]]
cp -a "$FIXTURE_ROOT/deb/." "$3/"`);
  await executable(path.join(bin, "rpm2cpio"), `[[ $# == 1 && $1 == "$FIXTURE_ROOT/Linea POS.rpm" ]]
printf 'rpm payload'`);
  await executable(path.join(bin, "cpio"), `[[ $# == 3 && $1 == --extract && $2 == --make-directories && $3 == --quiet ]]
[[ $(cat) == 'rpm payload' ]]
cp -a "$FIXTURE_ROOT/rpm/." ./`);
  return {
    root,
    run: (env = {}) => execFileAsync("bash", [maintenanceVerifier, appimage, deb, rpm], {
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, FIXTURE_ROOT: root, ...env },
    }),
  };
}

test("Linux maintenance verifier executes the final AppImage through AppRun and checks all payloads", async (t) => {
  const f = await maintenanceFixture(t);
  await f.run();
  assert.equal(await readFile(path.join(f.root, "launches"), "utf8"), "AppRun\n");
});

test("RPM extraction creates missing parents from a real file-only CPIO archive", async (t) => {
  const probe = spawnSync("cpio", ["--version"]);
  if (probe.error?.code === "ENOENT") return t.skip("cpio is not installed");
  assert.equal(probe.status, 0);
  const f = await maintenanceFixture(t);
  const archive = spawnSync("cpio", ["--create", "--format=newc", "--quiet"], {
    cwd: path.join(f.root, "rpm"), input: "./usr/bin/pos_tauri\n",
  });
  assert.equal(archive.status, 0, archive.stderr.toString());
  const listing = spawnSync("cpio", ["--list", "--quiet"], { input: archive.stdout });
  assert.equal(listing.status, 0);
  assert.match(listing.stdout.toString(), /^(?:\.\/)?usr\/bin\/pos_tauri\n$/);
  await writeFile(path.join(f.root, "Linea POS.rpm"), archive.stdout);
  await rm(path.join(f.root, "tools", "cpio"));
  await writeFile(path.join(f.root, "tools", "rpm2cpio"), '#!/usr/bin/env bash\nexec cat -- "$1"\n');
  await f.run();

  // Prove the same archive reproduces the release failure without the fix.
  const broken = path.join(f.root, "verifier-without-mkdir.sh");
  await writeFile(broken, (await readFile(maintenanceVerifier, "utf8")).replace(" --make-directories", ""));
  await assert.rejects(execFileAsync("bash", [broken, ...["Linea POS.AppImage", "Linea POS.deb", "Linea POS.rpm"].map((name) => path.join(f.root, name))], {
    env: { ...process.env, LC_ALL: "C", PATH: `${path.join(f.root, "tools")}:${process.env.PATH}`, FIXTURE_ROOT: f.root },
  }), /Cannot open/);

  // Extraction must preserve a defective archive's mode and report it, never repair it.
  await chmod(path.join(f.root, "rpm", "usr", "bin", "pos_tauri"), 0o644);
  const nonExecutable = spawnSync("cpio", ["--create", "--format=newc", "--quiet"], {
    cwd: path.join(f.root, "rpm"), input: "./usr/bin/pos_tauri\n",
  });
  assert.equal(nonExecutable.status, 0, nonExecutable.stderr.toString());
  await writeFile(path.join(f.root, "Linea POS.rpm"), nonExecutable.stdout);
  await assert.rejects(f.run({ LC_ALL: "C" }), (error) => {
    assert.equal(error.code, 1);
    assert.match(error.stderr, /stage=rpm\.payload exit=1/);
    assert.match(error.stderr, /type=regular file mode=644/);
    return true;
  });
});

test("Linux maintenance failure diagnostics identify extraction stages and preserve exit codes", async (t) => {
  for (const format of ["appimage", "deb", "rpm"]) {
    await t.test(format, async (t) => {
      const f = await maintenanceFixture(t);
      if (format !== "appimage") {
        const tool = format === "deb" ? "dpkg-deb" : "rpm2cpio";
        await writeFile(path.join(f.root, "tools", tool), "#!/usr/bin/env bash\nexit 37\n");
        if (format === "rpm") {
          await writeFile(path.join(f.root, "tools", "cpio"), "#!/usr/bin/env bash\ncat >/dev/null\n");
        }
      }
      await assert.rejects(f.run({ LC_ALL: "C", APPIMAGE_EXTRACT_STATUS: format === "appimage" ? "37" : "0" }), (error) => {
        assert.equal(error.code, 37);
        assert.match(error.stderr, new RegExp(`stage=${format}\\.extract exit=37`));
        assert.match(error.stderr, /type=regular file mode=/);
        assert.match(error.stderr, /command=/);
        return true;
      });
    });
  }
});

test("Windows GUI payload capture waits and rejects inexact output, stderr, and failure codes", async (t) => {
  if (process.platform !== "win32") return t.skip("requires Windows GUI subsystem and pwsh");
  const probe = spawnSync("pwsh", ["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"]);
  if (probe.error?.code === "ENOENT") return t.skip("pwsh is not installed");
  assert.equal(probe.status, 0);
  const root = await mkdtemp(path.join(os.tmpdir(), "pos GUI capture "));
  t.after(() => rm(root, { recursive: true, force: true }));
  const source = path.join(root, "fixture.cs");
  const exe = path.join(root, "pos_tauri.exe");
  await writeFile(source, `using System;
using System.IO;
using System.Text;
using System.Threading;
class Fixture {
  static int Main(string[] args) {
    if (args.Length != 1 || args[0] != "maintenance-info") return 99;
    Thread.Sleep(300);
    string defect = Environment.GetEnvironmentVariable("CAPTURE_DEFECT");
    string output = "Linea POS 0.2.1: mantenimiento local disponible\\n";
    if (defect == "extra") output += "\\n";
    if (defect == "missing-newline") output = output.TrimEnd('\\n');
    if (defect == "case") output = output.ToUpperInvariant();
    byte[] bytes = Encoding.UTF8.GetBytes(output);
    using (Stream stdout = Console.OpenStandardOutput()) stdout.Write(bytes, 0, bytes.Length);
    if (defect == "stderr") {
      using (Stream stderr = Console.OpenStandardError()) stderr.WriteByte(88);
    }
    return defect == "exit" ? 42 : 0;
  }
}`);
  const compiler = path.join(process.env.WINDIR, "Microsoft.NET", "Framework64", "v4.0.30319", "csc.exe");
  await execFileAsync(compiler, ["/nologo", "/target:winexe", `/out:${exe}`, source]);
  for (const format of ["NSIS", "MSI"]) {
    const block = stepRunBlock(release, `Prove maintenance mode is in the ${format} payload`);
    const capture = block.slice(block.indexOf("$capture ="));
    const script = path.join(root, `${format}.ps1`);
    await writeFile(script, `$ErrorActionPreference = "Stop"\n$executables = @(Get-Item -LiteralPath $env:GUI_FIXTURE)\n${capture}`);
    const run = (defect = "") => execFileAsync("pwsh", ["-NoProfile", "-File", script], {
      env: { ...process.env, RUNNER_TEMP: root, GUI_FIXTURE: exe, CAPTURE_DEFECT: defect },
    });
    await run();
    await run(); // Repeated invocations must use independent capture files.
    for (const defect of ["extra", "missing-newline", "case", "stderr", "exit"]) {
      await assert.rejects(run(defect), /payload maintenance-info failed/);
    }
  }
});

test("Linux maintenance verifier rejects a broken launcher even when its payload binary works", async (t) => {
  const f = await maintenanceFixture(t);
  const binary = path.join(f.root, "appimage", "usr", "bin", "pos_tauri");
  assert.equal((await execFileAsync(binary, ["maintenance-info"])).stdout, maintenanceOutput);
  await assert.rejects(f.run({ LAUNCHER_STATUS: "42" }), { code: 42 });
  assert.equal(await readFile(path.join(f.root, "launches"), "utf8"), "AppRun\n");
});

test("Linux maintenance verifier requires exact AppImage stdout", async (t) => {
  const f = await maintenanceFixture(t);
  for (const env of [{ LAUNCHER_PREFIX: "unexpected\n" }, { LAUNCHER_SUFFIX: "\n" }]) {
    await assert.rejects(f.run(env));
  }
});

test("Linux maintenance verifier retains regular executable payload checks for every format", async (t) => {
  for (const format of ["appimage", "deb", "rpm"]) {
    for (const defect of ["missing", "symlink", "dangling symlink", "directory", "not executable", "extra output", "nonzero"]) {
      await t.test(`${format}: ${defect}`, async (t) => {
        const f = await maintenanceFixture(t);
        const binary = path.join(f.root, format, "usr", "bin", "pos_tauri");
        if (defect === "missing") await rm(binary);
        if (defect === "symlink") {
          const target = path.join(f.root, "payload executable");
          await writeFile(target, await readFile(binary), { mode: 0o755 });
          await rm(binary);
          await symlink(target, binary);
        }
        if (defect === "dangling symlink") {
          await rm(binary);
          await symlink("missing-target", binary);
        }
        if (defect === "directory") {
          await rm(binary);
          await mkdir(binary);
        }
        if (defect === "not executable") await chmod(binary, 0o644);
        if (defect === "extra output" || defect === "nonzero") {
          await writeFile(binary, `${await readFile(binary, "utf8")}\n${defect === "extra output" ? "printf 'extra\\n'" : "exit 42"}\n`);
        }
        await assert.rejects(f.run({ LC_ALL: "C" }), (error) => {
          assert.match(error.stderr, new RegExp(`stage=${format}\\.(launcher|payload) exit=`));
          assert.match(error.stderr, format === "appimage" && defect !== "symlink" ? /path=.*POS\.AppImage/ : /path=.*pos_tauri/);
          if (defect === "symlink" || (defect === "dangling symlink" && format !== "appimage")) {
            assert.match(error.stderr, /type=symbolic link mode=/);
            assert.match(error.stderr, /symlink target:/);
          }
          if (format !== "appimage" && defect === "not executable") assert.match(error.stderr, /type=regular file mode=644/);
          if (format !== "appimage" && defect === "directory") assert.match(error.stderr, /type=directory mode=/);
          if (defect === "nonzero") assert.equal(error.code, 42);
          if (defect === "extra output" || defect === "nonzero") {
            assert.match(error.stderr, /maintenance stdout bytes/);
          }
          return true;
        });
      });
    }
  }
});

test("Windows restore documentation preserves quoted paths and synchronous exit-code handling", async () => {
  const docs = await readFile(new URL("../docs/releases.md", import.meta.url), "utf8");
  const block = docs.match(/```powershell\n([\s\S]*?)```/)?.[1];
  assert.ok(block);
  const args = block.match(/^\$restoreArgs = '([^'\n]+)'$/m)?.[1];
  assert.ok(args, "Start-Process needs a single argument string with literal quotes");
  assert.match(args, /^restore-migration-backup --database "[^"\r\n]+" --backup "[^"\r\n]+"$/);
  // These concrete paths contain spaces; outer PowerShell quotes alone do not survive ArgumentList.
  const paths = [...args.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
  assert.equal(paths.length, 2);
  for (const value of paths) assert.match(value, /^C:\\[^\r\n]* [^\r\n]*\.db$/);
  const commands = block.split("\n").filter((line) => line.includes("= Start-Process "));
  assert.equal(commands.length, 2);
  for (const command of commands) {
    assert.match(command, /-ArgumentList \$restoreArgs -Wait -PassThru$/);
  }
  assert.equal(occurrences(block, /^\$result\.ExitCode$/gm), 2);
});

test("requires the SQLite 13 migration rehearsal in CI and the release gate", () => {
  assert.equal(occurrences(`${ci}\n${release}`, /pnpm qa:migration-v8-v13/g), 2);
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
  const maintenance = linux.indexOf("- name: Prove maintenance mode is executable from final Linux bundles");
  const sign = linux.indexOf("- name: Sign final Linux installer bytes");
  const verify = linux.indexOf("- name: Verify final Linux installer signatures");
  const upload = linux.indexOf("- name: Upload exactly six Linux release assets");
  assert.ok(build < sanitize && sanitize < maintenance && maintenance < sign && sign < verify && verify < upload);
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

test("assembles and records the candidate only in validate mode", () => {
  const assemble = jobSection(release, "assemble");
  assert.match(assemble, /needs: \[validate, linux, windows-nsis, windows-msi\]/);
  assert.doesNotMatch(assemble, /^    if:/m);
  assert.match(assemble, /node scripts\/verify-updater-signatures\.mjs --assets release-assets/);
  assert.match(assemble, /node scripts\/latest-json\.mjs generate/);
  assert.match(assemble, /git log --format='- %s \(`%h`\)' "\$previous_tag\.\.\$SHA"/);
  assert.match(assemble, /candidate" =~ \^v\(0\|\[1-9\]\[0-9\]\*\)/);
  assert.match(assemble, /--notes-file "\$RUNNER_TEMP\/updater-release-notes\.md"/);
  assert.match(assemble, /node scripts\/latest-json\.mjs validate/);
  assert.match(assemble, /sha256sum -- \* \| sort\) > "\$hashes"[\s\S]*?mv -- "\$hashes" release-assets\/SHA256SUMS/);
  assert.match(assemble, /candidate-provenance\.json/);
  assert.match(assemble, /validate_run_id:\$validate_run_id/);
  assert.match(assemble, /\[\[ "\$\{#assets\[@\]\}" -eq 12 \]\]/);
  assert.match(assemble, /find release-assets -mindepth 1 -maxdepth 1 -print0/);
  assert.match(assemble, /\[\[ -f "\$asset" && ! -L "\$asset" && -s "\$asset" \]\]/);
  assert.match(assemble, /name: release-candidate-\$\{\{ needs\.validate\.outputs\.sha \}\}/);
  assert.doesNotMatch(assemble, /GH_TOKEN|gh release|contents: write|environment: release/);
});

test("keeps all release mutations in the publish-only job", () => {
  const publish = jobSection(release, "publish");
  assert.match(publish, /if: needs\.release-source\.outputs\.mode == 'publish'/);
  assert.match(publish, /environment: release-publication/);
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
  assert.match(reconcile, /\(cd publication-assets && sha256sum -- \* \| sort\)/);
  assert.match(reconcile, /\(cd "\$RUNNER_TEMP\/remote-assets" && sha256sum -- \* \| sort\)/);
  assert.match(reconcile, /wc -l < "\$RUNNER_TEMP\/local-sha256"\)" -eq 10/);
  assert.match(reconcile, /diff -u "\$RUNNER_TEMP\/local-sha256" "\$RUNNER_TEMP\/remote-sha256"/);
});

test("scopes GH_TOKEN only to steps that invoke gh or the GitHub API", () => {
  assert.doesNotMatch(release, /^    env:\n      GH_TOKEN:/m);
  assert.equal(occurrences(release, /GH_TOKEN: \$\{\{ github\.token \}\}/g), 5);
  assert.doesNotMatch(jobSection(release, "assemble"), /GH_TOKEN/);
});

test("publish promotes a QA-approved candidate and cannot rebuild bundles", () => {
  const publish = jobSection(release, "publish");
  assert.match(release, /candidate_run_id:[\s\S]*?required: false/);
  assert.match(release, /DISPATCH_CANDIDATE_RUN_ID: \$\{\{ inputs\.candidate_run_id \}\}/);
  assert.match(release, /Manual publish requires candidate_run_id as a positive Actions run ID/);
  assert.match(release, /Validate mode does not accept candidate_run_id/);
  assert.match(publish, /REQUESTED_RUN_ID: \$\{\{ inputs\.candidate_run_id \}\}/);
  assert.match(publish, /actions\/artifacts\?name=\$ARTIFACT_NAME/);
  assert.match(publish, /\.event <<< "\$run_json"\)" == "workflow_dispatch"/);
  assert.match(publish, /\.conclusion <<< "\$run_json"\)" == "success"/);
  assert.match(publish, /\.head_sha <<< "\$run_json"\)" == "\$EXPECTED_SHA"/);
  assert.match(publish, /candidate-provenance\.json/);
  assert.match(publish, /sha256sum --check --strict SHA256SUMS/);
  assert.match(publish, /find publication-assets -maxdepth 1 -type f -print0/);
  assert.doesNotMatch(publish, /pnpm (?:install|build|tauri)|cargo (?:build|test|check|clippy)|tauri-apps\/tauri-action/);
  assert.doesNotMatch(publish, /TAURI_SIGNING_PRIVATE_KEY/);
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
