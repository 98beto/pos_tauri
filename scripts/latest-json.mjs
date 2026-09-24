import { readdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { decodeTauriSignature } from "./tauri-signature-format.mjs";

const REPOSITORY = "98beto/pos_tauri";
const TAG_PATTERN = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const ASSET_NAME_PATTERN = /^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?$/;
const PLATFORM_KEYS = ["linux-x86_64", "windows-x86_64"];
const EXPECTED_ASSET_COUNT = 7;

async function filesIn(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(entries.map(async (entry) => {
    const entryPath = path.join(directory, entry.name);
    return entry.isDirectory() ? filesIn(entryPath) : [entryPath];
  }));
  return nested.flat();
}

function exactlyOne(files, description, predicate) {
  const matches = files.filter(predicate);
  if (matches.length !== 1) {
    throw new Error(`Expected exactly one ${description}, found ${matches.length}`);
  }
  return matches[0];
}

function releaseUrl(tag, filePath) {
  const assetName = encodeURIComponent(path.basename(filePath))
    .replace(/[!'()*]/g, (character) => `%${character.charCodeAt(0).toString(16).toUpperCase()}`);
  return `https://github.com/${REPOSITORY}/releases/download/${tag}/${assetName}`;
}

function assertTag(tag) {
  if (!TAG_PATTERN.test(tag ?? "")) {
    throw new Error(`Invalid release tag "${tag ?? ""}"; expected vX.Y.Z`);
  }
}

export async function inspectReleaseAssets(assetsDirectory) {
  const paths = await filesIn(assetsDirectory);
  const files = paths.filter((file) => path.basename(file) !== "latest.json");
  const names = files.map((file) => path.basename(file));

  const unsafeName = names.find((name) => !ASSET_NAME_PATTERN.test(name));
  if (unsafeName) {
    throw new Error(
      `Unsafe release asset filename ${JSON.stringify(unsafeName)}; names must start and end with A-Z, a-z, or 0-9, with only dot, underscore, and hyphen also allowed inside`,
    );
  }
  if (new Set(names).size !== names.length) {
    throw new Error("Release asset filenames must be unique");
  }
  if (names.some((name) => name.endsWith(".tar.gz") || name.endsWith(".nsis.zip"))) {
    throw new Error("Legacy updater archives are not valid Tauri 2 native assets");
  }
  if (files.length !== EXPECTED_ASSET_COUNT) {
    throw new Error(`Expected exactly ${EXPECTED_ASSET_COUNT} release assets, found ${files.length}`);
  }

  const appImage = exactlyOne(files, "AppImage", (file) => file.endsWith(".AppImage"));
  const appImageSignature = exactlyOne(
    files,
    "AppImage signature",
    (file) => file === `${appImage}.sig`,
  );
  const nsis = exactlyOne(files, "NSIS setup executable", (file) => /-setup\.exe$/i.test(file));
  const nsisSignature = exactlyOne(
    files,
    "NSIS signature",
    (file) => file === `${nsis}.sig`,
  );

  exactlyOne(files, "Debian package", (file) => file.endsWith(".deb"));
  exactlyOne(files, "RPM package", (file) => file.endsWith(".rpm"));
  exactlyOne(files, "MSI package", (file) => file.endsWith(".msi"));

  const [linuxSignature, windowsSignature] = await Promise.all([
    readFile(appImageSignature, "utf8"),
    readFile(nsisSignature, "utf8"),
  ]);
  decodeTauriSignature(linuxSignature);
  decodeTauriSignature(windowsSignature);

  return {
    appImage,
    appImageSignature,
    linuxSignature,
    nsis,
    nsisSignature,
    windowsSignature,
  };
}

export async function generateLatestJson({
  tag,
  assetsDirectory,
  notes,
  pubDate = new Date().toISOString(),
}) {
  assertTag(tag);
  const assets = await inspectReleaseAssets(assetsDirectory);
  const date = new Date(pubDate);
  if (Number.isNaN(date.valueOf())) {
    throw new Error("pub_date must be a valid date");
  }

  const manifest = {
    version: tag.slice(1),
    notes: typeof notes === "string" && notes.trim() ? notes.trim() : `Release ${tag}`,
    pub_date: date.toISOString(),
    platforms: {
      "linux-x86_64": {
        signature: assets.linuxSignature,
        url: releaseUrl(tag, assets.appImage),
      },
      "windows-x86_64": {
        signature: assets.windowsSignature,
        url: releaseUrl(tag, assets.nsis),
      },
    },
  };
  await validateLatestJson({ tag, assetsDirectory, manifest });
  return manifest;
}

export async function validateLatestJson({ tag, assetsDirectory, manifest }) {
  assertTag(tag);
  const assets = await inspectReleaseAssets(assetsDirectory);
  if (manifest.version !== tag.slice(1)) {
    throw new Error(`latest.json version must equal ${tag.slice(1)}`);
  }
  if (typeof manifest.notes !== "string" || !manifest.notes.trim()) {
    throw new Error("latest.json notes must not be empty");
  }
  if (Number.isNaN(new Date(manifest.pub_date).valueOf())) {
    throw new Error("latest.json pub_date must be a valid date");
  }
  if (
    !manifest.platforms
    || Object.keys(manifest.platforms).sort().join(",") !== [...PLATFORM_KEYS].sort().join(",")
  ) {
    throw new Error(`latest.json must contain only ${PLATFORM_KEYS.join(" and ")}`);
  }

  const expected = {
    "linux-x86_64": {
      signature: assets.linuxSignature,
      url: releaseUrl(tag, assets.appImage),
    },
    "windows-x86_64": {
      signature: assets.windowsSignature,
      url: releaseUrl(tag, assets.nsis),
    },
  };
  for (const platform of PLATFORM_KEYS) {
    const entry = manifest.platforms[platform];
    if (!entry?.signature || entry.signature !== expected[platform].signature) {
      throw new Error(`${platform} signature does not match its native updater asset`);
    }
    if (entry.url !== expected[platform].url) {
      throw new Error(`${platform} URL does not reference the expected release asset`);
    }
    if (/\.msi(?:$|[?#])/i.test(entry.url)) {
      throw new Error("MSI must not be used as an updater asset");
    }
  }

  return true;
}

function optionsFromArguments(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    if (!args[index]?.startsWith("--") || args[index + 1] === undefined) {
      throw new Error(`Invalid argument ${args[index] ?? ""}`);
    }
    options[args[index].slice(2)] = args[index + 1];
  }
  return options;
}

const isMain = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  try {
    const command = process.argv[2];
    const options = optionsFromArguments(process.argv.slice(3));
    const manifestPath = options.manifest ?? options.output;
    if (!options.tag || !options.assets || !manifestPath) {
      throw new Error("--tag, --assets, and --manifest/--output are required");
    }

    if (command === "generate") {
      const notes = options["notes-file"]
        ? await readFile(options["notes-file"], "utf8")
        : undefined;
      const manifest = await generateLatestJson({
        tag: options.tag,
        assetsDirectory: options.assets,
        notes,
        pubDate: options["pub-date"],
      });
      await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    } else if (command === "validate") {
      const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
      await validateLatestJson({
        tag: options.tag,
        assetsDirectory: options.assets,
        manifest,
      });
    } else {
      throw new Error("Expected generate or validate command");
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
