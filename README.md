# SSH Remote extensions

The extensions offered in the signed registry at https://sshremote.dev/registry/v1/ for
[SSH Remote](https://github.com/thinkbigx/sshremote). Each folder in `packs/` is one extension
with its own `extension.json` (id, version, permissions, what it contributes) and is versioned on
its own: raising its `version` and deploying publishes an update for it alone.

| Pack | Kind |
|---|---|
| `text-tools` | Script (a Rust crate built to WebAssembly) |
| `nature-scenes`, `ocean`, `tech-styles` | Style packs (Lottie backgrounds) |
| `web-apps` | Tools: web apps in a pane |
| `programs` | Programs in panes, opened three ways: Search (as the Start menu), Command (as Run), File (API 4) |

Screens, files, remote access, the web gateway, the MCP server and SSH tunnels are parts of the
app itself, not extensions.

## Building

    python build.py OUT

writes `OUT/registry/v1/ID-VERSION.sshx` for each pack and `OUT/registry/v1/index.json` listing
them with their SHA-256. A `make.py` beside a pack runs first (building what it needs). The index
is signed and uploaded by the app repository's `scripts/deploy.sh`, which looks for this
repository beside it (`../sshremote-extensions`, or `$EXTENSIONS`).

Writing your own extension: see `docs/extensions.md` in the app repository, or run
`sshremote ext new my-tool`.
