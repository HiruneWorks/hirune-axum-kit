"""Use real production HTTPS and SMTP endpoints without external services."""

import base64
import hashlib
import hmac
import json
import pathlib
import secrets
import smtplib
import ssl
import struct
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from database import sql


ORIGIN = "https://api:8443"
STATE_FILE = pathlib.Path("/state/account.json")
TLS_CONTEXT = ssl.create_default_context(cafile="/certificates/ca.crt")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, file, code, message, headers, new_url):
        return None


def request(method, path, body=None, cookie=None, origin=ORIGIN, expected=200, headers=None):
    request_headers = dict(headers or {})
    if origin is not None:
        request_headers["Origin"] = origin
    if cookie:
        request_headers["Cookie"] = cookie
    if body is not None:
        request_headers["Content-Type"] = "application/json"
    opener = urllib.request.build_opener(
        urllib.request.ProxyHandler({}),
        urllib.request.HTTPSHandler(context=TLS_CONTEXT),
        NoRedirect(),
    )
    url = path if path.startswith("http") else ORIGIN + path
    call = urllib.request.Request(
        url, data=json.dumps(body).encode() if body is not None else None,
        headers=request_headers, method=method,
    )
    try:
        response = opener.open(call, timeout=15)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        assert response.status == expected, (
            f"{method} {urllib.parse.urlsplit(url).path}: expected {expected}, got {response.status}"
        )
        data = response.read()
        return (json.loads(data) if data else None), response.headers


def wait_ready():
    deadline = time.monotonic() + 60
    last_error = None
    while time.monotonic() < deadline:
        try:
            _, headers = request("GET", "/health/ready", expected=204)
            assert headers["Strict-Transport-Security"] == "max-age=31536000", "Production HSTS header missing or unexpected"
            request("GET", "http://mailpit:8025/api/v1/messages", origin=None)
            return
        except (urllib.error.URLError, TimeoutError, ConnectionError, AssertionError) as error:
            last_error = error
            time.sleep(0.2)
    raise RuntimeError(f"Production fixture readiness failed: {last_error}")


def verify_tls():
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        opener.open(ORIGIN + "/health/ready", timeout=10)
    except urllib.error.URLError as error:
        assert isinstance(error.reason, ssl.SSLCertVerificationError)
    else:
        raise AssertionError("Private HTTPS certificate was accepted without its CA")
    with smtplib.SMTP("mailpit", 1025, timeout=10) as smtp:
        smtp.ehlo()
        code, _ = smtp.mail("probe@example.test")
        assert code == 530, "Mailpit accepted plaintext SMTP MAIL"
        smtp.starttls(context=TLS_CONTEXT)
        smtp.ehlo()
        code, _ = smtp.mail("probe@example.test")
        assert code == 250, "STARTTLS SMTP was not accepted"
        smtp.rset()
    with smtplib.SMTP("mailpit-unmatched", 1025, timeout=10) as smtp:
        smtp.ehlo()
        try:
            smtp.starttls(context=TLS_CONTEXT)
        except ssl.SSLCertVerificationError as error:
            assert "hostname mismatch" in error.verify_message.lower()
        else:
            raise AssertionError("SMTP certificate matched the deliberately incorrect hostname")
    print("HTTPS private CA and hostname verified; SMTP rejects plaintext and requires STARTTLS.")


def verification_token(email):
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        result, _ = request("GET", "http://mailpit:8025/api/v1/search?" +
                            urllib.parse.urlencode({"query": "to:" + email}), origin=None)
        for message in result.get("messages", []):
            if "Verify your email" not in message["Subject"]:
                continue
            detail, _ = request("GET", "http://mailpit:8025/api/v1/message/" + message["ID"], origin=None)
            token = detail["Text"].split("#token=", 1)[1].split()[0]
            assert len(token) == 43
            assert "<html" in detail["HTML"]
            return token
        time.sleep(0.2)
    raise RuntimeError("Production worker did not deliver verification mail over STARTTLS")


def session_cookie(headers):
    cookie = headers.get("Set-Cookie", "")
    for attribute in ["__Host-session=", "Secure", "HttpOnly", "SameSite=Lax", "Path=/"]:
        assert attribute in cookie, "Production session cookie attribute missing"
    assert "domain=" not in cookie.lower()
    return cookie.split(";", 1)[0]


def totp_code(secret, step):
    key = base64.b32decode(secret + "=" * (-len(secret) % 8))
    # SHA-1 is mandated by this enrolled TOTP protocol, not an application hash choice.
    digest = hmac.new(key, struct.pack(">Q", step), hashlib.sha1).digest()
    offset = digest[-1] & 15
    number = struct.unpack(">I", digest[offset:offset + 4])[0] & 0x7FFFFFFF
    return f"{number % 1_000_000:06d}"


