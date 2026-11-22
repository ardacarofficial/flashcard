/**
 * Calls a public API method through the dev bridge, exactly as the UI does (same dispatch as
 * the desktop IPC). Resolves with the result; rejects with an Error carrying the `ApiError`
 * DTO as `.api` (`e.api.code`, `e.api.details`).
 */
export async function bridgeCall(url, token, method, params = {}) {
  const res = await fetch(`${url}/rpc`, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: JSON.stringify({ method, params }),
  });
  const reply = await res.json().catch(() => null);
  if (reply?.ok === true) return reply.result;
  const api = reply?.error ?? { code: 'INTERNAL', message: `HTTP ${res.status}` };
  const err = new Error(`${method}: ${api.code}: ${api.message}`);
  err.api = api;
  throw err;
}

/** The built-in Classic note type (`fc_core::notetype::builtin::CLASSIC_ID`). */
export const CLASSIC_NOTETYPE_ID = '01900000-0000-7000-8000-000000000001';
