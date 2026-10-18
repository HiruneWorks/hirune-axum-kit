# k3s deployment

The production path is Cloudflare Tunnel → private APISIX → API Service → API
Pods. APISIX uses standalone YAML mode without etcd, an Admin API, or an ingress
controller. The gateway and API use only private ClusterIP Services. New backend
endpoints use the catch-all route. Add a controller when independently deployed
services need dynamic route ownership.

`base/` contains namespace-neutral runtime manifests and gateway configuration.
Ignored `.envs/.dev/k3s/` and `.envs/.prod/k3s/` overlays select images, namespaces,
settings, and generated Secrets. Follow [deployment](../README.md) to copy the
examples and use the canonical `deploy/justfile` commands.

## Prepare the cluster

Install k3s on your nodes. The acceptance fixture pins `v1.37.1+k3s1`.
On every server, merge these settings into `/etc/rancher/k3s/config.yaml`, then
restart k3s. Use the same settings when installing a new server.

```yaml
disable:
  - traefik
  - servicelb
```

Keep CoreDNS and the embedded network-policy controller. Keep metrics-server if
you enable HPA. Use at least two schedulable nodes with room for extra API,
worker, maintenance, and gateway Pods during rollout. Topology spread rules
prefer different nodes. PodDisruptionBudgets preserve one ready replica during
voluntary eviction. Provision control-plane availability separately.

Restrict namespace write access. The ingress policies allow only same-namespace
Pods with the expected labels: tunnel → gateway, gateway → API, and gateway →
Valkey. Valkey has no public listener or password; these namespace policies and
namespace write permissions establish its access boundary. A Pod in dev
cannot enter prod through these rules, even with the same labels. Anyone who can
create Pods inside the target namespace can impersonate these labels. Host-network
traffic and cluster administrators are outside this boundary. Do not add public
Services or ports for the gateway or API.

## Configure ingress and credentials

1. Publish the canonical image built from `compose/production/app/Dockerfile`.
   Set each overlay's image to the intended immutable tag or digest. Add an image
   pull secret if the registry requires one.
2. Set public HTTPS origins, SMTP, and runtime settings in the environment's
   `app.env`. Supply independent encryption and rate-limit keys in `secrets.env`.
   Set `INTERNAL_PROXY_SECRET` only when a trusted SSR server needs client-IP
   overrides. JSON values in Kubernetes env files have no outer dotenv quotes.
