import http from 'k6/http';
import { check, sleep } from 'k6';

const extended = __ENV.K6_PROFILE === 'extended';
export const options = {
  vus: extended ? 20 : 2,
  duration: extended ? '2m' : '10s',
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(95)<2000'],
    checks: ['rate==1'],
  },
};

export default function () {
  const headers = { Authorization: `Bearer ${__ENV.AUTH_TOKEN}` };
  const readiness = http.get(`${__ENV.BASE_URL}/health/ready`);
  check(readiness, { ready: response => response.status === 204 });
  const account = http.get(`${__ENV.BASE_URL}/api/v1/account`, { headers });
  check(account, { authenticated: response => response.status === 200 && !!response.json('id') });
  const organizations = http.get(`${__ENV.BASE_URL}/api/v1/organizations?limit=25`, { headers });
  check(organizations, { scoped_page: response => response.status === 200 && Array.isArray(response.json('items')) });
  sleep(0.1);
}
