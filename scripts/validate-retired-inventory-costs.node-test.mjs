import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  hasValidHistoricalMarker,
  validateRetiredInventoryCosts,
} from "./validate-retired-inventory-costs.mjs";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const snake = (...parts) => parts.join("_");
const camel = (first, ...parts) => first + parts.map(
  (part) => part[0].toUpperCase() + part.slice(1),
).join("");
const validateFixture = (root) => validateRetiredInventoryCosts(root, { enforceImmutableFiles: false });
const byteMutations = [
  ["BOM", (buffer) => Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), buffer])],
  ["NUL", (buffer) => Buffer.concat([buffer, Buffer.from([0])])],
  ["invalid UTF-8 (0xff)", (buffer) => Buffer.concat([buffer, Buffer.from([0xff])])],
];

async function fixture(t, files) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pos-retired-costs-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const [relativePath, contents] of Object.entries(files)) {
    const filePath = path.join(root, relativePath);
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, contents);
  }
  return root;
}

test("allows only the four exact purchase price fields automatically", async (t) => {
  const allowed = [
    snake("purchase", "unit", "price", "cents"),
    camel("purchase", "unit", "price", "cents"),
    snake("purchase", "total", "cents"),
    camel("purchase", "total", "cents"),
  ];
  const root = await fixture(t, { "src/runtime.ts": allowed.join("\n") });
  assert.deepEqual(await validateFixture(root), []);
});

test("rejects every purchase-concept alias while allowing only four exact fields", async (t) => {
  const rejected = [
    snake("purchase", "unit", "cost", "cents"),
    `previous_${snake("purchase", "unit", "price", "cents")}`,
    `not${camel("purchase", "unit", "price", "cents")}`,
    `${camel("purchase", "unit", "price", "cents")}Snapshot`,
    snake("purchase", "price", "cents"),
    `previous_${snake("purchase", "total", "cents")}`,
    `not${camel("purchase", "total", "cents")}`,
  ];
  const root = await fixture(t, { "src/runtime.ts": rejected.join("\n") });
  const violations = await validateFixture(root);
  assert.deepEqual([...new Set(violations.map(({ line }) => line))].sort(), [1, 2, 3, 4, 5, 6, 7]);
});

test("rejects compounds and repeated separators", async (t) => {
  const rejected = [
    camel("inventory", "cost", "cents"),
    ["legacy", "average", "cost", "cents", "snapshot"].join("__"),
    ["movement", "cost", "cents"].join("--"),
    ["unit", "price", "cents"].join("_-_"),
    `re${camel("valua", "tion")}State`,
  ];
  const root = await fixture(t, { "src/runtime.ts": rejected.join("\n") });
  assert.equal((await validateFixture(root)).length, rejected.length);
});

test("decodes hex and unicode escapes before scanning", async (t) => {
  const escaped = [
    String.raw`inventory_\x63ost_\u0063ents`,
    String.raw`movement_\u{63}ost_cents`,
    String.raw`valua\x74ion`,
  ];
  const root = await fixture(t, { "src/runtime.ts": escaped.join("\n") });
  assert.equal((await validateFixture(root)).length, escaped.length);
});

test("reconstructs nested multiline static concatenation", async (t) => {
  const source = [
    `const a = ("unit" +\n  ("__price" +\n  "--cents"));`,
    `const b = concat!("movement",\n  concat!("_cost",\n  "_cents"));`,
    `const c = "val" +\n  ("ua" +\n  ("tion"));`,
  ].join("\n");
  const root = await fixture(t, { "src/runtime.ts": source });
  assert.equal((await validateFixture(root)).length, 3);
});

test("reconstructs literal interpolation inside templates", async (t) => {
  const source = "const field = `inventory_${\n  \"cost\"\n}_cents`;\n";
  const root = await fixture(t, { "src/runtime.ts": source });
  assert.equal((await validateFixture(root)).length, 1);
});

test("scans every textual file including importable nonstandard extensions", async (t) => {
  const extensions = ["ts", "INC", "template", ""];
  const retired = camel("movement", "cost", "cents");
  const files = Object.fromEntries(extensions.map((extension, index) => [
    `src/file-${index}${extension ? `.${extension}` : ""}`,
    retired,
  ]));
  const root = await fixture(t, files);
  assert.equal((await validateFixture(root)).length, extensions.length);
});

