import { createHash } from "node:crypto";
import { lstat, readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const snake = (...parts) => parts.join("_");
const SYMBOLS = {
  inventory: snake("inventory", "cost", "cents"),
  average: snake("average", "cost"),
  movement: snake("movement", "cost", "cents"),
  before: snake("inventory", "cost", "before", "cents"),
  after: snake("inventory", "cost", "after", "cents"),
  unitCost: snake("unit", "cost", "cents"),
  unitPrice: snake("unit", "price", "cents"),
  spanishTerm: ["valora", "cion"].join(""),
  englishTerm: ["valua", "tion"].join(""),
};

const compound = (...parts) => parts.join("(?:[_-]+)?");
const RETIRED_PATTERNS = [
  [SYMBOLS.before, new RegExp(compound("inventory", "cost", "before", "cents"), "giu")],
  [SYMBOLS.after, new RegExp(compound("inventory", "cost", "after", "cents"), "giu")],
  [SYMBOLS.inventory, new RegExp(compound("inventory", "cost", "cents"), "giu")],
  [SYMBOLS.average, new RegExp(`${compound("average", "cost")}(?:[_-]+cents)?`, "giu")],
  [SYMBOLS.movement, new RegExp(compound("movement", "cost", "cents"), "giu")],
  [SYMBOLS.unitCost, new RegExp(compound("unit", "cost", "cents"), "giu")],
  [SYMBOLS.unitPrice, new RegExp(compound("unit", "price", "cents"), "giu")],
  [SYMBOLS.spanishTerm, new RegExp("valoraci[oó]n", "giu")],
  [SYMBOLS.englishTerm, new RegExp(SYMBOLS.englishTerm, "giu")],
];

const PURCHASE_FIELDS = new RegExp(
  `(?<![\\p{L}\\p{N}_])(?:${snake("purchase", "unit", "price", "cents")}|purchaseUnitPriceCents|${snake("purchase", "total", "cents")}|purchaseTotalCents)(?![\\p{L}\\p{N}_])`,
  "gu",
);
const PURCHASE_CONCEPTS = [
  [snake("purchase", "unit", "price", "cents"), /purchase(?:[_-]+)?unit(?:[_-]+)?price(?:[_-]+)?cents/giu],
  [snake("purchase", "total", "cents"), /purchase(?:[_-]+)?total(?:[_-]+)?cents/giu],
  [snake("purchase", "price", "cents"), /purchase(?:[_-]+)?price(?:[_-]+)?cents/giu],
];
// This is a source-level symbol/concept guard, not a parser or dynamic data-flow analysis.
// dist and the other exact roots are generated dependencies/output, not source.
// Names below src/** are deliberately never ignored.
const IGNORED_GENERATED_ROOTS = new Set([".git", "dist", "node_modules", "target", "src-tauri/target"]);

// Migrations 001-012 have shipped. Any byte-level change is a gate failure, even if it
// does not contain a retired symbol.
const IMMUTABLE_FILES = new Map([
  ["src-tauri/migrations/001_create_users.sql", "847244dee2c0dde13120abfe8c2cd01ef81f8ce9e5937daaab311255d74a75d0"],
  ["src-tauri/migrations/002_create_brands.sql", "6aef237d6069e61d7e780db2bc169ea7ab1b7985dbf2b8fdaa93de55f8eecd99"],
  ["src-tauri/migrations/003_create_categories.sql", "c4f2a6c7e64dd802e4b5121944b7d8db3d6f4d068095f92e0dc7fadcb125923c"],
  ["src-tauri/migrations/004_create_products.sql", "494786996905da6839409b62cfe720757e4913719be23175c996dd0616bdead6"],
  ["src-tauri/migrations/005_create_sales.sql", "d119172ab923ca8ad9f5f347f74b4f58ff4916773187270baff8521b3158e1f1"],
  ["src-tauri/migrations/006_create_sale_details.sql", "fd3c50a7d988aa1de49c73d9b9577c12f2b9ea0f79646217ca29e2ff4c47a8df"],
  ["src-tauri/migrations/007_create_inventory_movements.sql", "a589f03ddd9a5e9afc51dd1cc58a2915fc35f57490e61f23c6660cbad631bad2"],
  ["src-tauri/migrations/008_create_indexes.sql", "e2cbba959454aeeba081f70044be4466714480da789a9db80c5b55afe1cc4444"],
  ["src-tauri/migrations/009_create_product_presentations.sql", "2eaa90e11c39e58d64e6e4dfb9b8d91c3a90367a210c4fb5e1a898d10b03f786"],
  ["src-tauri/migrations/010_rebuild_sale_details.sql", "ed7ae6cb9652cad3ea16ea9bce37b6d2f60e41b6001047bd7f6d94ec677ae1d5"],
  ["src-tauri/migrations/011_rebuild_inventory_movements.sql", "6ab324ab5c4453880ea9d5852a99167efd8c51d9b71cee253e0a51b9ff117fc6"],
  ["src-tauri/migrations/012_add_sale_pricing_and_discounts.sql", "dfb071ef4f53a9bf923b5c9d5a10d9a10f9eec1c261e8b8ba9caa1a503225a1a"],
]);

const HISTORICAL_FILES = new Map([
  ["docs/plans/inventory-movement-ux-v0.2.0-rollout.md", {
    sha256: "7f986d50715cd0328e6e4c7e817ceeb51a0d6e95056239a56ead3a9de3994481",
    marker: `**Evolucion:** las decisiones de costo y ${SYMBOLS.spanishTerm} son historicas y quedan reemplazadas. El plan vigente [\`measured-presentations-inventory-cost-corrections.md\`](./measured-presentations-inventory-cost-corrections.md) conserva inventario fisico, separa compras de conteos y retira el costo acumulado.`,
  }],
  ["docs/plans/product-presentations-measured-inventory.md", {
    sha256: "ed4cc984a6a0044fd0e17e05cd182ba989876199e809cd820d23d4db7fc6bb93",
    marker: `**Evolucion:** el modelo de costo promedio y ${SYMBOLS.spanishTerm} queda reemplazado. [\`measured-presentations-inventory-cost-corrections.md\`](./measured-presentations-inventory-cost-corrections.md) conserva presentaciones medidas y centimetros, pero separa compras de conteos fisicos y retira el costo acumulado.`,
  }],
]);

const PINNED_REFERENCE_FILES = new Map([
  ["README.md", "045f9255f1b9550cfb6f2f078a0ab61dd247312538601e1e8d24f70a51a626b3"],
  ["docs/demo.html", "2c93226cd014faaafc1a63f4c016f20f5636cd5d303f9ea7c012c5a1137dfa6f"],
  ["docs/plans/measured-presentations-inventory-cost-corrections.md", "43e86deb8a8ebde7561cf48a16ceffac8bf83343c627571407cfae6622d70bc6"],
  ["docs/quality-assurance.md", "4f1caf23b223fb6d92fcee0609d34dcd16e08cc99f89f8cadb9091851b1d4ec7"],
  ["docs/releases.md", "467b4dd31a27ee9775479ac4cedbb30e7dd9a80e95d0b272d8850e355ae5e8ab"],
  // The test suite contains intentional evasions and is itself an immutable reproduction corpus.
  ["scripts/validate-retired-inventory-costs.node-test.mjs", "010570bbebcef031507e08eeab46e005c680e50fa75952b5ee05c740d1e8ec70"],
  // These files retain the shipped-schema compatibility and migration QA corpus.
  ["src-tauri/src/db/migrations.rs", "353a4b5226533e325983d53f97f833c75a99b15ce89782877a6af75801975fe2"],
  ["src-tauri/src/db/mod.rs", "baefbaf239965cb4f35f288613f727e19e0235e339c821c2e156bd21d07b50dd"],
  ["src-tauri/src/db/tests/migration_qa.rs", "09207bedd4aae147608c3befadbb299fa6db898ebf0be7172b73b384e126c0bf"],
]);

// Active-code exceptions are intentionally explicit. Each entry identifies one symbol occurrence
// by source position and a full SHA-256 of its surrounding three-line context.
const ACTIVE_EXCEPTIONS = [
  { symbol: SYMBOLS.spanishTerm, path: "scripts/validate-retired-inventory-costs.mjs", line: 15, column: 16, fingerprint: "5d2b0eef4af0b305a44bb60b8084bde78e8b169ff1b7b463105c28f2de2b0f50" },
  { symbol: SYMBOLS.englishTerm, path: "scripts/validate-retired-inventory-costs.mjs", line: 16, column: 16, fingerprint: "36cd84a4523502ba1b6b9bdd8f9020325c89da971dd05c1b864044133310c7e7" },
  { symbol: SYMBOLS.spanishTerm, path: "src-tauri/migrations/013_measured_presentations_centimeters.sql", line: 195, column: 45, fingerprint: "117ef0bc50790ab0cec6ad968339f27a4fb6d333d0794ddabdaa4e2103f35661" },
  { symbol: SYMBOLS.spanishTerm, path: "src-tauri/migrations/013_measured_presentations_centimeters.sql", line: 195, column: 68, fingerprint: "117ef0bc50790ab0cec6ad968339f27a4fb6d333d0794ddabdaa4e2103f35661" },
  { symbol: SYMBOLS.unitPrice, path: "src-tauri/src/models/inventory_movement.rs", line: 227, column: 27, fingerprint: "005a634b455b077ea0204651ba043b207aaebdb357b6a4355634c16d431fafb1" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src-tauri/src/models/inventory_movement.rs", line: 227, column: 18, fingerprint: "005a634b455b077ea0204651ba043b207aaebdb357b6a4355634c16d431fafb1" },
  { symbol: SYMBOLS.unitPrice, path: "src-tauri/src/models/inventory_movement.rs", line: 228, column: 22, fingerprint: "44c98961bb4d5b65a9a1171a2c2788738ed726fa1711be907db00e3b800fda66" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src-tauri/src/models/inventory_movement.rs", line: 228, column: 13, fingerprint: "44c98961bb4d5b65a9a1171a2c2788738ed726fa1711be907db00e3b800fda66" },
  { symbol: snake("purchase", "total", "cents"), path: "src-tauri/src/models/inventory_movement.rs", line: 229, column: 18, fingerprint: "87fb57171213acb0072eb5f263f9b7a8d0a207541dabaaea1b3b36c0074943b0" },
  { symbol: snake("purchase", "total", "cents"), path: "src-tauri/src/models/inventory_movement.rs", line: 230, column: 13, fingerprint: "00ee7b06c53563596632ce61d94c7291a06602643242fc2b061cdb469cf6e447" },
  { symbol: snake("purchase", "total", "cents"), path: "src-tauri/src/services/inventory.rs", line: 598, column: 23, fingerprint: "9c93cf2a8f171f9989f4f7999bffecbf4a5f79cff7b835bff9a0a8fd75dea580" },
  { symbol: SYMBOLS.unitPrice, path: "src-tauri/src/services/inventory.rs", line: 1079, column: 27, fingerprint: "3dbfacb06dcc7252420e83419b2b240665d214192d70cabc2d1cb0e53ca956d9" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src-tauri/src/services/inventory.rs", line: 1079, column: 18, fingerprint: "3dbfacb06dcc7252420e83419b2b240665d214192d70cabc2d1cb0e53ca956d9" },
  { symbol: SYMBOLS.unitPrice, path: "src-tauri/src/services/inventory.rs", line: 1080, column: 22, fingerprint: "51547739cd6a285afc59fbb62374f975fc9ca69f8a201b01e32f9f4b1ccd2796" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src-tauri/src/services/inventory.rs", line: 1080, column: 13, fingerprint: "51547739cd6a285afc59fbb62374f975fc9ca69f8a201b01e32f9f4b1ccd2796" },
  { symbol: snake("purchase", "total", "cents"), path: "src-tauri/src/services/inventory.rs", line: 1081, column: 18, fingerprint: "1fccb8a1e10fb3437bd1f77820f6781759f529a69a9af52cbf9673e82d49c8a5" },
  { symbol: snake("purchase", "total", "cents"), path: "src-tauri/src/services/inventory.rs", line: 1082, column: 13, fingerprint: "57ffaeeeb577382845a7f50d121c88fc3b6332996f50e1a417a223e41fa9ba67" },
  { symbol: snake("purchase", "total", "cents"), path: "src-tauri/src/services/inventory.rs", line: 1690, column: 35, fingerprint: "7366a0a04a2f3ecab6b8d57b49f95d1d6983fc08c0acabc9bf5501d5a20a7f41" },
  { symbol: snake("purchase", "total", "cents"), path: "src-tauri/src/services/inventory.rs", line: 2159, column: 32, fingerprint: "8fd7eed8fd5816144fdfdf40889a444f9359b0fcf1b655351f4bd79e2d0087ef" },
  { symbol: SYMBOLS.englishTerm, path: "src/features/inventory/InventoryView.test.tsx", line: 232, column: 15, fingerprint: "86409642a1fa3041e305c93ce8f250c38e718e4f4b5fb70690dceab7e1e0a2f6" },
  { symbol: SYMBOLS.spanishTerm, path: "src/features/inventory/InventoryView.test.tsx", line: 236, column: 120, fingerprint: "4ee1ae444697c1b7e803e809efa47b4fcf2d2aba890b70bf521511fdc5640928" },
  { symbol: SYMBOLS.englishTerm, path: "src/features/inventory/manual-movement-intent.test.ts", line: 105, column: 32, fingerprint: "75df0991c15d5a4598c3b3fe79d5e277138de871112dfe9829ee63853d6d5e9d" },
  { symbol: SYMBOLS.spanishTerm, path: "src/features/inventory/manual-movement-intent.test.ts", line: 108, column: 54, fingerprint: "7f7d4f8442f88640b0442ab9f8ddc8cf19bb0045da32ab2fc6f31eb3ebe12ad0" },
  { symbol: SYMBOLS.spanishTerm, path: "src/features/inventory/manual-movement-intent.test.ts", line: 108, column: 74, fingerprint: "7f7d4f8442f88640b0442ab9f8ddc8cf19bb0045da32ab2fc6f31eb3ebe12ad0" },
  { symbol: SYMBOLS.unitPrice, path: "src/features/inventory/ProductInventoryDetail.test.tsx", line: 21, column: 355, fingerprint: "8701e840e4cd690582d526ad2b9d05d393c3b3d88a295b0d401ba176be5c604c" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src/features/inventory/ProductInventoryDetail.test.tsx", line: 21, column: 346, fingerprint: "8701e840e4cd690582d526ad2b9d05d393c3b3d88a295b0d401ba176be5c604c" },
  { symbol: SYMBOLS.unitPrice, path: "src/features/inventory/ProductInventoryDetail.test.tsx", line: 21, column: 392, fingerprint: "8701e840e4cd690582d526ad2b9d05d393c3b3d88a295b0d401ba176be5c604c" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src/features/inventory/ProductInventoryDetail.test.tsx", line: 21, column: 383, fingerprint: "8701e840e4cd690582d526ad2b9d05d393c3b3d88a295b0d401ba176be5c604c" },
  { symbol: snake("purchase", "total", "cents"), path: "src/features/inventory/ProductInventoryDetail.test.tsx", line: 21, column: 425, fingerprint: "8701e840e4cd690582d526ad2b9d05d393c3b3d88a295b0d401ba176be5c604c" },
  { symbol: snake("purchase", "total", "cents"), path: "src/features/inventory/ProductInventoryDetail.test.tsx", line: 21, column: 457, fingerprint: "8701e840e4cd690582d526ad2b9d05d393c3b3d88a295b0d401ba176be5c604c" },
  { symbol: SYMBOLS.englishTerm, path: "src/features/inventory/ProductInventoryDetail.test.tsx", line: 41, column: 64, fingerprint: "766ae624057620849494acc1f1f132e69cc56f523353fe01244c7ab9a93ff33f" },
  { symbol: SYMBOLS.unitPrice, path: "src/features/inventory/PurchaseCostCorrection.test.tsx", line: 15, column: 345, fingerprint: "13dc001d22fdfb9ff3b7fbb851b9bcb087a7fc3818ddc8d42c23ebee1cef8f37" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src/features/inventory/PurchaseCostCorrection.test.tsx", line: 15, column: 336, fingerprint: "13dc001d22fdfb9ff3b7fbb851b9bcb087a7fc3818ddc8d42c23ebee1cef8f37" },
  { symbol: SYMBOLS.unitPrice, path: "src/features/inventory/PurchaseCostCorrection.test.tsx", line: 15, column: 382, fingerprint: "13dc001d22fdfb9ff3b7fbb851b9bcb087a7fc3818ddc8d42c23ebee1cef8f37" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src/features/inventory/PurchaseCostCorrection.test.tsx", line: 15, column: 373, fingerprint: "13dc001d22fdfb9ff3b7fbb851b9bcb087a7fc3818ddc8d42c23ebee1cef8f37" },
  { symbol: snake("purchase", "total", "cents"), path: "src/features/inventory/PurchaseCostCorrection.test.tsx", line: 15, column: 415, fingerprint: "13dc001d22fdfb9ff3b7fbb851b9bcb087a7fc3818ddc8d42c23ebee1cef8f37" },
  { symbol: snake("purchase", "total", "cents"), path: "src/features/inventory/PurchaseCostCorrection.test.tsx", line: 15, column: 447, fingerprint: "13dc001d22fdfb9ff3b7fbb851b9bcb087a7fc3818ddc8d42c23ebee1cef8f37" },
  { symbol: SYMBOLS.unitPrice, path: "src/features/inventory/PurchaseCostCorrection.tsx", line: 66, column: 621, fingerprint: "3ab7586a52feaed40d51e5b1b4b6e8589678f7eaf30647574a767308d17bdc5f" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src/features/inventory/PurchaseCostCorrection.tsx", line: 66, column: 612, fingerprint: "3ab7586a52feaed40d51e5b1b4b6e8589678f7eaf30647574a767308d17bdc5f" },
  { symbol: SYMBOLS.unitPrice, path: "src/features/inventory/PurchaseCostCorrection.tsx", line: 66, column: 674, fingerprint: "3ab7586a52feaed40d51e5b1b4b6e8589678f7eaf30647574a767308d17bdc5f" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src/features/inventory/PurchaseCostCorrection.tsx", line: 66, column: 665, fingerprint: "3ab7586a52feaed40d51e5b1b4b6e8589678f7eaf30647574a767308d17bdc5f" },
  { symbol: snake("purchase", "total", "cents"), path: "src/features/inventory/PurchaseCostCorrection.tsx", line: 66, column: 890, fingerprint: "3ab7586a52feaed40d51e5b1b4b6e8589678f7eaf30647574a767308d17bdc5f" },
  { symbol: snake("purchase", "total", "cents"), path: "src/features/inventory/PurchaseCostCorrection.tsx", line: 66, column: 938, fingerprint: "3ab7586a52feaed40d51e5b1b4b6e8589678f7eaf30647574a767308d17bdc5f" },
  { symbol: SYMBOLS.unitPrice, path: "src/types/inventory_movement.ts", line: 178, column: 21, fingerprint: "fa1197f4b757a865a36917aa81d3d5057239e7b4a328c99e47798ef6877200e9" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src/types/inventory_movement.ts", line: 178, column: 12, fingerprint: "fa1197f4b757a865a36917aa81d3d5057239e7b4a328c99e47798ef6877200e9" },
  { symbol: SYMBOLS.unitPrice, path: "src/types/inventory_movement.ts", line: 179, column: 16, fingerprint: "6741f2fbf093da4e92631d53c30026c15a7a68c457a66845bc5111d7206fab30" },
  { symbol: snake("purchase", "unit", "price", "cents"), path: "src/types/inventory_movement.ts", line: 179, column: 7, fingerprint: "6741f2fbf093da4e92631d53c30026c15a7a68c457a66845bc5111d7206fab30" },
  { symbol: snake("purchase", "total", "cents"), path: "src/types/inventory_movement.ts", line: 180, column: 12, fingerprint: "53286a106fbd29fbd4c97bde410c93190320adcdf4691969220a94a53966dc24" },
  { symbol: snake("purchase", "total", "cents"), path: "src/types/inventory_movement.ts", line: 181, column: 7, fingerprint: "e85d68dd57ac279e6217bfa6d5ee880b926ff474cfb046efb7b85ee8e7c3b59f" },
];

const sha256 = (value) => createHash("sha256").update(value).digest("hex");
const normalizeContext = (value) => value.trim().replace(/\s+/gu, " ");

function sourcePosition(contents, index) {
  const before = contents.slice(0, index);
  const line = before.split("\n").length;
  const lineStart = before.lastIndexOf("\n") + 1;
  return { line, column: index - lineStart + 1 };
}

function contextFingerprint(contents, index) {
  const lines = contents.split(/\r?\n/u);
  const { line } = sourcePosition(contents, index);
  const context = lines.slice(Math.max(0, line - 2), line + 1).map(normalizeContext).join("\n");
  return sha256(context);
}

function decodeEscapes(contents) {
  let text = "";
  const sourceIndexes = [];
  for (let index = 0; index < contents.length;) {
    const rest = contents.slice(index);
    const escape = rest.match(/^\\(?:x([\da-f]{2})|u\{([\da-f]{1,6})\}|u([\da-f]{4}))/iu);
    if (!escape) {
      text += contents[index];
      sourceIndexes.push(index);
      index += 1;
      continue;
    }
    const codePoint = Number.parseInt(escape[1] ?? escape[2] ?? escape[3], 16);
    const decoded = codePoint <= 0x10ffff ? String.fromCodePoint(codePoint) : escape[0];
    text += decoded;
    sourceIndexes.push(...Array(decoded.length).fill(index));
    index += escape[0].length;
  }
  sourceIndexes.push(contents.length);
  return { text, sourceIndexes };
}

function purchaseFieldSpans(text) {
  return [...text.matchAll(PURCHASE_FIELDS)].map((match) => [match.index, match.index + match[0].length]);
}

function scanText(text, sourceContents, sourceIndexes, found) {
  const purchaseSpans = purchaseFieldSpans(text);
  for (const [symbol, pattern] of PURCHASE_CONCEPTS) {
    pattern.lastIndex = 0;
    for (const match of text.matchAll(pattern)) {
      const allowed = purchaseSpans.some(([start, end]) => match.index === start && match.index + match[0].length === end);
      if (allowed) continue;
      const sourceIndex = sourceIndexes[Math.min(match.index, sourceIndexes.length - 1)];
      found.push({ symbol, ...sourcePosition(sourceContents, sourceIndex), fingerprint: contextFingerprint(sourceContents, sourceIndex) });
    }
  }
  for (const [symbol, pattern] of RETIRED_PATTERNS) {
    pattern.lastIndex = 0;
    for (const match of text.matchAll(pattern)) {
      if ((symbol === SYMBOLS.unitPrice || symbol === SYMBOLS.unitCost)
          && purchaseSpans.some(([start, end]) => match.index >= start && match.index + match[0].length <= end)) continue;
      const sourceIndex = sourceIndexes[Math.min(match.index, sourceIndexes.length - 1)];
      const position = sourcePosition(sourceContents, sourceIndex);
      found.push({
        symbol,
        ...position,
        fingerprint: contextFingerprint(sourceContents, sourceIndex),
      });
    }
  }
}

function decodeLiteralBody(body) {
  return decodeEscapes(body).text
    .replace(/\\([\\'"`])/gu, "$1")
    .replace(/\\[nrt]/gu, " ");
}

function staticLiteralTokens(expression, sourceOffset) {
  const tokens = [];
  const pattern = /"((?:\\.|[^"\\])*)"|'((?:\\.|[^'\\])*)'/gu;
  for (const match of expression.matchAll(pattern)) {
    tokens.push({ value: decodeLiteralBody(match[1] ?? match[2]), start: sourceOffset + match.index, end: sourceOffset + match.index + match[0].length });
  }
  return tokens;
}

function isStaticGlue(value, allowComma) {
  const punctuation = allowComma ? "\\s|\\+|\\||,|\\(|\\)|\\.|!|\\$|\\{|\\}" : "\\s|\\+|\\||\\(|\\)|\\.|!|\\$|\\{|\\}";
  return new RegExp(`^(?:(?:${punctuation})|concat)*$`, "iu").test(value);
}

function scanStaticExpression(expression, sourceContents, sourceOffset, found) {
  const tokens = staticLiteralTokens(expression, sourceOffset);
  const allowComma = /(?:concat!\s*\(|\.concat\s*\()/iu.test(expression);
  for (let start = 0; start < tokens.length; start += 1) {
    let value = tokens[start].value;
    for (let end = start + 1; end < tokens.length; end += 1) {
      const gapStart = tokens[end - 1].end - sourceOffset;
      const gapEnd = tokens[end].start - sourceOffset;
      if (!isStaticGlue(expression.slice(gapStart, gapEnd), allowComma)) break;
      value += tokens[end].value;
      const indexes = Array(value.length + 1).fill(tokens[start].start);
      scanText(value, sourceContents, indexes, found);
    }
  }
}

function scanStaticConstructions(contents, found) {
  scanStaticExpression(contents, contents, 0, found);
  for (const template of contents.matchAll(/`((?:\\.|[^`\\])*)`/gu)) {
      let value = "";
      let cursor = 0;
      let valid = true;
      for (const interpolation of template[1].matchAll(/\$\{([^{}]*)\}/gu)) {
        value += decodeLiteralBody(template[1].slice(cursor, interpolation.index));
        const tokens = staticLiteralTokens(interpolation[1], 0);
        if (tokens.length === 0 || !isStaticGlue(interpolation[1]
          .replace(/"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/gu, ""), true)) {
          valid = false;
          break;
        }
        value += tokens.map((token) => token.value).join("");
        cursor = interpolation.index + interpolation[0].length;
      }
      if (valid) {
        value += decodeLiteralBody(template[1].slice(cursor));
        scanText(value, contents, Array(value.length + 1).fill(template.index), found);
      }
  }
  for (const match of contents.matchAll(/\[([^\[\]]*)\]/gsu)) {
    const tokens = staticLiteralTokens(match[1], match.index + 1);
    const residue = match[1].replace(/"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/gu, "");
    if (tokens.length === 0 || !/^\s*(?:,\s*)*$/u.test(residue)) continue;
    const suffix = contents.slice(match.index + match[0].length);
    const join = suffix.match(/^\s*\.\s*join\s*\(\s*("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')\s*\)/su);
    const values = tokens.map((token) => token.value);
    const separator = join ? staticLiteralTokens(join[1], 0)[0]?.value : undefined;
    if (values.length > 0 && separator !== undefined) {
      const value = values.join(separator);
      scanText(value, contents, Array(value.length + 1).fill(match.index), found);
    }
  }
}

export function retiredOccurrences(contents) {
  const found = [];
  const decoded = decodeEscapes(contents);
  scanText(decoded.text, contents, decoded.sourceIndexes, found);
  scanStaticConstructions(contents, found);
  return [...new Map(found.map((occurrence) => [
    `${occurrence.symbol}\0${occurrence.line}\0${occurrence.column}`,
    occurrence,
  ])).values()];
}

async function sourceEntries(rootDirectory, relativeDirectory = "") {
  const directory = path.join(rootDirectory, relativeDirectory);
  const entries = await readdir(directory, { withFileTypes: true });
  const found = [];
  for (const entry of entries) {
    const relativePath = path.posix.join(relativeDirectory, entry.name);
    if (entry.isSymbolicLink()) {
      found.push({ path: relativePath, symlink: true });
    } else if (entry.isDirectory()) {
      if (IGNORED_GENERATED_ROOTS.has(relativePath)) continue;
      found.push(...await sourceEntries(rootDirectory, relativePath));
    } else if (entry.isFile()) {
      found.push({ path: relativePath, symlink: false });
    }
  }
  return found;
}

export function hasValidHistoricalMarker(contents, marker) {
  if (marker !== null && !contents.split(/\r?\n/u).includes(marker)) return false;
  const contradiction = /(?:modelo|costo|valoraci[oó]n).{0,80}(?:sigue|permanece|es|esta)\s+(?:vigente|activo)|(?:no|sin)\s+(?:queda[r]?|esta[r]?)\s+(?:reemplazad[oa]|retirad[oa])/giu;
  return !contradiction.test(contents);
}

function exceptionKey(relativePath, occurrence) {
  return `${relativePath}\0${occurrence.symbol}\0${occurrence.line}\0${occurrence.column}\0${occurrence.fingerprint}`;
}

function decodeText(buffer) {
  if (buffer.includes(0)) return null;
  try {
    const contents = new TextDecoder("utf-8", { fatal: true }).decode(buffer);
    const controls = [...contents].filter((character) => {
      const code = character.codePointAt(0);
      return code < 32 && character !== "\n" && character !== "\r" && character !== "\t" && character !== "\f";
    }).length;
    return controls > Math.max(1, contents.length / 100) ? null : contents;
  } catch {
    return null;
  }
}

export async function validateRetiredInventoryCosts(rootDirectory = process.cwd(), options = {}) {
  const { enforceImmutableFiles = true, enforcePinnedFiles = enforceImmutableFiles } = options;
  const violations = [];
  if ((await lstat(rootDirectory)).isSymbolicLink()) return [{ symbol: "symlink", path: ".", line: 1 }];

  const entries = await sourceEntries(rootDirectory);
  const paths = new Set(entries.map((entry) => entry.path));
  for (const immutablePath of IMMUTABLE_FILES.keys()) {
    if (enforceImmutableFiles && !paths.has(immutablePath)) {
      violations.push({ symbol: "immutable-file", path: immutablePath, line: 1 });
    }
  }
  if (enforcePinnedFiles) {
    for (const historicalPath of HISTORICAL_FILES.keys()) {
      if (!paths.has(historicalPath)) violations.push({ symbol: "historical-file", path: historicalPath, line: 1 });
    }
    for (const pinnedPath of PINNED_REFERENCE_FILES.keys()) {
      if (!paths.has(pinnedPath)) violations.push({ symbol: "pinned-reference", path: pinnedPath, line: 1 });
    }
  }

  const exceptions = new Set(ACTIVE_EXCEPTIONS.map((entry) => exceptionKey(entry.path, entry)));
  for (const entry of entries.sort((left, right) => left.path.localeCompare(right.path))) {
    if (entry.symlink) {
      violations.push({ symbol: "symlink", path: entry.path, line: 1 });
      continue;
    }

    // Integrity checks cover original bytes, before text decoding or binary filtering.
    const buffer = await readFile(path.join(rootDirectory, entry.path));
    const immutableHash = IMMUTABLE_FILES.get(entry.path);
    if (immutableHash !== undefined && enforceImmutableFiles) {
      if (sha256(buffer) !== immutableHash) violations.push({ symbol: "immutable-file", path: entry.path, line: 1 });
      continue;
    }

    const historical = HISTORICAL_FILES.get(entry.path);
    if (historical !== undefined && enforcePinnedFiles) {
      if (sha256(buffer) !== historical.sha256 || !hasValidHistoricalMarker(decodeText(buffer) ?? "", historical.marker)) {
        violations.push({ symbol: "historical-file", path: entry.path, line: 1 });
      }
      continue;
    }

    const pinnedHash = PINNED_REFERENCE_FILES.get(entry.path);
    if (pinnedHash !== undefined && enforcePinnedFiles) {
      if (sha256(buffer) !== pinnedHash) violations.push({ symbol: "pinned-reference", path: entry.path, line: 1 });
      continue;
    }

    const contents = decodeText(buffer);
    if (contents === null) continue;
    for (const occurrence of retiredOccurrences(contents)) {
      if (!exceptions.has(exceptionKey(entry.path, occurrence))) {
        violations.push({ symbol: occurrence.symbol, path: entry.path, line: occurrence.line, column: occurrence.column });
      }
    }
  }
  return violations;
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const violations = await validateRetiredInventoryCosts();
  if (violations.length === 0) {
    console.log("No retired inventory cost symbols found outside the auditable exceptions.");
  } else {
    console.error("Retired inventory cost symbols or integrity violations found:");
    for (const violation of violations) {
      console.error(`- ${violation.path}:${violation.line}${violation.column ? `:${violation.column}` : ""} (${violation.symbol})`);
    }
    process.exitCode = 1;
  }
}