def prepare_account():
    wait_ready()
    email = "production-" + secrets.token_hex(8) + "@example.test"
    password = "production acceptance " + secrets.token_urlsafe(24)
    request("POST", "/api/v1/auth/signup", {"email": email, "password": password}, expected=202)
    request("POST", "/api/v1/auth/verification/confirm",
            {"token": verification_token(email)}, expected=204)
    credentials = {"email": email, "password": password}
    request("POST", "/api/v1/auth/login", credentials, origin="https://untrusted.example", expected=403)
    request("POST", "/verification/unforwarded-login", credentials, expected=400)
    direct, _ = request("POST", "http://api:3000/api/v1/auth/login",
                        {**credentials, "transport": "bearer"},
                        headers={"X-Forwarded-For": "spoofed, invalid"})
    assert direct["status"] == "authenticated"
    result, headers = request("POST", "/api/v1/auth/login", credentials,
                              headers={"X-Forwarded-For": "spoofed, invalid"})
    assert result["status"] == "authenticated"
    cookie = session_cookie(headers)
    account, _ = request("GET", "/api/v1/account", cookie=cookie)
    assert account["verified"] is True
    request("POST", "/api/v1/organizations", {"name": "Rejected origin"},
            cookie=cookie, origin=None, expected=403)
    enrollment, _ = request("POST", "/api/v1/account/totp", cookie=cookie)
    secret = urllib.parse.parse_qs(urllib.parse.urlsplit(enrollment["authenticator_url"]).query)["secret"][0]
    step = int(time.time()) // 30
    recovery, _ = request("POST", "/api/v1/account/totp/confirm",
                          {"challenge": enrollment["challenge"], "code": totp_code(secret, step)},
                          cookie=cookie)
    assert recovery["recovery_codes"]
    request("GET", "/api/v1/account", cookie=cookie, expected=401)
    STATE_FILE.write_text(json.dumps({**credentials, "secret": secret, "step": step}), encoding="utf-8")
    STATE_FILE.chmod(0o600)
    print("Production signup, STARTTLS delivery, host cookie, origin rejection, proxy header replacement, and MFA enrollment passed.")


def verify_rejected_smtp_certificate():
    wait_ready()
    verify_tls()
    failed_attempts = int(sql("runtime", "SELECT count(*) FROM job_attempts WHERE error_code='smtp_send_failed';"))
    request("POST", "/api/v1/auth/signup", {
        "email": "tls-rejection-" + secrets.token_hex(8) + "@example.test",
        "password": "untrusted certificate fixture " + secrets.token_urlsafe(24),
    }, expected=202)
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        attempts = int(sql("runtime", "SELECT count(*) FROM job_attempts WHERE error_code='smtp_send_failed';"))
        if attempts > failed_attempts:
            messages, _ = request("GET", "http://mailpit:8025/api/v1/messages", origin=None)
            assert messages["total"] == 0, "Rust SMTP accepted a certificate with invalid trust or hostname"
            print("Rust worker rejected the SMTP certificate; failed attempt persisted and no mail accepted.")
            return
        time.sleep(0.2)
    raise RuntimeError("Expected Rust SMTP certificate rejection was not observed")


def verify_restored_account():
    wait_ready()
    state = json.loads(STATE_FILE.read_text(encoding="utf-8"))
    while int(time.time()) // 30 <= state["step"]:
        time.sleep(0.2)
    result, _ = request("POST", "/api/v1/auth/login",
                        {"email": state["email"], "password": state["password"]})
    assert result["status"] == "mfa_required"
    _, headers = request("POST", "/api/v1/auth/mfa", {
        "challenge": result["challenge"],
        "proof": {"kind": "totp", "code": totp_code(state["secret"], int(time.time()) // 30)},
    })
    cookie = session_cookie(headers)
    account, _ = request("GET", "/api/v1/account", cookie=cookie)
    assert account["verified"] is True and account["mfa_enabled"] is True
    request("DELETE", "/api/v1/sessions", cookie=cookie, expected=204)
    request("GET", "/api/v1/account", cookie=cookie, expected=401)
    STATE_FILE.unlink()
    print("Restored password and encrypted TOTP state work with retained keys; session revocation passed.")


{"prepare": prepare_account, "restored": verify_restored_account,
 "rejected-certificate": verify_rejected_smtp_certificate}[sys.argv[1]]()
