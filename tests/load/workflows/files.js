import { sleep } from 'k6';
import { Trend } from 'k6/metrics';
import { json, jsonRequest, request, requireCheck } from '../helpers.js';

const searchVisibility = new Trend('search_visibility_ms', true);

export function transferOriginal(firstApi, secondApi, organizationPath, foreignOrganizationPath, token) {
  const payload = `untrusted-original-vu-${__VU}-iteration-${__ITER}\0`.repeat(512);
  const searchTerm = `vu${__VU}iteration${__ITER}original`;
  const filesPath = `${organizationPath}/files`;
  const reserved = json(jsonRequest(firstApi, 'POST', filesPath, 'file_reserve', token,
    { filename: `${searchTerm}.bin`, media_type: 'application/octet-stream', byte_size: payload.length }, 201));
  requireCheck(reserved.state === 'uploading' && typeof reserved.id === 'string',
    'file reservation starts in uploading state');
  const filePath = `${filesPath}/${reserved.id}`;
  const contentPath = `${filePath}/content`;
  const uploaded = json(request(firstApi, 'PUT', contentPath, 'file_upload', token,
    payload, 200, { 'Content-Type': 'application/octet-stream' }));
  requireCheck(uploaded.state === 'ready' && uploaded.byte_size === payload.length,
    'upload completes with the declared size');
  const downloaded = request(secondApi, 'GET', contentPath, 'file_download', token);
  requireCheck(downloaded.body === payload, 'download preserves original bytes');
  requireCheck(downloaded.headers['Content-Type'] === 'application/octet-stream'
    && downloaded.headers['X-Content-Type-Options'] === 'nosniff'
    && downloaded.headers['Content-Disposition'].startsWith('attachment;'),
  'original download retains attachment protections');
  const head = request(secondApi, 'HEAD', contentPath, 'file_head', token);
  requireCheck(Number(head.headers['Content-Length']) === payload.length,
    'HEAD exposes the stored original length');
  const ranged = request(secondApi, 'GET', contentPath, 'file_range', token,
    null, 206, { Range: 'bytes=2-9' });
  requireCheck(ranged.body === payload.slice(2, 10)
    && ranged.headers['Content-Range'] === `bytes 2-9/${payload.length}`,
  'range download returns the requested original bytes');
  request(secondApi, 'GET', `${foreignOrganizationPath}/files/${reserved.id}/content`,
    'foreign_organization_file_rejected', token, null, 404);

  const searchPath = `${filesPath}/search?q=${searchTerm}`;
  request(secondApi, 'GET', `${foreignOrganizationPath}/files/search?q=${searchTerm}`,
    'foreign_organization_search_rejected', token, null, 404);
  const started = Date.now();
  let visible = false;
  do {
    const results = json(request(secondApi, 'GET', searchPath, 'file_search_poll', token));
    requireCheck(Array.isArray(results.items) && results.items.every(file => file.organization_id === reserved.organization_id),
      'search results remain within their authorized organization');
    visible = results.items.some(file => file.id === reserved.id);
    if (!visible) sleep(0.25);
  } while (!visible && Date.now() - started < 10000);
  const visibilityMilliseconds = Date.now() - started;
  searchVisibility.add(visibilityMilliseconds);
  requireCheck(visible && visibilityMilliseconds < 10000,
    'worker makes the filename searchable within the bounded wait');

  request(firstApi, 'DELETE', filePath, 'file_delete', token, null, 204);
  request(secondApi, 'GET', contentPath, 'deleted_file_rejected', token, null, 404);
  const afterDelete = json(request(secondApi, 'GET', searchPath, 'file_search_after_delete', token));
  requireCheck(afterDelete.items.every(file => file.id !== reserved.id),
    'deleted file disappears immediately even before index reconciliation');
}
