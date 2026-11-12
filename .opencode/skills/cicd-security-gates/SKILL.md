---
name: CI/CD Security Gates
description: Design and implement CI/CD pipelines that build, scan, sign, and deploy containers with blocking security quality gates. Use for GitHub Actions or GitLab CI pipelines, image scanning gates, SBOM and signing, manifest validation, secret scanning, least-privilege tokens, and deployment protection.
---

# CI/CD Security Gates

## Principles

- Pipeline as code, reviewed like code: pinned tool versions, explicit stages,
  reproducible commands, no manual steps that are not scripted.
- Fail fast and block: a gate that only warns is not a gate. Never hide failures
  with `|| true`, `continue-on-error`, or a soft threshold.
- Build once, promote the same digest. Rebuilding per environment breaks the
  guarantee that what was scanned is what runs.
- Least privilege for the pipeline identity: read-only default token, write
  scopes only in the job that needs them, no long-lived cloud credentials when
  OIDC is available.
- Pin third-party actions and images to a full commit SHA or immutable digest.
  Mutable tags such as `@master` or `:latest` are a supply-chain risk.

## Stage order

1. **Quality**: format check, lints as errors, unit tests with a locked
   dependency graph.
2. **Secret scan**: fail on any verified leak in the diff and the history.
3. **Build**: Dockerfile lint, config/manifest scan, image build, image scan.
4. **Artifacts**: SBOM generation, vulnerability report, image digest.
5. **Sign and publish**: sign the digest, push to the registry, keep provenance.
6. **Deploy**: gated by protected environment and every previous gate.

## Gate policy

| Gate | Tool | Blocking threshold |
|---|---|---|
| Formatting, lints, tests | `cargo fmt`, `cargo clippy -D warnings`, `cargo test` | Any failure |
| Dockerfile lint | `hadolint` | `error` and above |
| IaC and manifest misconfiguration | `trivy config`, `kube-score` | `HIGH` and `CRITICAL` |
| Image vulnerabilities | `trivy image` | `CRITICAL` always; `HIGH` when a fix exists |
| Secrets | `gitleaks` | Any verified finding |
| Manifest schema | `kubeconform -strict` | Any invalid resource |
| SBOM | `syft` | Missing artifact |
| Signature | `cosign` | Unsigned or unverifiable digest in production |

Document every exception with an owner, a reason, and an expiry date in the
ignore file (`.trivyignore.yaml`, `.gitleaks.toml`). Exceptions without expiry
become permanent.

## GitHub Actions template

```yaml
name: ci

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  IMAGE: ghcr.io/${{ github.repository }}:${{ github.sha }}

jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - run: cargo test --all-targets --locked

  secrets:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - run: |
          docker run --rm -v "$PWD:/repo" zricethezav/gitleaks:v8.21.2 \
            detect --source /repo --no-git --redact --verbose

  image:
    runs-on: ubuntu-latest
    needs: [quality, secrets]
    steps:
      - uses: actions/checkout@v4
      - name: Lint Dockerfile
        uses: hadolint/hadolint-action@v3.1.0
        with:
          failure-threshold: error
      - name: Scan Dockerfile and manifests
        uses: aquasecurity/trivy-action@0.28.0
        with:
          scan-type: config
          scan-ref: .
          severity: HIGH,CRITICAL
          exit-code: "1"
      - uses: docker/setup-buildx-action@v3
      - name: Build image
        uses: docker/build-push-action@v6
        with:
          context: .
          load: true
          tags: ${{ env.IMAGE }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
      - name: Scan image
        uses: aquasecurity/trivy-action@0.28.0
        with:
          image-ref: ${{ env.IMAGE }}
          severity: HIGH,CRITICAL
          ignore-unfixed: true
          exit-code: "1"
      - name: Generate SBOM
        uses: anchore/sbom-action@v0.17.0
        with:
          image: ${{ env.IMAGE }}
          format: spdx-json
          artifact-name: sbom.spdx.json

  manifests:
    runs-on: ubuntu-latest
    needs: quality
    if: ${{ hashFiles('k8s/**/*.yaml', 'k8s/**/*.yml') != '' }}
    steps:
      - uses: actions/checkout@v4
      - run: |
          docker run --rm -v "$PWD/k8s:/manifests" \
            ghcr.io/yannh/kubeconform:v0.6.7 -strict -summary /manifests

  publish:
    if: github.ref == 'refs/heads/main' && github.event_name == 'push'
    runs-on: ubuntu-latest
    needs: [image, manifests]
    permissions:
      contents: read
      packages: write
      id-token: write
    steps:
      - uses: actions/checkout@v4
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - uses: docker/build-push-action@v6
        with:
          context: .
          push: true
          tags: ${{ env.IMAGE }}
      - uses: sigstore/cosign-installer@v3.7.0
      - run: cosign sign --yes ${{ env.IMAGE }}
      - run: cosign verify ${{ env.IMAGE }} \
          --certificate-identity-regexp "https://github.com/${{ github.repository }}/" \
          --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

- Replace every action tag with a full commit SHA in production pipelines.
- `id-token: write` exists only in `publish`, so PR jobs cannot mint OIDC tokens.
- Prefer promoting the digest produced in `image` over rebuilding in `publish`;
  pass it through `docker save`/`load` or an artifact when build time matters.
- Add `environment: production` with required reviewers on the deploy job.
- Use `pull_request`, never `pull_request_target` with untrusted checkout.

## GitLab CI equivalent

```yaml
stages: [quality, security, build, deploy]

image-scan:
  stage: security
  image:
    name: aquasec/trivy:0.57.1
    entrypoint: [""]
  script:
    - trivy config --severity HIGH,CRITICAL --exit-code 1 .
    - trivy image --severity HIGH,CRITICAL --ignore-unfixed --exit-code 1 "$IMAGE"
```

Use `rules:` to limit deployment jobs to the default branch and protected
environments. Keep secrets in masked, protected CI variables, never in
`.gitlab-ci.yml`.

## Local reproduction

```bash
hadolint Dockerfile
trivy config .
trivy image --severity HIGH,CRITICAL --ignore-unfixed app:dev
kubeconform -strict -summary k8s/
gitleaks detect --no-git --redact
syft app:dev -o spdx-json > sbom.spdx.json
```

If a tool is unavailable locally, do not mark the gate as verified: state that
it runs only in CI and link the job definition.

## Verification checklist

- Every gate blocks with a non-zero exit and has been observed failing on a
  known-bad input.
- Actions and scanner images are pinned; default token permissions are read-only.
- SBOM and scan reports are retained as CI artifacts with a defined retention.
- The deployed artifact is the scanned digest, not a rebuild.
- Exceptions have an owner and an expiry date.
- Deployment requires a protected environment or manual approval.

## Anti-patterns

- `continue-on-error: true`, `|| true`, or `--exit-code 0` on a security gate.
- Scanning only on `main`, leaving pull requests unguarded.
- Building the image twice and deploying an unscanned tag.
- Storing registry or cloud credentials as long-lived repository secrets.
- Installing unpinned scanner binaries from `latest` at job runtime.
