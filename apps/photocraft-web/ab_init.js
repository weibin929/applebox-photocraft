// Trunk initializer (Apple Box fork, AB_FORK.md): download progress + the host bridge.
// The host page (same origin, the parent frame) talks to the editor with postMessage:
//   host → editor  { type: 'lc-pc-open', name, bytes }   open a file (ArrayBuffer)
//   host → editor  { type: 'lc-pc-command', cmd }        'save_psd' | 'export_png'
//   host → editor  { type: 'lc-pc-call', id, method, params }   bridge v2: one control request (allow-listed)
//   host → editor  { type: 'lc-pc-font', name, bytes }   a font for type layers (ArrayBuffer; send before lc-pc-open)
//   editor → host  { type: 'lc-pc-reply', id, ok, result | error }
//   editor → host  { type: 'lc-pc-state', seq, … }        the editor's state when it changed; order by seq
//   editor → host  { type: 'lc-pc-progress', loaded, total }
//   editor → host  { type: 'lc-pc-ready', backend, bridge }   bridge = 2 when lc-pc-call is available
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
    const v2 = typeof b.host_control === 'function' && typeof b.set_host_events === 'function';
    const ready = () => post({ type: 'lc-pc-ready', backend: navigator.gpu ? 'webgpu' : 'webgl2', bridge: v2 ? 2 : 1 });
    addEventListener('message', (e) => {
      if (e.origin !== ORIGIN || e.source !== window.parent) return;
      const m = e.data || {};
      if (m.type === 'lc-pc-open' && m.bytes) b.open_bytes(String(m.name || 'document'), new Uint8Array(m.bytes));
      else if (m.type === 'lc-pc-command') b.host_command(String(m.cmd || ''));
      else if (m.type === 'lc-pc-call' && v2) b.host_control(m.id >>> 0, String(m.method || ''), JSON.stringify(m.params || {}));
      else if (m.type === 'lc-pc-font' && m.bytes && typeof b.ab_add_font === 'function') b.ab_add_font(String(m.name || 'font'), new Uint8Array(m.bytes));
    });
    if (v2) {
      // Bridge v2: the editor says `ready` once its control channel takes requests (the bindings,
      // and so this initializer, exist a moment before the app does).
      b.set_host_events((s) => {
        let m;
        try { m = JSON.parse(s); } catch (e) { return; }
        if (m.type === 'reply') post({ type: 'lc-pc-reply', id: m.id, ok: !!m.ok, result: m.result, error: m.error });
        else if (m.type === 'lc-pc-state') post(m);
        else if (m.type === 'ready') ready();
      });
    } else {
      ready();
    }
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
