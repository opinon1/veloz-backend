"""Programmatic Etomin sandbox 3DS completion.

Etomin's 3DS flow normally needs a real browser to load the DDC page,
collect device info via JavaScript, post it to `/complete`, and follow
the sandbox callback that flips the transaction to APPROVED. Sandbox
accepts an empty `deviceInfo` body, so we can do the same dance over
plain HTTP from a test runner.

Used to simulate "user finished 3DS" inside integ tests so they can
assert the transition PENDING → APPROVED + grant fulfillment without
booting a headless browser in CI.
"""
from __future__ import annotations

import re

import httpx


def complete_3ds_in_sandbox(redirect_to: str, timeout_s: int = 30) -> str:
    """Drive Etomin's sandbox 3DS to completion.

    Steps:
      1. POST `<redirect_to>/complete` with empty deviceInfo. Sandbox
         responds with HTML containing the next-hop sandbox callback URL
         in a JS `href:` literal.
      2. Extract that URL.
      3. GET it (follows redirects). Etomin updates server-side state
         here — the response body / final URL doesn't matter.

    Returns the sandbox callback URL that was visited (mostly for debug).
    Raises if the response shape doesn't match what we expect.
    """
    if not redirect_to:
        raise ValueError("redirect_to is required")
    if "/3ds/ddc/" not in redirect_to:
        raise ValueError(f"unexpected redirect_to shape: {redirect_to}")

    complete_url = redirect_to.rstrip("/") + "/complete"
    with httpx.Client(timeout=timeout_s, follow_redirects=False) as cl:
        r = cl.post(
            complete_url,
            data={"deviceInfo": "{}"},
            headers={"Content-Type": "application/x-www-form-urlencoded"},
        )
        r.raise_for_status()
        html = r.text

        m = re.search(
            r"""href:\s*['"]([^'"]+/sandbox/3ds/callback[^'"]+)['"]""",
            html,
        )
        if not m:
            raise RuntimeError(
                "no sandbox callback URL in /complete response — "
                "Etomin sandbox may have changed shape"
            )
        callback_url = m.group(1)

    with httpx.Client(timeout=timeout_s, follow_redirects=True) as cl:
        cl.get(callback_url)

    return callback_url
