# bevy_vrm1 Makefile
#
# ビルドは必ず make 経由で回すこと (生 cargo を直接叩かない)。

# ── Build lock ───────────────────────────────────────────────────────────────
# ../bevy_ash_xr と共有のビルド mutex。
#
# 【なぜロックするか】ash_xr は path 依存で本 crate を参照し、また worktree は
#   それぞれ別 target/ を持つため、cargo 自身のビルドロックはリポジトリ・worktree
#   間で効かない。重いビルドが 2 本重なると ~15GB の WSL RAM 上限を超える。
#   ash_xr の Makefile と同一の固定 /tmp パスに flock を張り、両リポジトリ
#   (+ 各 worktree) で同時に走るビルドを 1 本に強制する。単独ビルド時は競合ゼロ =
#   即取得で無害。
#
# 【どのターゲットに付けるか】「ビルドして終了する」ターゲットだけ $(LOCK) で包む。
#   長時間走り続けるプロセス (example 実行等) には付けない — ロックを保持し続けて
#   他方のビルドを止めてしまうため (build phase だけ先に済ませ、run 本体は外で回す)。
#
# 【flock を直実行しない】外側から `flock $(BUILD_LOCK) make check` のように make を
#   包むと、recipe 内の $(LOCK) が外側のロックを永久待ちして自己デッドロックする。
#   make を経由しない生 cargo を回すときだけ flock $(BUILD_LOCK) を内蔵させる。
BUILD_LOCK := /tmp/bevy_ash_xr-build.lock
LOCK := flock $(BUILD_LOCK)

# ── Targets ──────────────────────────────────────────────────────────────────
# ARGS で追加引数を渡す (例: make test ARGS="--features log spring_bone")

.PHONY: check clippy test build

check:
	$(LOCK) cargo check $(ARGS)

clippy:
	$(LOCK) cargo clippy --all-targets $(ARGS)

test:
	$(LOCK) cargo test $(ARGS)

build:
	$(LOCK) cargo build $(ARGS)
