---
name: Docker Containers
description: Build, harden, and debug Docker images and Compose stacks. Use for Dockerfiles, multi-stage builds, image size and cache optimization, non-root containers, container networking, volumes, and local container workflows.
---

# Docker Containers

## Image design

- Build one concern per image; one process per container unless the process
  manager is genuinely required.
- Pin base images to a specific tag and, for reproducibility, a digest
  (`FROM debian:12-slim@sha256:...`). Never use `latest` in committed files.
- Prefer small, minimal bases: `-slim`, `-alpine`, or distroless. Alpine means
  musl, which changes native linking; verify the binary before switching.
- Use multi-stage builds so compilers, package managers, and source code never
  reach the runtime layer.
- Run as a non-root user with an explicit `USER` and a numeric UID.
- Add OCI labels: `org.opencontainers.image.{title,description,source,version,licenses}`.
- Expose only the port the process listens on, and document it with `EXPOSE`.

## Dockerfile rules

- Order instructions from least to most frequently changing to preserve layer
  cache; copy dependency manifests before full source.
- Combine related `RUN` steps and clean package caches in the same layer.
- Use `COPY` instead of `ADD` unless extracting a local archive is intended.
- Use `exec` form for `ENTRYPOINT`/`CMD` so the process receives signals:
  `ENTRYPOINT ["/app/server"]`.
- Add a `HEALTHCHECK` when the orchestrator will not provide one.
- Never bake secrets, tokens, or `.env` files into an image; use build secrets
  (`RUN --mount=type=secret`) or runtime injection.
- Keep a `.dockerignore` that excludes `target/`, `.git/`, `node_modules/`,
  tests fixtures, and local env files.

## Rust multi-stage template

```dockerfile
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /build/target/release/app /usr/local/bin/app
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/app"]
```

Use `cargo chef` or a dependency-only dummy build when dependency compilation
dominates build time and cache reuse matters.

## Everyday commands

```bash
docker build -t app:dev .
docker build --target builder -t app:debug .        # inspect a stage
docker run --rm -it -p 8080:8080 --env-file .env app:dev
docker run --rm -it --entrypoint /bin/sh app:dev    # only if the image has a shell
docker logs -f --tail 100 <container>
docker exec -it <container> sh
docker inspect <container> --format '{{.State.ExitCode}}'
docker history --no-trunc app:dev
docker image prune -f && docker builder prune -f
```

## Compose

- Keep one `compose.yaml` per environment concern; override with
  `compose.override.yaml` locally.
- Use `healthcheck` plus `depends_on: { db: { condition: service_healthy } }`
  instead of sleeps.
- Prefer named volumes for state and bind mounts only for active development.
- Put non-secret configuration in `environment`, secrets in `secrets` or
  `env_file`, and never commit the env file.
- Verify with `docker compose config` before `up`.

```bash
docker compose config -q
docker compose up -d --build
docker compose ps
docker compose logs -f --tail 100 <service>
docker compose down -v
```

## Debugging

| Symptom | Check |
|---|---|
| Build fails at a step | Rerun that stage with `docker build --target`, read the failing layer |
| Container exits immediately | `docker logs`, `docker inspect` exit code, test the entrypoint locally |
| Port unreachable | Process binds `0.0.0.0`, not `127.0.0.1`; port mapping matches |
| Image unexpectedly large | `docker history`, forgotten build tools, cache dirs, debug symbols |
| Changes not applied | Stale cache: `docker build --no-cache`; verify `.dockerignore` |

## Verification checklist

- `docker build` succeeds from a clean context.
- Container runs as non-root and passes its healthcheck.
- Image contains no secrets, `.env`, `.git`, or build tooling.
- Size and startup time are acceptable for the target environment.
- `docker compose config` is valid when Compose is used.
