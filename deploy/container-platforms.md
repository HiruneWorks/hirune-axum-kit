# Linux container platforms

Hirune Axum Kit container builds support `linux/amd64` and `linux/arm64`.
The Rust compiler runs on the builder's native platform. ARM64 builds use
`aarch64-unknown-linux-gnu` and the ARM64 GNU linker. They do not run an
ARM64 Rust compiler under emulation.

Build a local ARM64 candidate on an x86 Docker host:

```sh
docker buildx build --platform linux/arm64 --load --file compose/production/app/Dockerfile --tag service:arm64 .
```

Use a BuildKit builder with foreign-platform execution support for package
installation in the target runtime layer. Compilation uses the cross toolchain.
Production servers pull published images by digest. Do not compile Rust there.

## CI acceptance and publication

The owning quality or release workflow calls `.github/workflows/arm64.yml`.
It exports the exact cross-built image and checks its configuration and
filesystem layer hashes after loading it on a native ARM64 runner.
Docker's local image identifier is not used as a cross-store identity.

Publication waits for both platform gates. The publisher pushes the accepted
platform images, then creates one versioned index for both Linux architectures.
It does not rebuild the ARM64 image during publication. Read back the index
digest and both platform descriptors before deployment.

Native ARM64 acceptance starts PostgreSQL, applies the real migrations, and runs
API, worker and maintenance processes. It also checks role grants, encrypted
backups, the read-only backup scheduler and physical recovery with the exact
ARM64 PostgreSQL candidate.

Application images publish only on `v<workspace-version>` tags after the
existing quality and production gates. PostgreSQL images keep independent
`postgres-v<version>` tags and require backup/recovery acceptance on both CPUs.
Branch, pull-request and manual validation runs do not publish template images.
