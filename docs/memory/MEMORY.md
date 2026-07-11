# Memory Index

> memory 実体は repo 内 `docs/memory/`。書式は `.claude/rules/memory-format.md` 参照。harness auto-memory は無効化済

- [project_fork_policy](project_fork_policy.md) - not-elm/bevy_vrm1 の恒久 fork (上流 PR 禁止)、fork 先 SistrScarlet/bevy_vrm1、開発トランクは main
- [project_cross_repo_worktree_wiring](project_cross_repo_worktree_wiring.md) - ash_xr の worktree は symlink で vrm1 本体 checkout を共有 — vrm1 の branch 切替は全 ash_xr checkout に即波及 (隔離なし)