test("skips binary files based on content rather than filename", async (t) => {
  const root = await fixture(t, { "src/native.inc": Buffer.from("movementCostCents\0binary") });
  assert.deepEqual(await validateFixture(root), []);
});

test("reconstructs multiline array literals joined with a static underscore", async (t) => {
  const source = `const field = [\n  "movement",\n  "cost",\n  "cents",\n].join("_");\n`;
  const root = await fixture(t, { "src/generated.inc": source });
  assert.equal((await validateFixture(root)).length, 1);
});

test("ignores only exact generated directories rooted at the repository", async (t) => {
  const retired = camel("movement", "cost", "cents");
  const root = await fixture(t, {
    "node_modules/root.ts": retired,
    "dist/bundle.js": retired,
    "src/node_modules/nested.ts": retired,
    "src/dist/nested.ts": retired,
  });
  const violations = await validateFixture(root);
  assert.deepEqual(violations.map(({ path: violationPath }) => violationPath), [
    "src/dist/nested.ts",
    "src/node_modules/nested.ts",
  ]);
});

test("rejects symlinks before applying exact directory ignores", async (t) => {
  const root = await fixture(t, { "outside.ts": "export {};\n" });
  await symlink(path.join(root, "outside.ts"), path.join(root, "node_modules"));
  assert.deepEqual(await validateFixture(root), [
    { symbol: "symlink", path: "node_modules", line: 1 },
  ]);
});

test("rejects file and directory symlinks", async (t) => {
  const root = await fixture(t, { "outside.ts": "export {};\n" });
  await mkdir(path.join(root, "src", "real"), { recursive: true });
  await symlink(path.join(root, "outside.ts"), path.join(root, "src", "linked.ts"));
  await symlink(path.join(root, "src", "real"), path.join(root, "src", "linked-dir"));
  assert.deepEqual(await validateFixture(root), [
    { symbol: "symlink", path: "src/linked-dir", line: 1 },
    { symbol: "symlink", path: "src/linked.ts", line: 1 },
  ]);
});

test("pins every immutable migration from 001 through 013 with full hashes", async (t) => {
  const files = {};
  const migrations = await readdir(path.join(projectRoot, "src-tauri/migrations"));
  for (let version = 1; version <= 13; version += 1) {
    const prefix = String(version).padStart(3, "0");
    const name = migrations.find((entry) => entry.startsWith(`${prefix}_`));
    files[`src-tauri/migrations/${name}`] = await readFile(path.join(projectRoot, "src-tauri/migrations", name));
  }
  const root = await fixture(t, files);
  const immutableOnly = { enforcePinnedFiles: false };
  assert.deepEqual(await validateRetiredInventoryCosts(root, immutableOnly), []);

  const changedPath = "src-tauri/migrations/006_create_sale_details.sql";
  await writeFile(path.join(root, changedPath), `${files[changedPath]}\n`);
  assert.deepEqual(await validateRetiredInventoryCosts(root, immutableOnly), [
    { symbol: "immutable-file", path: changedPath, line: 1 },
  ]);
  await writeFile(path.join(root, changedPath), files[changedPath]);

  for (const [relativePath, buffer] of Object.entries(files)) {
    for (const [name, mutate] of byteMutations) {
      await t.test(`${relativePath}: ${name}`, async () => {
        await writeFile(path.join(root, relativePath), mutate(buffer));
        assert.deepEqual(await validateRetiredInventoryCosts(root, immutableOnly), [
          { symbol: "immutable-file", path: relativePath, line: 1 },
        ]);
        await writeFile(path.join(root, relativePath), buffer);
      });
    }
  }
});

