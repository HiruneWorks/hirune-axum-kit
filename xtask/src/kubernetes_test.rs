use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use uuid::Uuid;

const NODE_IMAGE: &str = "rancher/k3s:v1.37.1-k3s1";
const IMAGES: &[&str] = &[
    "apache/apisix:3.19.0-debian",
    "postgres:18-alpine",
    "axllent/mailpit:v1.31.5",
    "python:3.14-alpine",
    "rancher/mirrored-pause:3.10.2",
    "rancher/klipper-helm:v0.13.3-build20260727",
];

pub fn verify() -> Result<()> {
    let image = std::env::var("K3S_TEST_IMAGE")
        .context("set K3S_TEST_IMAGE to the canonical release application image")?;
    let fixture = Fixture {
        cluster: format!("kit-{}", &Uuid::now_v7().simple().to_string()[..12]),
        directory: tempfile::tempdir()?,
    };
    let result = fixture.verify(&image);
    if result.is_err() {
        let _ = crate::run(&mut fixture.kubectl(&["get", "nodes", "-o", "wide"]));
        let _ = crate::run(&mut fixture.kubectl(&["get", "pods", "-o", "wide"]));
        let _ = crate::run(&mut fixture.kubectl(&[
            "get",
            "events",
            "--field-selector",
            "type=Warning",
        ]));
        for role in ["server", "agent"] {
            let node = format!("k3d-{}-{role}-0", fixture.cluster);
            if let Ok(logs) = Command::new("docker")
                .args(["logs", "--tail", "2000", &node])
                .output()
            {
                let logs = format!(
                    "{}{}",
                    String::from_utf8_lossy(&logs.stdout),
                    String::from_utf8_lossy(&logs.stderr)
                );
                let errors: Vec<_> = logs
                    .lines()
                    .filter(|line| {
                        line.contains("Aborting sync.") || line.contains("Extension physdev")
                    })
                    .collect();
                for line in errors.iter().rev().take(3) {
                    eprintln!("{node}: {line}");
                }
            }
        }
    }
    result?;
    println!(
        "Two-node k3s acceptance passed: two API replicas, live rolling updates, shared sessions and quota, gateway configuration rollout, and private ingress policies. Real Cloudflare Tunnel was not connected."
    );
    Ok(())
}

struct Fixture {
    cluster: String,
    directory: TempDir,
}

impl Fixture {
    fn kubectl(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new("kubectl");
        command
            .arg("--kubeconfig")
            .arg(self.directory.path().join("kubeconfig"))
            .args(["--namespace", crate::kubernetes::NAMESPACE])
            .args(arguments);
        command
    }

