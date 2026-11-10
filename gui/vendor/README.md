# Vendored third-party code

`three-bundle.js` — [three.js](https://threejs.org) r187, MIT licensed, bundled with esbuild.

Bundled from `three` (core + `GLTFLoader`, `OrbitControls`, `MeshoptDecoder`) so the avatar
window works **entirely offline**: no CDN, no dev server, no network at runtime. This is a
hard requirement of the project — a tool whose selling point is "no network code" cannot
fetch its renderer from the internet.

Reproduce:

```bash
npm i --no-save three@0.187.0 esbuild
cat > entry.js <<'JS'
export * from 'three';
export { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
export { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
export { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js';
JS
./node_modules/.bin/esbuild entry.js --bundle --format=esm --minify --outfile=three-bundle.js
```

Licence text for the vendored bundle is in `THIRD-PARTY.md`.
