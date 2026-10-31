import test from "node:test";
import assert from "node:assert/strict";
import { assertReleaseSignature } from "./release-signature.js";
import { artifactNames } from "./metadata.js";

test("normal publication requires a valid timestamped signature", () => {
  assert.doesNotThrow(() => assertReleaseSignature({ status: "Valid", timestamped: true, signer: "Example" }));
  for (const signature of [{ status: "NotSigned" }, { status: "Valid", timestamped: false, signer: "Example" }, { status: "Valid", timestamped: true }]) {
    assert.throws(() => assertReleaseSignature(signature));
  }
});
test("unsigned preview never accepts damaged, expired or untrusted signatures", () => {
  assert.doesNotThrow(() => assertReleaseSignature({ status: "NotSigned" }, { unsignedPreview: true }));
  for (const status of ["HashMismatch", "NotTrusted", "UnknownError", "NotSupportedFileFormat", ""]) {
    assert.throws(() => assertReleaseSignature({ status }, { unsignedPreview: true }));
  }
});
test("preview artifacts are visibly separate from normal distributions", () => {
  const meta = { executableBaseName: "PurpleCapture", version: "1.0.2" };
  const normal = artifactNames(meta);
  const preview = artifactNames(meta, { unsignedPreview: true });
  for (const name of Object.keys(normal)) {
    assert.notEqual(normal[name], preview[name]);
    assert.match(preview[name], /-Unsigned(?:\.|$)/);
  }
});
