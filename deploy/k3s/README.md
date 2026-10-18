# k3s deployment

The production path is Cloudflare Tunnel → private APISIX → API Service → API
Pods. This profile uses APISIX standalone YAML mode. It needs no APISIX etcd,
Admin API, ingress controller, public LoadBalancer, NodePort, or Ingress object.
New API endpoints use the catch-all route. Add a controller when independently
deployed services need dynamic route ownership.

The baseline runs two API, two APISIX, and two cloudflared replicas. PostgreSQL,
SMTP, optional S3 storage, and optional search are external services. API and
worker Pods contain no authoritative local state. PostgreSQL owns sessions,
authentication limits, API allocations, and jobs. APISIX does not call
forward-auth or count business API usage.

## Prepare the cluster

Install a supported k3s version on your nodes. The acceptance fixture pins
`v1.37.1+k3s1`. Keep the embedded network-policy controller enabled. On **every
server**, merge these settings into `/etc/rancher/k3s/config.yaml`, then restart
k3s. The same settings apply when you install a new server.

```yaml
disable:
  - traefik
  - servicelb
```

Keep CoreDNS. Keep metrics-server if you enable the optional HPA. Use at least
two schedulable nodes with capacity for an extra API and gateway during rollout.
The topology spread rules prefer different nodes. PodDisruptionBudgets preserve
one ready replica during voluntary eviction. They do not establish control-plane
HA or protect against simultaneous node failure. Provision that separately.

Restrict namespace write access. The ingress policies trust namespace and Pod
labels; anyone who can create a tunnel-labelled Pod can enter that boundary.
Do not grant application users permission to create Pods or change these labels.
Host-network traffic and cluster administrators are outside this Pod boundary.
Do not add public Services or ports for the gateway or API.

## Configure and deploy

Run commands from the repository root with the intended `KUBECONFIG` and context.
The fixed namespace is `hirune-kit`. If you change it, also change Service DNS
in `apisix/apisix.yaml`, NetworkPolicy namespace selectors, the secret example,
and the operator command's namespace constant.

1. Build and publish the canonical image from
   `compose/production/app/Dockerfile`. Set `images` in `kustomization.yaml` to
   your immutable tag or digest. Configure an image pull secret if required.
2. Set public HTTPS origins, SMTP, and non-secret options in `app.env`. Keep
   settings and encryption keys identical across API and worker replicas.
