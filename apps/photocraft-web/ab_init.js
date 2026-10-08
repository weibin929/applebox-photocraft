// Trunk initializer (Apple Box fork, AB_FORK.md): download progress + the host bridge.
// The host page (same origin, the parent frame) talks to the editor with postMessage:
//   host → editor  { type: 'lc-pc-open', name, bytes }   open a file (ArrayBuffer)
//   host → editor  { type: 'lc-pc-command', cmd }        'save_psd' | 'export_png'
//   editor → host  { type: 'lc-pc-progress', loaded, total }
//   editor → host  { type: 'lc-pc-ready', backend }
//   editor → host  { type: 'lc-pc-file', name, kind, bytes }   a saved/exported file (ArrayBuffer, transferred)
//   editor → host  { type: 'lc-pc-error', message }
// Nothing here talks to any server; the host decides what to do with the files.
export default function initializer() {
  const ORIGIN = location.origin;
  const hasHost = window.parent && window.parent !== window;
  const post = (m, transfer) => { if (hasHost) window.parent.postMessage(m, ORIGIN, transfer || []); };

  function bridge(b) {
    b.set_embedded(true);
    b.set_host_writer((name, data) => {
      const kind = /\.psd$/i.test(name) ? 'psd' : /\.png$/i.test(name) ? 'png' : 'other';
      const buf = data.slice().buffer;
      post({ type: 'lc-pc-file', name, kind, bytes: buf }, [buf]);
    });
    addEventListener('message', (e) => {
      if (e.origin !== ORIGIN || e.source !== window.parent) return;
      const m = e.data || {};
      if (m.type === 'lc-pc-open' && m.bytes) b.open_bytes(String(m.name || 'document'), new Uint8Array(m.bytes));
      else if (m.type === 'lc-pc-command') b.host_command(String(m.cmd || ''));
    });
    post({ type: 'lc-pc-ready', backend: navigator.gpu ? 'webgpu' : 'webgl2' });
  }

  return {
    onStart: () => post({ type: 'lc-pc-progress', loaded: 0, total: 0 }),
    onProgress: ({ current, total }) => post({ type: 'lc-pc-progress', loaded: current, total: total || 0 }),
    onComplete: () => {},
    // Trunk assigns `window.wasmBindings` and fires `TrunkApplicationStarted` only after this
    // initializer's onSuccess returns, so the bridge is wired from that event (with a short poll as
    // a fallback for other loaders).
    onSuccess: (_wasm) => {
      let wired = false;
      const wire = () => {
        if (wired) return;
        const b = window.wasmBindings;
        if (!b) return;
        wired = true;
        if (!b.open_bytes || !b.set_host_writer || !b.host_command || !b.set_embedded) {
          post({ type: 'lc-pc-error', message: 'host bridge missing in this build' });
          return;
        }
        bridge(b);
      };
      addEventListener('TrunkApplicationStarted', wire, { once: true });
      let tries = 0;
      const poll = setInterval(() => { wire(); if (wired || ++tries > 200) clearInterval(poll); }, 50);
    },
    onFailure: (error) => post({ type: 'lc-pc-error', message: String(error) }),
  };
}