    fn read(&self, arguments: &[&str]) -> Result<String> {
        let output = self.kubectl(arguments).output()?;
        ensure!(
            output.status.success(),
            "k3s fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?)
    }

    fn json(&self, arguments: &[&str]) -> Result<Value> {
        let mut command = self.kubectl(arguments);
        let output = command.arg("-o").arg("json").output()?;
        ensure!(output.status.success(), "k3s fixture read failed");
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    fn apply(&self, value: Value) -> Result<()> {
        // Secret values go through stdin and never appear in arguments or output.
        let mut child = self
            .kubectl(&["apply", "-f", "-"])
            .stdin(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .context("kubectl stdin missing")?
            .write_all(&serde_json::to_vec(&value)?)?;
        ensure!(child.wait()?.success(), "fixture resource apply failed");
        Ok(())
    }

    fn probe(&self, operation: &str) -> Result<String> {
        self.read(&[
            "exec",
            "trusted-probe",
            "--",
            "python",
            "/checks/probe.py",
            operation,
        ])
    }

    fn deploy(&self, mode: &str) -> Result<()> {
        crate::run(
            Command::new(std::env::current_exe()?)
                .args(["deploy-k3s", mode])
                .env("KUBECONFIG", self.directory.path().join("kubeconfig"))
                .env("K3S_DIRECTORY", self.directory.path().join("profile")),
        )
    }

    fn verify(&self, image: &str) -> Result<()> {
        for image in std::iter::once(NODE_IMAGE).chain(IMAGES.iter().copied()) {
            crate::run(Command::new("docker").args(["pull", image]))?;
        }
        let api_address = TcpListener::bind("127.0.0.1:0")?.local_addr()?.to_string();
        crate::run(Command::new("k3d").args([
            "cluster",
            "create",
            &self.cluster,
            "--image",
            NODE_IMAGE,
            "--servers",
            "1",
            "--agents",
            "1",
            "--no-lb",
            "--api-port",
            &api_address,
            "--kubeconfig-update-default=false",
            "--kubeconfig-switch-context=false",
            "--k3s-arg",
            "--disable=traefik,servicelb,metrics-server,local-storage@server:0",
            "--k3s-arg",
            "--snapshotter=native@all",
            "--k3s-arg",
            "--flannel-backend=host-gw@server:0",
            "--volume",
            "/dev/null:/dev/kmsg@all",
            "--wait",
            "--timeout",
            "180s",
        ]))?;
        let config = Command::new("k3d")
            .args(["kubeconfig", "get", &self.cluster])
            .output()?;
        ensure!(config.status.success(), "cannot get isolated kubeconfig");
        fs::write(self.directory.path().join("kubeconfig"), &config.stdout)?;
        for image in std::iter::once(image).chain(IMAGES.iter().copied()) {
            self.import_image(image)?;
        }
        // Import the distribution's exact DNS image for hosts whose node containers cannot pull.
        let dns = self.json(&["get", "deployment", "coredns", "-n", "kube-system"])?;
        let dns_image = dns["spec"]["template"]["spec"]["containers"][0]["image"]
            .as_str()
            .context("CoreDNS image missing")?;
        crate::run(Command::new("docker").args(["pull", dns_image]))?;
        self.import_image(dns_image)?;
        crate::run(&mut self.kubectl(&[
            "wait",
            "--for=condition=Ready",
            "node",
            "--all",
            "--timeout=180s",
        ]))?;
        crate::run(&mut self.kubectl(&[
            "wait",
            "--for=condition=DiskPressure=false",
            "node",
            "--all",
            "--timeout=180s",
        ]))?;
        ensure!(
            self.json(&["get", "nodes"])?["items"]
                .as_array()
                .is_some_and(|nodes| nodes.len() == 2),
            "the fixture needs two registered k3s nodes"
        );
        crate::run(&mut self.kubectl(&[
            "rollout",
            "status",
            "deployment/coredns",
            "-n",
            "kube-system",
            "--timeout=180s",
        ]))?;
        self.prepare(image)?;
        self.check_unfinished_migration()?;
        self.deploy("rolling")?;
        self.wait_probe("trusted-probe")?;
        self.wait_probe("denied-probe")?;
        let pods = self.api_pods()?;
        ensure!(
            pods.len() == 2 && pods[0]["spec"]["nodeName"] != pods[1]["spec"]["nodeName"],
            "the two API replicas must run on different nodes"
        );
        println!("API replicas are ready on two distinct k3s nodes.");
        println!("{}", self.probe("prepare")?.trim());
        let client_id = self.probe("client-id")?;
        let client_id = client_id.trim();
        Uuid::parse_str(client_id)?;
        crate::run(&mut self.kubectl(&[
            "exec",
            "deployment/hirune-api",
            "--",
            "manage",
            "set-api-client-quota",
            client_id,
            "100000",
            "3",
        ]))?;
        self.probe("consume-two")?;
        self.probe("start-traffic")?;
        crate::run(&mut self.kubectl(&["rollout", "restart", "deployment/hirune-api"]))?;
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            let deployment = self.json(&["get", "deployment", "hirune-api"])?;
            ensure!(
                deployment["status"]["availableReplicas"]
                    .as_u64()
                    .unwrap_or(0)
                    >= 2,
                "rolling update reduced API availability below two replicas"
            );
            let changed = self
                .api_pods()?
                .iter()
                .filter(|pod| {
                    pod["metadata"]["deletionTimestamp"].is_null()
                        && !pods
                            .iter()
                            .any(|old| old["metadata"]["uid"] == pod["metadata"]["uid"])
                })
                .count()
                == 2;
            if changed && deployment["status"]["updatedReplicas"] == 2 {
                break;
            }
            ensure!(Instant::now() < deadline, "API rolling update timed out");
            thread::sleep(Duration::from_millis(200));
        }
        crate::run(&mut self.kubectl(&[
            "rollout",
            "status",
            "deployment/hirune-api",
            "--timeout=180s",
        ]))?;
        let replaced = self.api_pods()?;
        let ready: Vec<_> = replaced
            .iter()
            .filter(|pod| {
                pod["metadata"]["deletionTimestamp"].is_null()
                    && pod["status"]["conditions"]
                        .as_array()
                        .is_some_and(|conditions| {
                            conditions.iter().any(|condition| {
                                condition["type"] == "Ready" && condition["status"] == "True"
                            })
                        })
            })
            .collect();
        ensure!(
            ready.len() == 2 && ready[0]["spec"]["nodeName"] != ready[1]["spec"]["nodeName"],
            "replaced API replicas must be ready on two distinct nodes"
        );
        println!("{}", self.probe("stop-traffic")?.trim());
        println!("{}", self.probe("after-rollout")?.trim());
        self.probe("start-traffic")?;
        let gateway = self.json(&["get", "deployment", "hirune-apisix"])?;
        let api_before_gateway = self.api_pods()?;
        let route = self.directory.path().join("profile/apisix/apisix.yaml");
        let contents =
            fs::read_to_string(&route)?.replace("#END", "# configuration rollout acceptance\n#END");
        fs::write(route, contents)?;
        self.deploy("rolling")?;
        let updated = self.json(&["get", "deployment", "hirune-apisix"])?;
        ensure!(
            gateway["spec"]["template"] != updated["spec"]["template"],
            "gateway ConfigMap did not trigger a rollout"
        );
        ensure!(
            self.api_pods()?.iter().all(|pod| api_before_gateway
                .iter()
                .any(|old| old["metadata"]["uid"] == pod["metadata"]["uid"])),
            "a gateway-only config change replaced API Pods"
        );
        println!("{}", self.probe("stop-traffic")?.trim());
        self.deploy("drain")?;
        println!("{}", self.probe("retained-authority")?.trim());
        println!("{}", self.probe("private-ingress")?.trim());
        println!(
            "{}",
            self.read(&[
                "exec",
                "denied-probe",
                "--",
                "python",
                "/checks/probe.py",
                "denied-ingress"
            ])?
            .trim()
        );
        Ok(())
    }

    fn api_pods(&self) -> Result<Vec<Value>> {
        let value = self.json(&["get", "pods", "-l", "app.kubernetes.io/component=api"])?;
        Ok(value["items"]
            .as_array()
            .context("Pod list missing")?
            .clone())
    }

    fn import_image(&self, image: &str) -> Result<()> {
        // k3d's importer assumes overlayfs. Match the native snapshotter explicitly.
        for role in ["server", "agent"] {
            let node = format!("k3d-{}-{role}-0", self.cluster);
            let mut save = Command::new("docker")
                .args(["image", "save", image])
                .stdout(Stdio::piped())
                .spawn()?;
            let result = Command::new("docker")
                .args([
                    "exec",
                    "-i",
                    &node,
                    "ctr",
                    "--address=/run/k3s/containerd/containerd.sock",
                    "--namespace=k8s.io",
                    "images",
                    "import",
                    "--local",
                    "--snapshotter=native",
                    "-",
                ])
                .stdin(save.stdout.take().context("docker save stdout missing")?)
                .status();
            let saved = save.wait()?;
            ensure!(
                result?.success() && saved.success(),
                "native image import failed for {image}"
            );
        }
        Ok(())
    }

    fn check_unfinished_migration(&self) -> Result<()> {
        self.apply(json!({"apiVersion":"batch/v1","kind":"Job","metadata":{"name":"hirune-migration"},
            "spec":{"suspend":true,"template":{"spec":{"restartPolicy":"Never",
                "containers":[{"name":"migration","image":"python:3.14-alpine","command":["true"]}]}}}}))?;
        let output = Command::new(std::env::current_exe()?)
            .args(["deploy-k3s", "rolling"])
            .env("KUBECONFIG", self.directory.path().join("kubeconfig"))
            .env("K3S_DIRECTORY", self.directory.path().join("profile"))
            .output()?;
        ensure!(
            !output.status.success()
                && String::from_utf8_lossy(&output.stderr).contains("unfinished"),
            "an unfinished migration must block deployment"
        );
        crate::run(&mut self.kubectl(&["delete", "job", "hirune-migration", "--wait=true"]))?;
        println!("An unfinished migration Job correctly blocked deployment.");
        Ok(())
    }

    fn wait_probe(&self, name: &str) -> Result<()> {
        crate::run(&mut self.kubectl(&[
            "wait",
            "--for=condition=Ready",
            &format!("pod/{name}"),
            "--timeout=180s",
        ]))
    }

    fn prepare(&self, image: &str) -> Result<()> {
        let profile = self.directory.path().join("profile");
        copy_directory(Path::new("deploy/k3s"), &profile)?;
        let (name, tag) = image
            .rsplit_once(':')
            .context("K3S_TEST_IMAGE needs an explicit tag")?;
        let path = profile.join("kustomization.yaml");
        let contents = fs::read_to_string(&path)?
            .replace("registry.example.com/your-project/hirune-kit", name)
            .replace("newTag: 0.1.0", &format!("newTag: {tag}"));
        fs::write(path, contents)?;
        let path = profile.join("tunnel.yaml");
        fs::write(
            &path,
            fs::read_to_string(&path)?.replace("replicas: 2", "replicas: 0"),
        )?;
        let path = profile.join("app.env");
        fs::write(
            &path,
            fs::read_to_string(&path)?
                .replace("APP_ENV=production", "APP_ENV=test")
                .replace("https://app.example.com/", "http://localhost/")
                .replace("https://app.example.com", "http://localhost")
                .replace("smtp.example.com", "mailpit")
                .replace("SMTP_PORT=587", "SMTP_PORT=1025")
                .replace("SMTP_STARTTLS=true", "SMTP_STARTTLS=false"),
        )?;
        crate::run(&mut self.kubectl(&["apply", "-f", "deploy/k3s/namespace.yaml"]))?;
        let password = auth_core::token::generate_secure_token();
        let url = format!("postgres://kit:{password}@postgres:5432/kit");
        let key = base64::engine::general_purpose::STANDARD.encode([17_u8; 32]);
        let mut resources = vec![
            secret(
                "hirune-app-secrets",
                json!({"ACTIVE_SECRET_KEY":"test-v1", "SECRET_KEYS":json!({"test-v1":key}).to_string(),
                "RATE_LIMIT_KEY":auth_core::token::generate_secure_token(), "INTERNAL_PROXY_SECRET":auth_core::token::generate_secure_token()}),
            ),
            secret("hirune-runtime-database", json!({"DATABASE_URL":url})),
            secret("hirune-maintenance-database", json!({"DATABASE_URL":url})),
            secret(
                "hirune-migration-database",
                json!({"MIGRATION_DATABASE_URL":url}),
            ),
            secret(
                "hirune-tunnel",
                json!({"TUNNEL_TOKEN":"not-connected-in-local-test"}),
            ),
            json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"probe-checks"},
                "data":{"probe.py":include_str!("../../deploy/k3s/verification/probe.py")}}),
        ];
        for (name, image, port, env) in [
            (
                "postgres",
                "postgres:18-alpine",
                5432,
                json!([
                {"name":"POSTGRES_USER","value":"kit"},{"name":"POSTGRES_DB","value":"kit"},
                {"name":"POSTGRES_PASSWORD","value":password} ]),
            ),
            ("mailpit", "axllent/mailpit:v1.31.5", 1025, json!([])),
        ] {
            resources.push(json!({"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":name},
                "spec":{"replicas":1,"selector":{"matchLabels":{"fixture":name}},"template":{
                    "metadata":{"labels":{"fixture":name}},"spec":{"automountServiceAccountToken":false,
                    "containers":[{"name":name,"image":image,"env":env,"ports":[{"containerPort":port}],
                    "readinessProbe":{"tcpSocket":{"port":port},"periodSeconds":2}}]}}}}));
            resources.push(json!({"apiVersion":"v1","kind":"Service","metadata":{"name":name},
                "spec":{"selector":{"fixture":name},"ports":if name == "mailpit" {json!([
                    {"name":"smtp","port":1025},{"name":"http","port":8025}])} else {json!([{"port":port}])}}}));
        }
        resources.push(json!({"apiVersion":"networking.k8s.io/v1","kind":"NetworkPolicy",
            "metadata":{"name":"fixture-database-mail"},"spec":{"podSelector":{"matchExpressions":[
                {"key":"fixture","operator":"In","values":["postgres","mailpit"]}]},"policyTypes":["Ingress"],
                "ingress":[{"from":[{"podSelector":{}}]}]}}));
        for (name, component) in [("trusted-probe", "tunnel"), ("denied-probe", "probe")] {
            resources.push(json!({"apiVersion":"v1","kind":"Pod","metadata":{"name":name,
                "labels":{"app.kubernetes.io/part-of":"hirune-kit","app.kubernetes.io/component":component}},
                "spec":{"automountServiceAccountToken":false,"containers":[{"name":"probe","image":"python:3.14-alpine",
                    "command":["python","-c","import time; time.sleep(3600)"],
                    "envFrom":[{"secretRef":{"name":"hirune-app-secrets"}}],
                    "volumeMounts":[{"name":"checks","mountPath":"/checks","readOnly":true}]}],
                    "volumes":[{"name":"checks","configMap":{"name":"probe-checks"}}]}}));
        }
        self.apply(json!({"apiVersion":"v1","kind":"List","items":resources}))?;
        for name in ["postgres", "mailpit"] {
            crate::run(&mut self.kubectl(&[
                "rollout",
                "status",
                &format!("deployment/{name}"),
                "--timeout=180s",
            ]))?;
        }
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new("k3d")
            .args(["cluster", "delete", &self.cluster])
            .env("KUBECONFIG", self.directory.path().join("kubeconfig"))
            .status();
    }
}

fn secret(name: &str, data: Value) -> Value {
    json!({"apiVersion":"v1","kind":"Secret","metadata":{"name":name},"stringData":data})
}

fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}
