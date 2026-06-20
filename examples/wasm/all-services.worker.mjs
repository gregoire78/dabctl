import init, {
  decodeEtiToLatmAllServicesMemoryWithOptions,
  WasmAllServicesDecodeOptions,
} from "../../pkg/dabctl.js";

let ready = false;

self.onmessage = async (event) => {
  try {
    if (!ready) {
      await init(new URL("../../pkg/dabctl_bg.wasm", import.meta.url));
      ready = true;
    }

    const { etiBytes, datetimeFormat = "iso8601", dedupPad = true, slideBase64 = false } =
      event.data;

    const options = new WasmAllServicesDecodeOptions();
    options.datetimeFormat = datetimeFormat;
    options.dedupPad = dedupPad;
    options.slideBase64 = slideBase64;

    const output = decodeEtiToLatmAllServicesMemoryWithOptions(
      new Uint8Array(etiBytes),
      options,
    );

    const services = [];
    const transferList = [];

    for (let i = 0; i < output.serviceCount; i += 1) {
      const svc = output.getService(i);
      if (!svc) {
        continue;
      }

      const latmBytes = svc.latmBytes;
      services.push({
        sid: svc.sid,
        label: svc.label,
        latmBytes,
        metadataJsonl: svc.metadataJsonl,
      });
      transferList.push(latmBytes.buffer);
    }

    self.postMessage(
      {
        ok: true,
        serviceCount: services.length,
        services,
      },
      transferList,
    );
  } catch (err) {
    self.postMessage({ ok: false, error: String(err) });
  }
};
