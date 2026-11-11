# SSH Remote extensions

The extensions offered in the signed registry at https://sshremote.dev/registry/v1/ for
[SSH Remote](https://github.com/thinkbigx/sshremote). Each folder in `packs/` is one extension
with its own `extension.json` (id, version, permissions, what it contributes) and is versioned on
its own: raising its `version` and deploying publishes an update for it alone.

| Pack | Kind |
|---|---|
| `ssh-tunnels`, `text-tools` | Programs (Rust crates in their folders) |
| `nature-scenes`, `ocean`, `tech-styles` | Style packs (Lottie backgrounds) |
| `remote-desktop`, `web-gateway`, `mcp-server` | SSH Remote itself built with only that part; `make.py` takes the programs from `$HELPERS` (see `helper_pack.py`) |

## Building

    python build.py OUT

writes `OUT/registry/v1/ID-VERSION.sshx` for each pack and `OUT/registry/v1/index.json` listing
them with their SHA-256. A `make.py` beside a pack runs first (building its program). The index
is signed and uploaded by the app repository's `scripts/deploy.sh`, which looks for this
repository beside it (`../sshremote-extensions`, or `$EXTENSIONS`).

Writing your own extension: see `docs/extensions.md` in the app repository, or run
`sshremote ext new my-tool`.
