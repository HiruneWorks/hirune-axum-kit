import http from 'k6/http';
import { check, fail, sleep } from 'k6';
import { Counter, Trend } from 'k6/metrics';

const accounts = JSON.parse(__ENV.LOAD_ACCOUNTS);
const apiUrls = [__ENV.FIRST_API_URL, __ENV.SECOND_API_URL];
const extended = __ENV.K6_PROFILE === 'extended';
const virtualUsers = extended ? 20 : 2;
const completedWorkflows = new Counter('completed_workflows');
const sessionRevocations = new Counter('session_revocations');
const workflowFailures = new Counter('workflow_failures');
const searchVisibility = new Trend('search_visibility_ms', true);
const thresholds = {
  http_req_failed: ['rate==0'],
  http_req_duration: ['p(95)<2000'],
  checks: ['rate==1'],
  session_revocations: [`count==${virtualUsers}`],
  workflow_failures: ['count==0'],
  search_visibility_ms: ['p(95)<10000'],
};
for (let virtualUser = 1; virtualUser <= virtualUsers; virtualUser += 1) {
  thresholds[`completed_workflows{virtual_user:${virtualUser}}`] = ['count>0'];
}

export const options = {
  scenarios: {
    mixed: {
      executor: 'constant-vus',
      vus: virtualUsers,
      duration: extended ? '2m' : '10s',
      gracefulStop: '30s',
    },
  },
  thresholds,
};

let currentToken;
let revocationCompleted = false;

function requireCheck(condition, name) {
  if (!check(condition, { [name]: value => value })) {
    fail(`Mixed workload check failed: ${name}`);
  }
}

function request(baseUrl, method, path, name, token, body = null, expectedStatus = 200, extraHeaders = {}) {
  const response = http.request(method, `${baseUrl}${path}`, body, {
    headers: { Authorization: `Bearer ${token}`, ...extraHeaders },
    tags: { name },
    timeout: '15s',
    redirects: 0,
    responseCallback: http.expectedStatuses(expectedStatus),
  });
  if (response.status !== expectedStatus) {
    let errorCode = 'unparsed_response';
    try {
      const candidate = response.json('error.code');
      if (typeof candidate === 'string' && /^[a-z0-9_]{1,64}$/.test(candidate)) {
        errorCode = candidate;
      }
    } catch (_) {
      // A failed binary response need not contain a JSON error envelope.
    }
    console.error(`${name}: expected ${expectedStatus}, received ${response.status}, code=${errorCode}`);
  }
  requireCheck(response.status === expectedStatus, `${name}: expected HTTP status`);
  return response;
}

function json(response) {
  try {
    return response.json();
  } catch (_) {
    requireCheck(false, 'response contains valid JSON');
  }
}

function responseHeader(response, name) {
  const key = Object.keys(response.headers).find(key => key.toLowerCase() === name.toLowerCase());
  return response.headers[key];
}

function jsonRequest(baseUrl, method, path, name, token, body, expectedStatus = 200) {
  return request(baseUrl, method, path, name, token, JSON.stringify(body), expectedStatus, {
    'Content-Type': 'application/json',
  });
}

function verifyOrganizationPagination(firstApi, secondApi, account) {
  const firstPage = json(request(firstApi, 'GET', '/api/v1/organizations?limit=1',
    'organizations_first_page', currentToken));
  requireCheck(firstPage.items.length === 1 && typeof firstPage.next_cursor === 'string',
    'organization first page has one item and a cursor');
  const secondPage = json(request(secondApi, 'GET',
    `/api/v1/organizations?limit=1&cursor=${encodeURIComponent(firstPage.next_cursor)}`,
    'organizations_second_page', currentToken));
  requireCheck(secondPage.items.length === 1 && secondPage.next_cursor === null,
    'organization second page completes pagination');
  const identifiers = [firstPage.items[0].id, secondPage.items[0].id];
  requireCheck(new Set(identifiers).size === 2
    && account.organization_ids.every(identifier => identifiers.includes(identifier)),
  'pagination returns exactly the current account organizations');
}

