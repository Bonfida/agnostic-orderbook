# Asset Agnostic Orderbook JS library

Webassembly can be used to deserialize orderbooks. By default only the pure JS methods are available, in order to enable Webassembly:

1. Install `wasm-pack`: https://rustwasm.github.io/wasm-pack/installer/

2. Add the following to your `package.json`

```json
 "dex-wasm": "file:wasm/pkg"
```

3. Uncomment the Webassembly methods in `slab.ts`

4. Build the library using `yarn build:node`

5. Run `npm install ./wasm/pkg`
