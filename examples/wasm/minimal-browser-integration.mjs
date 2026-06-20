import init, {
  dabctlVersion,
  decodeEtiToLatmMemoryWithOptions,
  WasmLatmDecodeOptions,
} from "../../pkg/dabctl.js";

async function main() {
  // Initialize wasm-bindgen runtime in browser context.
  await init(new URL("../../pkg/dabctl_bg.wasm", import.meta.url));
  console.log("dabctl WASM version:", dabctlVersion());

  // Load ETI bytes from an HTTP-accessible location.
  const etiResponse = await fetch("../../test-local/multiplex-t.eti");
  if (!etiResponse.ok) {
    throw new Error(`Unable to fetch ETI fixture: HTTP ${etiResponse.status}`);
  }
  const etiBytes = new Uint8Array(await etiResponse.arrayBuffer());

  const options = new WasmLatmDecodeOptions();
  options.sid = "0xf201";
  options.dedupPad = true;
  options.slideBase64 = false;
  options.datetimeFormat = "iso8601";

  const output = decodeEtiToLatmMemoryWithOptions(etiBytes, options);

  console.log("LATM bytes:", output.latmBytes.length);
  console.log("FD3 events:", output.metadataJsonl.length);
  console.log("FD3 preview:\n" + output.fd3Preview());
}

main().catch((err) => {
  console.error("Browser WASM integration example failed:", err);
});
