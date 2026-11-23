# quartet-timbre

Quartet's frontend: a Vue + TresJS 3D stage for live performances, census and training dashboards, and controls. It talks only to rhythm. See the [repository README](../README.md).

## Layout

```text
timbre/
├── src/
│   ├── api/              The typed client (fetch + EventSource) and hand-written API types
│   ├── views/            Pages: performances, performance (stage + census), training, ensemble
│   ├── components/       CensusPanel, the stage canvas, and app-level components
│   │   └── ui/           Reusable components: buttons, panels, and the SVG charts
│   ├── stage/            The three.js scene, beat interpolation, the frame stream
│   ├── composables/      Theme and training-stream composables
│   ├── styles/           Design tokens (light and dark) and base styles
│   └── __tests__/        Vitest (jsdom)
├── Caddyfile             Production: static files plus the /api proxy to rhythm
└── Dockerfile            Vite build served by Caddy
```

## Commands

```sh
pnpm install
pnpm dev          # Vite dev server; proxies /api to rhythm
pnpm type-check   # vue-tsc
pnpm test:unit    # vitest
pnpm build        # type-check + production build
pnpm lint         # oxlint + eslint, with fixes
pnpm format       # oxfmt, with fixes
```

`pnpm lint` and `pnpm format` apply fixes; `just check` runs the same tools in check mode.
