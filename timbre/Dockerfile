# timbre: the static frontend, served by Caddy, which also proxies /api to
# rhythm (the browser sees one origin, as with the Vite dev proxy). Build
# from the repository root:
#   docker build -f timbre/Dockerfile .
FROM node:26.13.1-trixie-slim AS build
RUN npm install -g pnpm@12.21.0
WORKDIR /src/timbre
COPY timbre/package.json timbre/pnpm-lock.yaml timbre/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY timbre/ ./
RUN pnpm exec vite build

FROM caddy:2.11.9-alpine
COPY timbre/Caddyfile /etc/caddy/Caddyfile
COPY --from=build /src/timbre/dist /srv
ENV QUARTET_RHYTHM_UPSTREAM=rhythm:8080
EXPOSE 80