test("pins original bytes of the checkout corpus without the ignored local demo", async (t) => {
  const corpus = [
    ["historical-file", "docs/plans/inventory-movement-ux-v0.2.0-rollout.md"],
    ["historical-file", "docs/plans/product-presentations-measured-inventory.md"],
    ["pinned-reference", "README.md"],
    ["pinned-reference", "docs/plans/measured-presentations-inventory-cost-corrections.md"],
    ["pinned-reference", "docs/quality-assurance.md"],
    ["pinned-reference", "docs/releases.md"],
    ["pinned-reference", "scripts/validate-retired-inventory-costs.node-test.mjs"],
    ["pinned-reference", "src-tauri/src/db/migrations.rs"],
    ["pinned-reference", "src-tauri/src/db/mod.rs"],
    ["pinned-reference", "src-tauri/src/db/tests/migration_qa.rs"],
  ];
  const files = {};
  for (const [, relativePath] of corpus) {
    files[relativePath] = await readFile(path.join(projectRoot, relativePath));
  }
  const root = await fixture(t, files);
  const pinnedOnly = { enforceImmutableFiles: false, enforcePinnedFiles: true };
  const baseline = await validateRetiredInventoryCosts(root, pinnedOnly);
  assert.deepEqual(baseline, []);

  for (const [symbol, relativePath] of corpus) {
    const buffer = files[relativePath];
    for (const [name, mutate] of byteMutations) {
      await t.test(`${relativePath}: ${name}`, async () => {
        await writeFile(path.join(root, relativePath), mutate(buffer));
        const violations = await validateRetiredInventoryCosts(root, pinnedOnly);
        assert.deepEqual(violations.filter((violation) => violation.path === relativePath), [
          { symbol, path: relativePath, line: 1 },
        ]);
        assert.deepEqual(
          violations.filter((violation) => violation.path !== relativePath),
          baseline.filter((violation) => violation.path !== relativePath),
        );
        await writeFile(path.join(root, relativePath), buffer);
        assert.deepEqual(await validateRetiredInventoryCosts(root, pinnedOnly), baseline);
      });
    }
  }
});

test("v15 migration reference pins reject additions, replacements, removal and copied consumers", async (t) => {
  const paths = [
    "src-tauri/src/db/migrations.rs",
    "src-tauri/src/db/mod.rs",
    "src-tauri/src/db/tests/migration_qa.rs",
  ];
  const pinnedOnly = { enforceImmutableFiles: false, enforcePinnedFiles: true };
  const retired = snake("inventory", "cost", "cents");
  for (const relativePath of paths) {
    await t.test(relativePath, async (t) => {
      const contents = await readFile(path.join(projectRoot, relativePath), "utf8");
      const root = await fixture(t, { [relativePath]: contents });
      const ownViolations = async () => (await validateRetiredInventoryCosts(root, pinnedOnly))
        .filter((violation) => violation.path === relativePath);
      assert.deepEqual(await ownViolations(), []);
      for (const changed of [
        `${contents}\n// harmless drift\n`,
        `${contents}\nfn new_consumer() { ${retired}(); }\n`,
        "// replacement without retired concepts\n",
      ]) {
        await writeFile(path.join(root, relativePath), changed);
        assert.deepEqual(await ownViolations(), [
          { symbol: "pinned-reference", path: relativePath, line: 1 },
        ]);
      }
      await rm(path.join(root, relativePath));
      assert.deepEqual(await ownViolations(), [
        { symbol: "pinned-reference", path: relativePath, line: 1 },
      ]);
      const copiedPath = "src-tauri/src/services/new_consumer.rs";
      await mkdir(path.dirname(path.join(root, copiedPath)), { recursive: true });
      await writeFile(path.join(root, copiedPath), contents);
      assert.ok((await validateFixture(root)).some((violation) => (
        violation.path === copiedPath
      )));
    });
  }
});

test("relocated purchase correction and UI absence probes remain exact exceptions", async (t) => {
  for (const relativePath of [
    "src-tauri/src/services/inventory.rs",
    "src/features/inventory/InventoryView.test.tsx",
  ]) {
    await t.test(relativePath, async (t) => {
      const contents = await readFile(path.join(projectRoot, relativePath), "utf8");
      const root = await fixture(t, { [relativePath]: contents });
      assert.deepEqual(await validateFixture(root), []);

      // No relaxation for matching context at a different position or path.
      await writeFile(path.join(root, relativePath), `\n${contents}`);
      assert.ok((await validateFixture(root)).length > 0);
      await writeFile(path.join(root, relativePath), contents);
      const copiedPath = "src/new-consumer.ts";
      await mkdir(path.dirname(path.join(root, copiedPath)), { recursive: true });
      await writeFile(path.join(root, copiedPath), contents);
      assert.ok((await validateFixture(root)).some(({ path: entryPath }) => entryPath === copiedPath));
      await rm(path.join(root, copiedPath));

      const target = relativePath.endsWith(".rs")
        ? "previous_purchase_unit_price_cents: previous_price,"
        : 'it("removes valuation and cost UI and exposes two separate actions"';
      assert.ok(contents.includes(target));
      await writeFile(path.join(root, relativePath), contents.replace(target, `${target} /* changed context */`));
      assert.ok((await validateFixture(root)).length > 0);

      // A new symbol in the same file is never covered by the old exceptions.
      await writeFile(path.join(root, relativePath), `${contents}\n${snake("inventory", "cost", "cents")}\n`);
      assert.deepEqual(await validateFixture(root), [{
        symbol: snake("inventory", "cost", "cents"), path: relativePath,
        line: contents.split("\n").length + 1, column: 1,
      }]);
    });
  }
});

