import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { referenceManifest } from "@lib/research/reference-manifest.js";
import { REFERENCE_IDS } from "@lib/research/reference-types.js";

const strict = process.argv.includes("--strict");
const sha256Pattern = /^[a-f0-9]{64}$/u;
const ids = referenceManifest.references.map(({ id }) => id);
if (
  ids.length !== REFERENCE_IDS.length ||
  new Set(ids).size !== REFERENCE_IDS.length ||
  REFERENCE_IDS.some((id) => !ids.includes(id))
) {
  throw new Error("Reference manifest must contain each of the eight reference IDs exactly once.");
}
if (!sha256Pattern.test(referenceManifest.baseline.binarySha256)) throw new Error("Invalid baseline binary SHA256.");

let captured = 0;
for (const reference of referenceManifest.references) {
  if (reference.status === "pending") {
    if (!reference.reason.trim()) throw new Error(`${reference.id}: pending references require a reason.`);
    continue;
  }
  captured++;
  if (!sha256Pattern.test(reference.sha256)) throw new Error(`${reference.id}: invalid SHA256.`);
  const bytes = await readFile(resolve(reference.relativePath));
  const actual = createHash("sha256").update(bytes).digest("hex");
  if (actual !== reference.sha256) throw new Error(`${reference.id}: hash mismatch.`);
  if (bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") throw new Error(`${reference.id}: not a PNG.`);
  if (bytes.readUInt32BE(16) !== reference.width || bytes.readUInt32BE(20) !== reference.height)
    throw new Error(`${reference.id}: dimensions mismatch.`);
}
if (strict && captured !== REFERENCE_IDS.length)
  throw new Error(`Strict reference check: ${captured}/8 captured; ${8 - captured} pending.`);
console.log(
  `references: manifest valid; ${captured}/8 captured, ${8 - captured}/8 pending${strict ? " (strict)" : ""}`,
);
