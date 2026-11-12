---
name: Kubernetes Minikube
description: Run and operate local Kubernetes clusters with Minikube, write production-shaped manifests, deploy images, and diagnose pods, services, and ingress. Use for local cluster setup, kubectl workflows, and Kubernetes debugging.
---

# Kubernetes Minikube

## Cluster lifecycle

```bash
minikube start --driver=docker --cpus=4 --memory=8192 --kubernetes-version=stable
minikube status
kubectl cluster-info
kubectl get nodes -o wide
minikube stop
minikube delete --profile=<name>
```

- Use `-p <profile>` to keep independent clusters; always pass the profile to
  every `minikube` command that targets it.
- Prefer the `docker` driver on Linux unless the host setup requires another.
- Enable what the workload needs instead of installing everything:
  `minikube addons enable ingress`, `minikube addons enable metrics-server`.

## Getting images into the cluster

```bash
minikube image load app:dev            # preferred: no registry needed
minikube image ls --format table
```

Alternative with a shared daemon, per shell session:

```bash
eval $(minikube docker-env)
docker build -t app:dev .
# build inside that shell so the node daemon sees the image
```

With `imagePullPolicy: Never` or `IfNotPresent`, the loaded tag must match the
manifest exactly. Avoid `latest`; use an explicit tag or digest.

## Manifest standards

- Set `metadata.namespace`, labels, and `app.kubernetes.io/*` recommended labels.
- Set `resources.requests` and `resources.limits` for every container.
- Add `readinessProbe` and `livenessProbe`; use `startupProbe` for slow starts.
- Set `securityContext`: `runAsNonRoot: true`, `runAsUser`, `allowPrivilegeEscalation: false`,
  `readOnlyRootFilesystem: true` where feasible, `capabilities.drop: ["ALL"]`.
- Configure non-secret values through `ConfigMap` and secrets through `Secret`;
  never commit real secret material.
- Prefer `Deployment` with a rolling update strategy; use `StatefulSet` only for
  stable identity or per-replica storage.
- Pin `imagePullPolicy` explicitly and `revisionHistoryLimit` for rollbacks.

Minimal reference:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
  namespace: app
  labels: { app.kubernetes.io/name: app }
spec:
  replicas: 2
  selector:
    matchLabels: { app.kubernetes.io/name: app }
  template:
    metadata:
      labels: { app.kubernetes.io/name: app }
    spec:
      containers:
        - name: app
          image: app:dev
          imagePullPolicy: Never
          ports: [{ containerPort: 8080, name: http }]
          resources:
            requests: { cpu: 100m, memory: 128Mi }
            limits: { cpu: 500m, memory: 256Mi }
          readinessProbe:
            httpGet: { path: /healthz, port: http }
            periodSeconds: 5
          livenessProbe:
            httpGet: { path: /healthz, port: http }
            initialDelaySeconds: 10
            periodSeconds: 10
          securityContext:
            runAsNonRoot: true
            allowPrivilegeEscalation: false
            capabilities: { drop: ["ALL"] }
```

## Apply and observe

```bash
kubectl apply -f k8s/ --dry-run=server     # validate against the API first
kubectl apply -f k8s/
kubectl rollout status deployment/app -n app --timeout=120s
kubectl get all -n app
kubectl get events -n app --sort-by=.lastTimestamp
kubectl describe pod -n app <pod>
kubectl logs -n app deploy/app --tail=200 -f
kubectl exec -it -n app <pod> -- sh
```

Rollback and cleanup:

```bash
kubectl rollout undo deployment/app -n app
kubectl delete -f k8s/
```

## Accessing workloads

```bash
kubectl port-forward -n app svc/app 8080:80
minikube service app -n app --url
minikube tunnel                            # for LoadBalancer services
minikube ip                                # combine with ingress host
```

For ingress, add the host to `/etc/hosts` pointing at `$(minikube ip)` and
verify with `kubectl get ingress -n app`.

## Debugging playbook

| State | Likely cause | First checks |
|---|---|---|
| `ImagePullBackOff` / `ErrImagePull` | Image not in node, wrong tag, private registry | `minikube image ls`, image tag vs manifest, `imagePullPolicy` |
| `CrashLoopBackOff` | App exits, bad config, missing env | `kubectl logs --previous`, exit code, config and secrets |
| `Pending` | Unschedulable: resources, node selector, PVC | `kubectl describe pod`, node capacity, events |
| `OOMKilled` | Memory limit too low | `kubectl describe pod` last state, raise limit or fix leak |
| Probe failures | Wrong path/port, slow start | `kubectl describe`, probe config, `startupProbe` |
| Service unreachable | Selector mismatch, target port | `kubectl get endpoints`, labels, `containerPort` name |
| Pending PVC | No default StorageClass | `kubectl get sc`, enable `storage-provisioner` |

Always read `kubectl describe` events and previous logs before changing
manifests.

## Verification checklist

- `kubectl apply --dry-run=server` passes for every manifest.
- Pods reach `Ready`; `kubectl rollout status` succeeds.
- Probes, resource requests/limits, and `securityContext` are set.
- Service endpoints are populated and the app answers through the chosen path
  (port-forward, NodePort, or ingress).
- No secrets are committed; `minikube delete` leaves no stray profile when done.