function renameOrganization(firstApi, secondApi, organizationPath) {
  const organization = json(request(secondApi, 'GET', organizationPath,
    'organization_read', currentToken));
  const name = `Load account ${__VU} iteration ${__ITER}`;
  const renamed = json(jsonRequest(firstApi, 'PUT', organizationPath,
    'organization_rename', currentToken, { name, expected_version: organization.version }));
  requireCheck(renamed.name === name && renamed.version === organization.version + 1,
    'organization write advances its version');
  const observed = json(request(secondApi, 'GET', organizationPath,
    'organization_read_after_write', currentToken));
  requireCheck(observed.name === name && observed.version === renamed.version,
    'organization write is visible on the other API');
  jsonRequest(secondApi, 'PUT', organizationPath, 'organization_stale_write', currentToken,
    { name: 'This stale write must fail', expected_version: organization.version }, 409);
}

function verifySessions(firstApi, secondApi, account) {
  if (!revocationCompleted) {
    request(secondApi, 'GET', '/api/v1/account', 'revocable_session_before_revoke', account.revocable_token);
    request(firstApi, 'DELETE', `/api/v1/sessions/${account.revocable_session_id}`,
      'session_revoke', currentToken, null, 204);
    revocationCompleted = true;
    sessionRevocations.add(1);
  }
  request(secondApi, 'GET', '/api/v1/account', 'revoked_session_rejected', account.revocable_token, null, 401);
  const previousToken = currentToken;
  const rotated = json(request(firstApi, 'POST', '/api/v1/sessions/current/rotate',
    'session_rotate', currentToken));
  requireCheck(rotated.status === 'authenticated' && typeof rotated.token === 'string'
    && rotated.token !== previousToken, 'session rotation produces a new credential');
  currentToken = rotated.token;
  request(secondApi, 'GET', '/api/v1/account', 'rotated_old_token_rejected', previousToken, null, 401);
  const authenticated = json(request(secondApi, 'GET', '/api/v1/account',
    'rotated_token_accepted', currentToken));
  requireCheck(authenticated.id === account.account_id && authenticated.verified,
    'new session token retains the correct verified account');
}

function transferOriginal(firstApi, secondApi, organizationPath, foreignOrganizationPath) {
  const payload = `untrusted-original-vu-${__VU}-iteration-${__ITER}\0`.repeat(512);
  const searchTerm = `vu${__VU}iteration${__ITER}original`;
  const filesPath = `${organizationPath}/files`;
  const reserved = json(jsonRequest(firstApi, 'POST', filesPath, 'file_reserve', currentToken,
    { filename: `${searchTerm}.bin`, media_type: 'application/octet-stream', byte_size: payload.length }, 201));
  requireCheck(reserved.state === 'uploading' && typeof reserved.id === 'string',
    'file reservation starts in uploading state');
  const filePath = `${filesPath}/${reserved.id}`;
  const contentPath = `${filePath}/content`;
  const uploaded = json(request(firstApi, 'PUT', contentPath, 'file_upload', currentToken,
    payload, 200, { 'Content-Type': 'application/octet-stream' }));
  requireCheck(uploaded.state === 'ready' && uploaded.byte_size === payload.length,
    'upload completes with the declared size');
  const downloaded = request(secondApi, 'GET', contentPath, 'file_download', currentToken);
  requireCheck(downloaded.body === payload, 'download preserves original bytes');
  requireCheck(downloaded.headers['Content-Type'] === 'application/octet-stream'
    && downloaded.headers['X-Content-Type-Options'] === 'nosniff'
    && downloaded.headers['Content-Disposition'].startsWith('attachment;'),
  'original download retains attachment protections');
  const head = request(secondApi, 'HEAD', contentPath, 'file_head', currentToken);
  requireCheck(Number(head.headers['Content-Length']) === payload.length,
    'HEAD exposes the stored original length');
  const ranged = request(secondApi, 'GET', contentPath, 'file_range', currentToken,
    null, 206, { Range: 'bytes=2-9' });
  requireCheck(ranged.body === payload.slice(2, 10)
    && ranged.headers['Content-Range'] === `bytes 2-9/${payload.length}`,
  'range download returns the requested original bytes');
  request(secondApi, 'GET', `${foreignOrganizationPath}/files/${reserved.id}/content`,
    'foreign_organization_file_rejected', currentToken, null, 404);

  const searchPath = `${filesPath}/search?q=${searchTerm}`;
  request(secondApi, 'GET', `${foreignOrganizationPath}/files/search?q=${searchTerm}`,
    'foreign_organization_search_rejected', currentToken, null, 404);
  const started = Date.now();
  let visible = false;
  do {
    const results = json(request(secondApi, 'GET', searchPath, 'file_search_poll', currentToken));
    requireCheck(Array.isArray(results.items) && results.items.every(file => file.organization_id === reserved.organization_id),
      'search results remain within their authorized organization');
    visible = results.items.some(file => file.id === reserved.id);
    if (!visible) sleep(0.25);
  } while (!visible && Date.now() - started < 10000);
  const visibilityMilliseconds = Date.now() - started;
  searchVisibility.add(visibilityMilliseconds);
  requireCheck(visible && visibilityMilliseconds < 10000,
    'worker makes the filename searchable within the bounded wait');

  request(firstApi, 'DELETE', filePath, 'file_delete', currentToken, null, 204);
  request(secondApi, 'GET', contentPath, 'deleted_file_rejected', currentToken, null, 404);
  const afterDelete = json(request(secondApi, 'GET', searchPath, 'file_search_after_delete', currentToken));
  requireCheck(afterDelete.items.every(file => file.id !== reserved.id),
    'deleted file disappears immediately even before index reconciliation');
}

