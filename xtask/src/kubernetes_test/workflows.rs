use super::*;
use serde::Deserialize;
use std::collections::BTreeMap;

pub(super) fn profile() -> Result<String> {
    let profile = std::env::var("K6_PROFILE").unwrap_or_else(|_| "smoke".into());
    ensure!(
        matches!(profile.as_str(), "smoke" | "extended"),
        "K6_PROFILE must be smoke or extended"
    );
    Ok(profile)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSummary {
    requests: u64,
    failed_requests: f64,
    failed_checks: u64,
    workflow_failures: u64,
    completed_workflows: u64,
    session_revocations: u64,
    p95_milliseconds: f64,
    failed_assertions: Vec<FailedAssertion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FailedAssertion {
    name: String,
    failures: u64,
}

fn collect_scripts(
    directory: &Path,
    root: &Path,
    data: &mut BTreeMap<String, String>,
    items: &mut Vec<Value>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_scripts(&path, root, data, items)?;
        } else if path.extension().is_some_and(|extension| extension == "js") {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            let key = relative.replace('/', "__");
            data.insert(key.clone(), fs::read_to_string(&path)?);
            items.push(json!({"key":key,"path":relative}));
        }
    }
    Ok(())
}

impl Fixture {
    pub(super) fn verify_workflows(&self) -> Result<()> {
        let profile = profile()?;
        let count = if profile == "extended" { 20 } else { 2 };
        println!("{}", self.probe("workflow-seed")?.trim());
        let accounts = self.probe("workflow-accounts")?;
        let users: Value =
            serde_json::from_str(&accounts).context("invalid workflow account fixture")?;
        ensure!(
            users.as_array().is_some_and(|users| users.len() == count),
            "wrong workflow account count"
        );
        for account in users.as_array().context("workflow accounts missing")? {
            let client = account["api_client_id"]
                .as_str()
                .context("workflow client ID missing")?;
            Uuid::parse_str(client)?;
            crate::run(&mut self.kubectl(&[
                "exec",
                "deployment/hirune-api",
                "--",
                "manage",
                "set-api-client-quota",
                client,
                "100000",
                "1000000",
            ]))?;
        }
        let gateways = self.json(&["get", "pods", "-l", "app.kubernetes.io/component=gateway"])?;
        let addresses: Vec<_> = gateways["items"]
            .as_array()
            .context("gateway Pods missing")?
            .iter()
            .filter(|pod| pod["metadata"]["deletionTimestamp"].is_null())
            .filter_map(|pod| pod["status"]["podIP"].as_str())
            .collect();
        ensure!(
            addresses.len() == 2,
            "workflows require two live gateway Pods"
        );
        let mut scripts = BTreeMap::new();
        let mut items = Vec::new();
        collect_scripts(
            Path::new("tests/load"),
            Path::new("tests/load"),
            &mut scripts,
            &mut items,
        )?;
        self.apply(json!({"apiVersion":"v1","kind":"List","items":[
            {"apiVersion":"v1","kind":"Secret","metadata":{"name":"fixture-workflow-accounts"},
                "stringData":{"accounts":accounts}},
            {"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"fixture-workflow-scripts"},"data":scripts}
        ]}))?;
        let deadline = if profile == "extended" { 600 } else { 300 };
        let job = json!({"apiVersion":"batch/v1","kind":"Job","metadata":{"name":"fixture-workflows"},
        "spec":{"backoffLimit":0,"activeDeadlineSeconds":deadline,"template":{
            "metadata":{"labels":{"app.kubernetes.io/part-of":"hirune-kit","app.kubernetes.io/component":"tunnel"}},
            "spec":{"automountServiceAccountToken":false,"restartPolicy":"Never",
                "securityContext":{"runAsNonRoot":true,"runAsUser":12345,"runAsGroup":12345,"fsGroup":12345},
                "containers":[{"name":"k6","image":"grafana/k6:2.2.0",
                    "args":["run","--quiet","--log-output=none","/scripts/mixed.js"],
                    "securityContext":{"allowPrivilegeEscalation":false,"readOnlyRootFilesystem":true,"capabilities":{"drop":["ALL"]}},
                    "resources":{"requests":{"cpu":"250m","memory":"128Mi"},"limits":{"cpu":"2","memory":"512Mi"}},
                    "env":[{"name":"FIRST_API_URL","value":format!("http://{}:9080",addresses[0])},
                        {"name":"SECOND_API_URL","value":format!("http://{}:9080",addresses[1])},
                        {"name":"K6_PROFILE","value":profile},{"name":"K3S_E2E","value":"true"},
                        {"name":"K6_NO_USAGE_REPORT","value":"true"},
                        {"name":"LOAD_ACCOUNTS","valueFrom":{"secretKeyRef":{"name":"fixture-workflow-accounts","key":"accounts"}}}],
                    "volumeMounts":[{"name":"scripts","mountPath":"/scripts","readOnly":true},{"name":"temporary","mountPath":"/tmp"}]}],
                "volumes":[{"name":"scripts","configMap":{"name":"fixture-workflow-scripts","items":items}},
                    {"name":"temporary","emptyDir":{"medium":"Memory","sizeLimit":"16Mi"}}]
            }}}});
        let result = crate::kubernetes_jobs::run(
            &self.deployment()?.target,
            job,
            Duration::from_secs(deadline + 30),
        );
        let logs = self.read(&["logs", "job/fixture-workflows"])?;
        // Only typed aggregate counters are emitted. Raw logs can contain bearer URLs.
        let summary: WorkflowSummary = logs
            .lines()
            .find_map(|line| line.strip_prefix("K3S_E2E_RESULT "))
            .context("Kubernetes workload did not report its aggregate result")
            .and_then(|line| {
                serde_json::from_str(line).context("invalid Kubernetes workflow summary")
            })?;
        println!(
            "Kubernetes workflows: {} requests, {} completed workflows, {} failed checks, {:.2} ms p95.",
            summary.requests,
            summary.completed_workflows,
            summary.failed_checks,
            summary.p95_milliseconds
        );
        for assertion in &summary.failed_assertions {
            println!(
                "Failed workflow check: {} ({} failures).",
                assertion.name, assertion.failures
            );
        }
        result?;
        ensure!(
            summary.requests > 0
                && summary.failed_requests == 0.0
                && summary.failed_checks == 0
                && summary.workflow_failures == 0
                && summary.completed_workflows >= count as u64
                && summary.session_revocations == count as u64
                && summary.p95_milliseconds < 2000.0,
            "deployed Kubernetes workflow thresholds failed"
        );
        // The terminal tunnel-labelled Pod must not block the later full shutdown.
        crate::run(&mut self.kubectl(&[
            "delete",
            "job",
            "fixture-workflows",
            "--cascade=foreground",
            "--wait=true",
        ]))?;
        // Credentials are scoped to this one Job and are not reused after token rotation.
        crate::run(&mut self.kubectl(&["delete", "secret", "fixture-workflow-accounts"]))?;
        Ok(())
    }
}
