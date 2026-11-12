---
name: Infrastructure Security Review
description: Assess the security of container images, Docker and Compose setups, Kubernetes workloads, and local Minikube clusters. Use for hardening reviews, vulnerability scans, RBAC and Pod Security checks, network exposure analysis, supply-chain validation, and severity-rated security findings with remediation.
---

# Infrastructure Security Review

## Mindset

- Review against an explicit threat model: assets, entry points, trust
  boundaries, identities, and what an attacker gains at each step.
- Apply least privilege, defense in depth, and secure defaults. Absence of a
  control is a finding, not a neutral observation.
- Prefer evidence over assumption: every finding cites the file, manifest, or
  command output that proves it.
- Stay read-only unless the user authorizes remediation. Never run exploits,
  port scans, or credential attacks against systems without written permission.

## Assessment method

1. **Inventory** images, Dockerfiles, Compose files, manifests, Helm charts,
   namespaces, service accounts, and exposed services.
2. **Map exposure**: published ports, `NodePort`/`LoadBalancer`, ingress hosts,
   `hostNetwork`, `hostPath`, mounted sockets, and privileged containers.
3. **Check controls** in this order: identity and RBAC, workload hardening,
   network segmentation, secrets handling, image and supply chain, logging and
   auditability.
4. **Rate severity** with the rubric below and record evidence.
5. **Report** findings with remediation and a verification command.

## Severity rubric

| Severity | Criteria |
|---|---|
| Critical | Direct cluster or host compromise, or secret exposure with immediate impact (privileged pod, mounted `docker.sock`, cluster-admin bound to a workload, committed credentials) |
| High | Broad lateral movement or data access (wildcard RBAC, `hostPath` to sensitive paths, no NetworkPolicy with a compromised workload, critical image CVE reachable from untrusted input) |
| Medium | Defense-in-depth gap or likely escalation (running as root, writable root filesystem, missing limits, mutable `latest` tag, unencrypted secret at rest) |
| Low | Hygiene or observability gap (missing labels, no resource quota, verbose logs, outdated but unreachable package) |

Report the CVSS vector or scanner severity when available, but adjust for
exploitability in the actual deployment context.

## Containers and images

- Base image: minimal, maintained, pinned by tag and digest; record the support
  window.
- Scan for vulnerabilities: `trivy image --severity HIGH,CRITICAL <image>`,
  `grype <image>`, or `docker scout cves <image>`. Distinguish reachable
  runtime libraries from build-time tooling.
- Run as non-root (`USER` with numeric UID, `runAsNonRoot: true`), read-only
  root filesystem where possible, drop all capabilities and add back only what
  is required.
- No secrets in layers, build args, environment defaults, or image history
  (`docker history --no-trunc`). Use build secrets or runtime injection.
- No `--privileged`, no host PID/network/IPC namespaces, no device passthrough,
  no writable host bind mounts, and never mount `/var/run/docker.sock`.
- Set CPU and memory limits so a compromised or buggy container cannot exhaust
  the host.
- Produce an SBOM (`syft <image> -o spdx-json`) and, where the pipeline allows,
  sign images (`cosign sign`) and verify signatures before deploy.
- Compose: avoid `privileged: true`, `network_mode: host`, `pid: host`, and
  plaintext secrets in `environment`. Prefer `secrets` or an external store.

## Kubernetes and Minikube

Identity and access:

- Enumerate with `kubectl auth can-i --list` and
  `kubectl get clusterrolebindings -o wide`. Flag `cluster-admin`, wildcard
  verbs/resources, `escalate`/`bind`/`impersonate`, and default service
  accounts with token automount.
- Set `automountServiceAccountToken: false` unless the workload calls the API.
- One namespace per trust boundary; avoid `default`.

Workload hardening:

- Enforce Pod Security Standards: `kubectl label ns <ns> pod-security.kubernetes.io/enforce=restricted`.
- Require `runAsNonRoot`, `seccompProfile: RuntimeDefault`,
  `allowPrivilegeEscalation: false`, and `capabilities.drop: ["ALL"]`.
- Reject `hostPath`, `hostNetwork`, `hostPID`, privileged containers, and
  `hostPort` in application workloads.

Network:

- Default-deny `NetworkPolicy` per namespace, then allow only required flows.
- Review `Service` types; prefer `ClusterIP` plus ingress over `NodePort`.
- Terminate TLS at ingress; no plaintext credentials over exposed routes.

Secrets and configuration:

- No `Secret` manifests, `.env`, kubeconfigs, or tokens in git.
- Prefer an external secret store; otherwise enable encryption at rest and
  restrict `get`/`list` on secrets by RBAC.
- Do not echo secret values during review; redact in reports.

Supply chain and admission:

- Allowlist registries; require digests or immutable tags; set explicit
  `imagePullPolicy`.
- Where available, add admission controls (Kyverno, OPA Gatekeeper, Sigstore
  policy controller) and verify signatures at admission.

Minikube specifics:

- Treat Minikube as development only: permissive defaults, no HA, no
  multi-tenant isolation, and often no audit logging or encryption at rest.
- Never expose the dashboard, API server, or ingress publicly; bind to
  localhost and use `minikube tunnel` only when required.
- Disable unused addons and delete the profile after testing.

## Useful commands

```bash
trivy image --severity HIGH,CRITICAL <image>
trivy config <path>                       # Dockerfile, Compose, manifests
kube-score score k8s/*.yaml
kubescape scan framework nsa k8s/
kube-bench                                # CIS benchmark, run as a Job
kubectl get pods -A -o json | jq '.items[] | {name: .metadata.name, sc: .spec.securityContext}'
kubectl get networkpolicy -A
kubectl get svc -A
kubectl auth can-i --list --as=system:serviceaccount:<ns>:<sa>
docker history --no-trunc <image>
```

Confirm that a control actually works. A `NetworkPolicy` with no CNI
enforcement, or a signature policy in audit-only mode, is not a control.

## Report format

For each finding:

```
ID: SEC-001
Asset: k8s/deployment.yaml (namespace app)
Severity: High
Evidence: securityContext missing; container runs as root (uid 0)
Impact: Container compromise grants root in the pod and writable filesystem
Remediation: set runAsNonRoot, runAsUser 10001, readOnlyRootFilesystem true, drop ALL caps
Verification: kubectl get pod -n app -o jsonpath='{.items[0].spec.containers[0].securityContext}'
```

End with a short prioritized plan (fix criticals first), residual risk, and
controls that could not be verified in the current environment.

## Verification checklist

- Every claimed finding has evidence and a reproducible verification command.
- Severity reflects real exploitability, not only scanner output.
- No secret values appear in the report or console output.
- Remediation preserves required functionality; note behavior changes.
- Verified controls are distinguished from controls assumed by configuration.