test("missing optional demo does not weaken required reference checks", async (t) => {
  const root = await fixture(t, {});
  const violations = await validateRetiredInventoryCosts(root);
  assert.ok(violations.some(({ symbol, path: relativePath }) => (
    symbol === "pinned-reference" && relativePath === "README.md"
  )));
  assert.ok(!violations.some(({ path: relativePath }) => relativePath === "docs/demo.html"));
});

test("accepts only the original optional demo bytes, rejecting byte mutations", async (t) => {
  const relativePath = "docs/demo.html";
  let buffer;
  try {
    buffer = await readFile(path.join(projectRoot, relativePath));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    t.skip("The optional local corpus is absent in this checkout.");
    return;
  }
  const root = await fixture(t, { [relativePath]: buffer });
  assert.deepEqual(await validateFixture(root), []);
  for (const [name, mutate] of byteMutations) {
    await t.test(name, async () => {
      await writeFile(path.join(root, relativePath), mutate(buffer));
      assert.deepEqual(await validateFixture(root), [
        { symbol: "optional-reference", path: relativePath, line: 1 },
      ]);
      await writeFile(path.join(root, relativePath), buffer);
      assert.deepEqual(await validateFixture(root), []);
    });
  }
});

test("rejects replacement demo bytes even without retired symbols or readable text", async (t) => {
  const relativePath = "docs/demo.html";
  const replacement = Buffer.from("<html>Replacement corpus</html>\n");
  const root = await fixture(t, { [relativePath]: replacement });
  for (const buffer of [replacement, ...byteMutations.map(([, mutate]) => mutate(replacement))]) {
    await writeFile(path.join(root, relativePath), buffer);
    assert.deepEqual(await validateFixture(root), [
      { symbol: "optional-reference", path: relativePath, line: 1 },
    ]);
  }
});

test("the optional reference does not exempt other HTML source files", async (t) => {
  const retired = camel("movement", "cost", "cents");
  const root = await fixture(t, {
    "docs/demo.html": retired,
    "docs/guide.html": retired,
    "src/demo.html": retired,
  });
  const violations = await validateRetiredInventoryCosts(root);
  assert.deepEqual(violations.filter(({ path: entryPath }) => entryPath === "docs/demo.html"), [
    { symbol: "optional-reference", path: "docs/demo.html", line: 1 },
  ]);
  for (const relativePath of ["docs/guide.html", "src/demo.html"]) {
    assert.deepEqual(violations.filter(({ path: entryPath }) => entryPath === relativePath), [
      { symbol: snake("movement", "cost", "cents"), path: relativePath, line: 1, column: 1 },
    ]);
  }
});

test("historical retirement markers reject explicit contradictions", () => {
  const marker = "OLD MODEL RETIRED";
  assert.equal(hasValidHistoricalMarker(`${marker}\nHistorical details.`, marker), true);
  assert.equal(hasValidHistoricalMarker("Historical details.", marker), false);
  assert.equal(hasValidHistoricalMarker(`${marker}\nEl modelo de costo sigue vigente.`, marker), false);
  assert.equal(hasValidHistoricalMarker(`${marker}\nLa valoracion no queda reemplazada.`, marker), false);
});

test("active exceptions bind the exact path, line, and context fingerprint", async (t) => {
  const relativePath = "src-tauri/src/models/inventory_movement.rs";
  const contents = await readFile(path.join(projectRoot, relativePath), "utf8");
  const root = await fixture(t, { [relativePath]: contents });
  assert.deepEqual(await validateFixture(root), []);

  await writeFile(path.join(root, relativePath), `\n${contents}`);
  assert.ok((await validateFixture(root)).length >= 2);

  await writeFile(path.join(root, relativePath), contents.replace(
    "pub previous_purchase_unit_price_cents: i64,",
    "pub previous_purchase_unit_price_cents: i64, // changed context",
  ));
  assert.ok((await validateFixture(root)).length >= 2);
});
