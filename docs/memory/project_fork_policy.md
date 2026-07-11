---
name: project_fork_policy
description: このリポジトリは not-elm/bevy_vrm1 の恒久 fork。上流 PR 禁止、bevy_ash_xr 専用に運用
metadata:
  node_type: memory
  type: project
---

このリポジトリは not-elm/bevy_vrm1 の**恒久 fork** として運用する (2026-07-04 user 決定)。fork 先: https://github.com/SistrScarlet/bevy_vrm1 (public、2026-07-11 作成)。

- **上流 PR はしない** (AI slop 回避)。perf 改善なども upstream に送らない
- 上流は v0.7.1 (2026-04-21) で停滞。上流待ちせず fork を進化させる
- **開発トランクは `main`** (2026-07-11 に `ash-xr-integration` を fast-forward 統合して廃止)。upstream v0.7.1 の真上に独自 commits が線形に乗る構成。upstream 追従は remote ref `upstream/main` から merge / cherry-pick
- remote 構成: `origin` = SistrScarlet/bevy_vrm1 (fork)、`upstream` = not-elm/bevy_vrm1 (参照専用)
- path 依存のままで良い (git 依存への変更は不要、2026-07-11 user 確認)。bevy_ash_xr が `path = "../bevy_vrm1"` で参照するため **常に `main` をチェックアウトしておく**
- Bevy 0.19 upgrade 時は自前 port + wgpu/mtoon 部 drop + workspace vendor 化を再判断
- bevy_ash_xr が使うのは「VRM asset loader + humanoid rig ECS」のみ。MtoonMaterialPlugin は vrm_bridge で除外済み (描画は AshMtoonExtension)
- 対応する bevy_ash_xr 側 memory: `../bevy_ash_xr/docs/memory/reference_bevy_vrm1.md`

**Why:** 上流停滞 + bevy_ash_xr の下流アーキテクチャ要求 (sub-plugin 公開等) を満たすには独自進化が必要なため。

**How to apply:** 変更提案時に「upstream に還元できるか」を考慮しない。公開リポジトリなので push 前に内容レビューは行うが、upstream との互換維持は制約にしない。
