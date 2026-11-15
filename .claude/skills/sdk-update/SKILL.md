---
name: sdk-update
description: Move this repository's pinned cmf-watch-sdk (WP2_APPS_REV in the Makefile) to a newer commit, check every app still builds, and teach AGENTS.md and the skills what the new SDK brought. Use when the owner says a new loader/stub/SDK is out, when an app needs a feature the pinned SDK lacks, or when asked to update the SDK.
---

# Moving the SDK pin

## 1. See what is new

```bash
make sdk
git -C .sdk/wp2-apps fetch --quiet origin
git -C .sdk/wp2-apps log --oneline HEAD..origin/main
git -C .sdk/wp2-apps diff --stat HEAD origin/main -- docs sdk
```

Read the changed parts of `docs/app-guide.md` and the newest `docs/abi-*.md` in the diff, and the `Api` changes in
`sdk/cmf-wp2-app/src/lib.rs`. Note for each new capability: the call or flag, the loader/stub date it needs, and
whether the simulator models it.

## 2. Move the pin

On a branch, set `WP2_APPS_REV` in the Makefile to the new full commit hash, then:

```bash
make sdk && make test && make build && make check
```

Every app must build and pass. A break in an app is fixed in this PR only if it's mechanical (a renamed call);
otherwise stop and tell the owner. Apps do not raise their `version` for an SDK bump: the store publishes only
version changes, so a rebuild alone ships nothing.

## 3. Teach the agents

Update AGENTS.md where the new SDK changes what an agent should say or do: the "What an app can be" table, "Loop"
(simulator coverage), "Apps that talk to a phone" (hosts, firmware dates). Update the `watch-app` skill if the order
of work changes. Write capabilities, the loader date they need, and pointers into the SDK docs; don't copy the docs.

## 4. PR

The SDK bump is a PR of its own (no app changes in it). Body: the commit range, what's new in one line each, and
"all apps build and pass on <new hash>".