3. Copy `.envs/.example/production/k3s-secrets.yaml` to ignored
   `.envs/.production/k3s-secrets.yaml`. Replace all placeholders. Generate a
   base64-encoded 32-byte encryption key and an independent random rate-limit
   key. Set a shared `INTERNAL_PROXY_SECRET` only if trusted SSR servers need
   `X-Real-Client-IP`. Give the runtime, maintenance, and schema owner separate
   database credentials with the grants in [deployment](../README.md#database-roles-and-maintenance).
   Secret `stringData` accepts JSON directly; do not add dotenv single quotes.
4. Create a remotely managed Cloudflare Tunnel. Route your application hostname
   to `http://hirune-apisix.hirune-kit.svc.cluster.local:9080`. Set the tunnel token
   in `hirune-tunnel`. Cloudflared replicas share that tunnel. Restrict other
   origins and verify that Cloudflare supplies the client-IP header.
5. Prepare the namespace and secrets, then inspect the rendered resources:

   ```sh
   kubectl apply -f deploy/k3s/namespace.yaml
   kubectl apply -f .envs/.production/k3s-secrets.yaml
   kubectl kustomize deploy/k3s
   ```

6. For the first deployment, or an upgrade whose schema, settings, HTTP, and job
   payloads work with both generations, run:

   ```sh
   just deploy-k3s rolling
   ```

The command creates one migration Job, waits for success, applies runtime
resources, and waits for all Deployments. It replaces a finished migration Job
on the next invocation; an unfinished Job blocks another deployment. A migration
failure leaves existing compatible processes running and blocks the new rollout.
Inspect `kubectl logs -n hirune-kit job/hirune-migration` and fix the cause before
retrying. Do not run concurrent deploy commands. This helper is not a deployment
lock. Use a serialized release pipeline for production.

Do not use `kubectl apply -k deploy/k3s` as the deployment procedure. That starts
runtime changes without waiting for migration and cannot update a finished Job's
immutable template. A plain `kubectl rollout undo` also does not restore schema,
Secrets, or settings. Use an explicitly compatible release to recover.

The API rollout has `maxUnavailable: 0` and `maxSurge: 1`. HTTP readiness gates
Service traffic. TCP liveness avoids restarting a healthy process because of
database failure or HTTP admission pressure. A five-second pre-stop delay gives
Service endpoints time to converge. The API then drains for at most 30 seconds.
The 45-second termination grace also covers the default 35-second worker drain.
Increase grace when you increase the worker drain; pre-stop consumes that budget.

APISIX config files use individual read-only mounts. Its generated configuration,
PID, log, and temporary paths remain writable as required by the standard image.
Hashed ConfigMap references replace Pods when settings or routes change. The
gateway preserves `CF-Connecting-IP`, `X-Internal-Secret`, and `X-Real-Client-IP`.
It does not add trusted identity on behalf of arbitrary clients. Requests have
no automatic upstream retries because a retry can count an admission twice.
The 128 MiB body limit and 330-second timeouts cover default 100 MiB/300-second
file transfers. Keep these limits aligned when changing file settings and the
Cloudflare plan's upload limits.

## Incompatible upgrades

Use an expand/migrate/contract sequence when possible. If old code cannot use
the new schema or job payloads, plan an interruption and run:

```sh
kubectl delete -f deploy/k3s/autoscaling.yaml --ignore-not-found
just deploy-k3s drain
```

This stops **all API, worker, and maintenance** Pods and waits for their exit
before migration. It then restores the manifest's replica counts. A failed
migration leaves those processes stopped. Correct the migration and repeat the
drained deployment. A per-Deployment `Recreate` strategy alone cannot coordinate
old workers with new producers. Back up the database before destructive changes.

## Scaling and secrets

API and workers scale independently. `autoscaling.yaml` is optional and requires
metrics-server. It scales the API from two to four Pods using CPU utilization.
With pools of 10, the baseline needs up to 50 connections during one API surge
(2 API + 1 surge + 1 worker + 1 maintenance). With the HPA maximum, budget at
least 70, plus migration, management, worker surges, maintenance surges, and
operational reserve. Check the complete concurrent rollout against PostgreSQL's
connection limit before raising replicas or pool sizes.

```sh
kubectl apply -f deploy/k3s/autoscaling.yaml
kubectl -n hirune-kit exec deployment/hirune-api -- manage set-api-client-quota UUID 100 10000
```

Manual worker scaling must also be reflected in the desired manifests. A deploy
applies those replica counts again. Environment-based Secret changes require
`kubectl rollout restart` for affected Deployments; a Secret update alone does
not change existing process environments. Coordinate encryption-key rotation
across every process. Do not remove keys still needed by stored encrypted data.
Enable k3s secrets encryption at rest and restrict Secret read access.

Private SMTP CAs, custom integration credentials, and ADC files need explicit
read-only volume mounts, as described in [deployment](../README.md#smtp-trust).
The example does not install providers or initialize buckets/indexes. Use the
explicit management commands after provisioning and migration.

## Local acceptance

On a Linux Docker host, install `k3d v5.9.0` and `kubectl v1.37.1`. Build the canonical release
image. Reserve at least 20 GiB of free disk space for native snapshots, and then run:

```sh
K3S_TEST_IMAGE=hirune-kit:local just test-k3s
```

The Rust fixture creates and removes its own k3d cluster: one k3s server and one
agent. It imports local images and uses a native container snapshotter, the
Flannel host-gw backend, and a null kernel-message device for nested Docker hosts.
Network-policy enforcement remains enabled. It leaves the current
kubeconfig unchanged. It applies a disposable copy of this profile with two
API Pods on distinct nodes, APISIX, PostgreSQL, Mailpit, and probe Pods. Real
cloudflared replicas are set to zero. Local HTTP/mail and one database owner
are fixture-only substitutions; the separate production fixture verifies
HTTPS, STARTTLS, and restricted database roles.

The host kernel must support bridge filtering, IP sets, and `xt_physdev` for the
embedded policy controller. CI loads these modules on its disposable Ubuntu
runner. A nested host without those modules can run Pods while policy rule
installation fails. The acceptance command must fail in that case; successful
manifest application does not establish ingress isolation. Its failure output
includes the controller's policy-sync errors.

The check verifies signup, worker mail, forwarded identity, a retained session,
shared API quota across replacement, continuous authenticated traffic during
API and gateway rollouts, a config-only gateway restart, and denied direct
ingress. It also rejects an unfinished migration Job and exercises a coordinated
drained deployment with retained authority. It checks that at least two API replicas remain available during the
observed update. This is a rollout regression check, not a capacity benchmark,
control-plane failover test, long-upload drain proof, or real Tunnel acceptance.
Verify the real hostname, header provenance, mail, providers, backups, and node
drain on your target cluster before production use.

Review pinned k3s, k3d, kubectl, APISIX, and cloudflared versions together when
updating this profile. See [development](../../docs/development.md) and actual
[milestones](../../memory/README.md) for verification results.
