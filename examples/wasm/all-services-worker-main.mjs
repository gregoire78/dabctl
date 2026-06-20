// Browser main-thread example for all-services decode in a dedicated Worker.
const worker = new Worker(new URL("./all-services.worker.mjs", import.meta.url), {
  type: "module",
});

worker.onmessage = (event) => {
  const msg = event.data;
  if (!msg.ok) {
    console.error("All-services worker failed:", msg.error);
    return;
  }

  console.log("Decoded services:", msg.serviceCount);
  for (const service of msg.services) {
    console.log(
      `[${service.sid}] ${service.label ?? "(no label)"} -> LATM ${service.latmBytes.length} bytes, FD3 ${service.metadataJsonl.length} events`,
    );
  }
};

worker.onerror = (err) => {
  console.error("Worker runtime error:", err);
};

async function run() {
  const etiResponse = await fetch("../../test-local/multiplex-t.eti");
  if (!etiResponse.ok) {
    throw new Error(`Unable to fetch ETI fixture: HTTP ${etiResponse.status}`);
  }

  // Transfer ETI bytes to worker to avoid copying large buffers.
  const etiBytes = new Uint8Array(await etiResponse.arrayBuffer());
  worker.postMessage(
    {
      etiBytes,
      datetimeFormat: "iso8601",
      dedupPad: true,
      slideBase64: false,
    },
    [etiBytes.buffer],
  );
}

run().catch((err) => {
  console.error("All-services worker main failed:", err);
});