function verifyExternalApi(firstApi, secondApi, account) {
  const primary = request(firstApi, 'GET', '/api/v1/external/organization',
    'external_organization_primary', account.api_keys[0]);
  const rotated = request(secondApi, 'GET', '/api/v1/external/organization',
    'external_organization_rotated', account.api_keys[1]);
  requireCheck(json(primary).id === account.organization_ids[0]
    && json(rotated).id === account.organization_ids[0],
  'both API keys resolve only their owning organization');
  requireCheck(responseHeader(primary, 'X-Quota-Limit') === '1000000'
    && responseHeader(rotated, 'X-RateLimit-Limit') === '100000',
  'external responses expose operator allocation');
  const previousRemaining = Number(responseHeader(primary, 'X-Quota-Remaining'));
  const nextRemaining = Number(responseHeader(rotated, 'X-Quota-Remaining'));
  const monthChanged = responseHeader(primary, 'X-Quota-Reset') !== responseHeader(rotated, 'X-Quota-Reset');
  requireCheck(nextRemaining === (monthChanged ? 999999 : previousRemaining - 1),
    'rotated keys across API instances share one monthly counter');
}

function runWorkflow() {
  const account = accounts[__VU - 1];
  requireCheck(!!account, 'each virtual user has a dedicated account fixture');
  if (currentToken === undefined) {
    currentToken = account.token;
    sleep((__VU - 1) * 0.2);
  }
  const started = Date.now();
  const firstApi = apiUrls[(__VU + __ITER) % 2];
  const secondApi = apiUrls[(__VU + __ITER + 1) % 2];
  const organizationPath = `/api/v1/organizations/${account.organization_ids[0]}`;
  const foreignAccount = accounts[__VU % accounts.length];
  const foreignOrganizationPath = `/api/v1/organizations/${foreignAccount.organization_ids[0]}`;
  request(firstApi, 'GET', '/health/ready', 'readiness', currentToken, null, 204);
  verifyOrganizationPagination(firstApi, secondApi, account);
  renameOrganization(firstApi, secondApi, organizationPath);
  request(secondApi, 'GET', foreignOrganizationPath, 'foreign_organization_rejected', currentToken, null, 404);
  verifySessions(firstApi, secondApi, account);
  verifyExternalApi(firstApi, secondApi, account);
  transferOriginal(firstApi, secondApi, organizationPath, foreignOrganizationPath);
  completedWorkflows.add(1, { virtual_user: String(__VU) });
  // Pace reservations below the ordinary per-account limit without weakening it.
  sleep(Math.max(0, 5 - (Date.now() - started) / 1000));
}

export default function () {
  workflowFailures.add(0);
  try {
    runWorkflow();
  } catch (error) {
    workflowFailures.add(1);
    throw error;
  }
}