3. Provision separate runtime, maintenance, and schema-owner database roles.
   Set their URLs in the corresponding env files. The production profile uses
   external PostgreSQL. The [role contract](../README.md#database-roles-and-maintenance)
   defines the required permissions.
4. Create a remotely managed Cloudflare Tunnel for each environment. For namespace
   `hirune-prod`, route its public hostname to
   `http://hirune-apisix.hirune-prod.svc.cluster.local:9080`. Set that Tunnel's
   token in prod's `tunnel.env`. Dev uses its own hostname, namespace, and Tunnel.
   Both cloudflared replicas in one environment connect to the same Tunnel.
5. Verify that Cloudflare supplies `CF-Connecting-IP` and cannot reach a separate
   public origin. APISIX preserves it and the optional SSR override headers.
   PostgreSQL owns authentication and business API counters. APISIX uses Valkey
   for an additional broad IP limit. It does not run forward-auth.

APISIX obtains its namespace through the Kubernetes Downward API. Its route uses
`${{POD_NAMESPACE}}` in the API and Valkey Service DNS names. No Pod IP or fixed namespace is
stored in the gateway route. Increasing API replicas does not change Tunnel or
gateway configuration.

## Shared edge limits

Each environment runs two APISIX replicas and one Valkey 9.1.2 Pod. Both gateways
use the same counter store. The baseline has nine runtime Pods: two API, two
APISIX, two cloudflared, one worker, one maintenance, and one Valkey. The migration
Job runs separately. PostgreSQL and optional search/object storage stay external.

Set `EDGE_RATE_LIMIT_REQUESTS` in `gateway.env`. The example allows 60000 requests
per client IP per 60-second fixed window across both gateways. This is an initial
abuse ceiling, not measured service capacity. Tune it for real traffic and shared
NAT users. It covers all routes except exact GET/HEAD `/health/live` and
`/health/ready`. Authentication routes still enforce their lower PostgreSQL limits.
External API allocations stay strict and independent of this edge counter.

APISIX uses `limit-count` with `policy: redis` because Valkey speaks that protocol.
No Redis server or application Valkey client is installed. The private counter key
uses the same Cloudflare/secret-authenticated SSR IP selection as the backend.
Equivalent IPv6 forms and IPv4-mapped IPv6 share a budget. Forwarded headers do
not establish identity. Gateway-generated 429 responses use `edge_rate_limited`,
the common JSON error envelope, a generated request ID, `Retry-After`, and allowed
origin CORS. Backend 429 responses retain their own body and quota headers.

The gateway sets a 200 ms socket timeout for Valkey connect/send/read operations.
This is not an end-to-end request deadline. Counter failure or memory exhaustion
lets the request continue to the backend. Monitor Valkey health,
memory, APISIX counter errors, and rejection volume. PostgreSQL authentication
limits and API allocations still apply during a Valkey outage. Counter failure
does not make API or gateway readiness depend on Valkey.

Valkey has a 128 MiB data limit, `noeviction`, no disk persistence, and no PVC.
Its single-replica `Recreate` strategy prevents independent stores from splitting
one budget. Do not scale it or attach an HPA. Counter replacement intentionally
resets only the short edge windows. API/gateway rollouts and application drain
keep it running. This baseline does not provide Valkey failover. If the service
later needs highly available edge enforcement, use one replicated Valkey service
with coordinated primary failover and adapt the gateway connection configuration.

## Migration and rollout

From `deploy/`, run `just check prod`, then `just up prod`.
All resources are rendered and validated once before any cluster mutation.
The command applies the namespace and prerequisites, creates one migration Job,
waits for its success, starts and waits for Valkey, then applies the other runtime
Deployments. `just down <environment>` stops Valkey with the other Deployments.
A finished migration Job is replaced on the next release. An unfinished Job
blocks another deployment, including a Job that is pending or suspended.
A failed migration blocks runtime rollout. Inspect it with the selected target:

```sh
kubectl --context k3s-prod --namespace hirune-prod logs job/hirune-migration
```

`rolling` is the default. Existing API and worker Pods keep running during the
migration. New code, schema, settings, HTTP, and job payloads must work with both
generations. For example, add a new column first, deploy code that no longer uses
an old column, and delete the old column in a later compatible release. Automatic
migration execution does not establish compatibility with running old code.

The API uses `maxUnavailable: 0` and `maxSurge: 1`. HTTP readiness gates Service
traffic. TCP liveness avoids restarting healthy processes during database failure
or HTTP admission pressure. A five-second pre-stop delay gives Service endpoints
time to converge. The API then drains for at most 30 seconds. The 45-second
termination grace also covers the default 35-second worker drain. Increase grace
when increasing the worker drain; pre-stop consumes that budget.

If old code cannot use the new schema or job contract, schedule an interruption:

```sh
just up prod drain
```

This stops all API, worker, and maintenance Pods and waits for exit before
migration. It then restores desired replicas. A migration failure leaves those
processes stopped. APISIX, Tunnel connectors, and Valkey keep running. Correct the cause and repeat the drained deployment. Remove
HPAs from both the overlay and selected namespace before this operation; an HPA
can restore old replicas while the database is changing. A per-Deployment
`Recreate` strategy does not coordinate all application roles.

Serialize release commands. This helper is not a distributed deployment lock.
Do not deploy with a plain `kubectl apply -k`: it does not wait for migration and
cannot replace a completed Job's immutable template. A plain `rollout undo` does
not restore database schema or configuration. Recover with an explicitly
compatible release and the required retained encryption keys.

## Configuration, scaling, and storage

Kustomize generates hashed ConfigMaps and Secrets. A value change updates the
Pod template reference and replaces affected Pods. Schema-owner credential changes
replace only the migration reference. Application-secret changes replace API,
worker, maintenance, and gateway Pods because gateways reference the optional SSR
secret key. Gateways receive no database, SMTP, or encryption credentials.
Edge-limit changes replace only gateways. Valkey config changes replace only the
counter Pod. Tunnel-token changes replace cloudflared Pods.
Old generated objects remain for existing Pods and recovery. Remove unused objects
only after checking live Pods and retained ReplicaSets. Do not prune them blindly.

APISIX mounts configuration files read-only. Paths for its generated configuration,
PID, logs, and temporary data remain writable as required by the standard image.
It does not retry upstream requests automatically, because retries can duplicate
admission. Its 128 MiB body limit and 330-second timeouts cover the default
100 MiB/300-second file transfers. Keep these limits and the Cloudflare plan
aligned with application upload settings.

API, worker, APISIX, and cloudflared replica counts are independent. The optional
`autoscaling.yaml` uses CPU to scale API replicas from two to four. Copy it into an
environment overlay and add it to that overlay's `resources`. Keep metrics-server
enabled. When enabling HPA, remove the API Deployment's `spec.replicas` with an
overlay patch so later applies do not reset the autoscaled count. Follow the
[Kubernetes field-ownership migration steps](https://kubernetes.io/docs/tasks/run-application/horizontal-pod-autoscale/#migrating-deployments-and-statefulsets-to-horizontal-autoscaling)
when changing an existing Deployment to HPA control.
Removing a resource from an overlay does not delete it from Kubernetes;
remove an active HPA explicitly with the correct context and namespace.

With pools of 10, two API Pods plus one API surge, one worker, and one maintenance
process need 50 database connections. At the HPA maximum, budget at least 70,
plus migration, management, worker/maintenance surges, and reserve. Check complete
concurrent rollout capacity before increasing replicas or pool sizes. Manual
scaling must also be reflected in the overlay, because deployment reapplies the
desired replica counts.

API Pods contain no authoritative local data. PostgreSQL, SMTP, optional object
storage, and optional search are external. Kubernetes does not provision their
persistence or backups through these manifests. The optional [single-host data
profile](../data-services/README.md) supplies Docker Compose PostgreSQL/Meilisearch and
pgBackRest backups outside k3s on the same machine. Use read-only mounts for private
SMTP CAs or provider credential files when needed. Enable Kubernetes Secret
encryption at rest and restrict Secret read access. Coordinate key rotation across
all processes and retain keys required by stored encrypted data.

## Acceptance

On a Linux Docker host, install `k3d v5.9.0` and `kubectl v1.37.1`, build the
canonical release image, and reserve at least 20 GiB for native snapshots:

```sh
K3S_TEST_IMAGE=hirune-kit:local just test-k3s
```

The Rust fixture creates and removes its own two-node k3d cluster. It uses a native
container snapshotter, Flannel host-gw, and a null kernel-message device for nested
hosts. Network-policy enforcement stays enabled. It never changes the user's
kubeconfig. The isolated kubeconfig has an intentionally unusable current context;
operations must use the explicitly selected context.

The fixture deploys a disposable prod overlay with two API Pods on distinct nodes,
two APISIX Pods, Valkey, PostgreSQL, Mailpit, and probes. Real cloudflared replicas are zero. Local
HTTP/mail and one database owner are fixture-only substitutions. The separate
production fixture verifies HTTPS, STARTTLS, and restricted database roles.

Acceptance covers an unfinished migration guard, signup and mail, retained
sessions and API quota, continuous authenticated traffic during API/gateway
replacement, config-only gateway rollout, coordinated application drain, and
private ingress. Direct requests to both gateways verify one combined IP budget,
header spoofing, SSR trust, duplicate rejection, IP normalization, CORS, health
exceptions, gateway restart continuity, counter timeout/recovery, retained strict
PostgreSQL quota, and intentional reset on Valkey replacement. A same-name dev
sentinel and a separate dev Valkey retain their Pods and state across prod
operations. A dev Pod with tunnel labels must fail to reach prod gateway, API,
and Valkey. This does not establish real Tunnel connectivity,
control-plane failover, long-upload drain, or production capacity.
The continuity checks use a fixture-only 600-second window so slow rollouts do
not expire their counters. Production keeps its 60-second window.

The host kernel needs bridge filtering, IP sets, and `xt_physdev`. CI loads these
modules on its disposable Ubuntu runner. Missing modules can leave running Pods
with failed policy installation. The acceptance command must fail in that case;
its failure output includes policy-controller errors.

Review pinned k3s, k3d, kubectl, APISIX, Valkey, and cloudflared versions together. Verify
real-hostname identity headers, mail, providers, backups, recovery, and node drain
on the target cluster. See [development](../../docs/development.md) and actual
[milestones](../../memory/README.md) for validation evidence.
