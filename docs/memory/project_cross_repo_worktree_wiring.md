---
name: project_cross_repo_worktree_wiring
description: ash_xr の worktree は symlink 経由で vrm1 本体 checkout を参照する — vrm1 の branch 切替は全 ash_xr checkout に即波及し、worktree 隔離は効かない
metadata:
  node_type: memory
  type: project
---

`bevy_ash_xr/.claude/worktrees/bevy_vrm1` は `/home/sistr/works/bevy/bevy_vrm1` (vrm1 本体 checkout) への **symlink** (2026-07-11 確認、2025-05-30 作成)。ash_xr の worktree (`.claude/worktrees/*`) の `path = "../bevy_vrm1"` 依存はこの symlink 経由で本体 checkout に解決されるため、ash_xr 本体 main も全 worktree も**同一の vrm1 ディレクトリ (= その時の checkout branch) を参照する**。

**Why:** CLAUDE.md § 開発体制の「連動 branch は worktree 兄弟配置で検証」は物理的に隔離された vrm1 コピーを前提に読めるが、実配線は単一 vrm1 を全員で共有している。vrm1 で branch を checkout すると、ash_xr 側で進行中の無関係な worktree 作業のビルド対象も即座に変わる (mtime 変化で再ビルドも走る)。

**How to apply:** ash_xr 連動 branch の検証は「vrm1 本体 checkout の branch を切り替える」ことで行う (worktree ペアを作っても symlink には効かない)。vrm1 の branch 切替前に、ash_xr 側で進行中の作業 (別 worktree 含む) への影響を一言確認する。逆に検証後は vrm1 を main に戻し忘れないこと。
