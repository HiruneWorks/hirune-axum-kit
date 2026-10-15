import http from 'k6/http';
import { check, fail } from 'k6';

export function requireCheck(condition, name) {
  if (!check(condition, { [name]: value => value })) {
    fail(`Mixed workload check failed: ${name}`);
  }
}

export function request(baseUrl, method, path, name, token, body = null, expectedStatus = 200, extraHeaders = {}) {
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

export function json(response) {
  try {
    return response.json();
  } catch (_) {
    requireCheck(false, 'response contains valid JSON');
  }
}

export function responseHeader(response, name) {
  const key = Object.keys(response.headers).find(key => key.toLowerCase() === name.toLowerCase());
  return response.headers[key];
}

export function jsonRequest(baseUrl, method, path, name, token, body, expectedStatus = 200) {
  return request(baseUrl, method, path, name, token, JSON.stringify(body), expectedStatus, {
    'Content-Type': 'application/json',
  });
}
