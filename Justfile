
# `check` is the definition of green. CI runs it and nothing else, so a gate
# added here is picked up there without a second edit.

set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Everything a change owes before it is handed over.
check: rust-fmt-check rust-clippy rust-test deny wasm-check wasm-test wasm-smoke lab-build

rust-fmt-check:
    cargo fmt --all -- --check

rust-clippy:
    cargo clippy --workspace --all-targets -- -D warnings

rust-test:
    cargo test --workspace

deny:
    cargo deny check

# The browser's target still builds and lints clean, tests and examples included.
wasm-check:
    cargo clippy --workspace --all-targets --target wasm32-unknown-unknown -- -D warnings

# The core's tests give the same results as natively when run as WebAssembly
# (wasm32-wasip1) under node:wasi, which .cargo/config.toml sets as the runner.
wasm-test:
    cargo test -p enzan-core --target wasm32-wasip1 --no-fail-fast -- --nocapture

# The wasm-bindgen version the build uses. Cargo.lock is the one place that
# says it; the wasm-bindgen-cli on the PATH has to be the same version.
wasm-bindgen-version:
    @cargo pkgid -p wasm-bindgen | sed 's/.*@//'

# Refuses a wasm-bindgen-cli whose version is not the one in Cargo.lock, or
# none, and says what to install.
wasm-bindgen-check:
    #!/usr/bin/env bash
    set -euo pipefail
    want="$({{just_executable()}} wasm-bindgen-version)"
    have="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
    if [ "$have" != "$want" ]; then
        echo "wasm-bindgen-check: need wasm-bindgen-cli $want (the version in Cargo.lock), found ${have:-none}" >&2
        echo "  cargo install --locked wasm-bindgen-cli --version $want" >&2
        exit 1
    fi
    echo "wasm-bindgen-check: $have ok"

# Builds enzan-wasm and generates its package for the web into lab/src/pkg.
wasm-pkg: wasm-bindgen-check
    cargo build -p enzan-wasm --target wasm32-unknown-unknown --release
    wasm-bindgen --target web --out-dir lab/src/pkg target/wasm32-unknown-unknown/release/enzan_wasm.wasm

# Calls the generated package from node: the boundary the native tests cannot reach.
wasm-smoke: wasm-pkg
    node lab/scripts/smoke.mjs

# Installs the lab's packages from its lock file (network) and builds it: the
# page is type-checked against the generated package.
lab-build: wasm-pkg
    #!/usr/bin/env bash
    set -euo pipefail
    command -v npm >/dev/null || { echo "lab-build: npm not found; install node 22 (it brings npm)" >&2; exit 1; }
    npm ci --prefix lab
    npm run build --prefix lab

# Serves the lab for a browser. Not part of `check`.
lab-dev: wasm-pkg
    #!/usr/bin/env bash
    set -euo pipefail
    command -v npm >/dev/null || { echo "lab-dev: npm not found; install node 22 (it brings npm)" >&2; exit 1; }
    [ -d lab/node_modules ] || npm ci --prefix lab
    npm run dev --prefix lab

# One worktree per issue, cut from a freshly fetched origin/main.
worktree-new type slug:
    git fetch origin
    git worktree add .worktrees/{{slug}} -b {{type}}/{{slug}} origin/main
    cd .worktrees/{{slug}} && just branch-check

# Refuses main, and a branch that does not contain origin/main.
branch-check:
    #!/usr/bin/env bash
    set -euo pipefail
    branch="$(git rev-parse --abbrev-ref HEAD)"
    if [ "$branch" = "main" ]; then
        echo "branch-check: on main; cut a branch with 'just worktree-new'" >&2
        exit 1
    fi
    if git rev-parse --verify --quiet origin/main >/dev/null; then
        git merge-base --is-ancestor origin/main HEAD || {
            echo "branch-check: $branch does not contain origin/main; rebase or recut" >&2
            exit 1
        }
    fi
    echo "branch-check: $branch ok"

# Commit messages on this branch carry no AI attribution (CONTRIBUTING.md).
commit-msg-check:
    #!/usr/bin/env bash
    set -euo pipefail
    range="origin/main..HEAD"
    git rev-parse --verify --quiet origin/main >/dev/null || range="HEAD"
    if git log --format=%B "$range" | grep -inE '^(co-authored-by|signed-off-by):|generated with'; then
        echo "commit-msg-check: attribution lines found in $range" >&2
        exit 1
    fi
    echo "commit-msg-check: $range ok"

# The gate before a hand-over: run it after the last commit, over the tree
# that is actually handed over.
pre-push: branch-check commit-msg-check check

