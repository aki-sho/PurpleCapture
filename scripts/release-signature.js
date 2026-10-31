export function assertReleaseSignature(signature, { unsignedPreview = false } = {}) {
  if (signature.status === "Valid" && signature.timestamped === true && signature.signer) return;
  if (unsignedPreview && signature.status === "NotSigned") return;
  throw new Error(`A trusted, timestamped code signature is required for publication. Status: ${signature.status}`);
}
