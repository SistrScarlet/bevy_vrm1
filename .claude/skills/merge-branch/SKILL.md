---
name: merge-branch
description: feature branch → main マージ + 後処理一式 (work-map SSOT 更新 + worktree 掃除 + docs commit)。**main branch でのみ実行可** + **`/code-review` 実施済み + user マージ承認が gate**。trigger = user「マージして」「merge」「main に統合」等。bevy_ash_xr 連動 branch の場合は cross-repo 手順 (vrm1 → ash_xr 順) を含む。
---

# Merge Branch — feature branch → main マージ + 後処理 (bevy_vrm1)

feature branch を main にマージし、進行 SSOT (`docs/plan/work-map.md`) を更新する。

## 前提確認 (gate) — 1 つでも欠けたら即 EXIT

1. **現在 `main` branch にいること** (`git branch --show-current`)。worktree 上では実行不可 → user に main で再実行を依頼して終了
2. **`/code-review` が branch 完了時に実施済みで finding 解消済みであること**。未実施なら「先に `/code-review` を実行してください」と伝えて終了 — **CC が代わりにマージを進めない**
3. **user がマージを明示的に承認していること** (「マージして」等)。過去の別作業への承認は流用しない

## 手順

### Step 1: マージ

```bash
git log --oneline main..{branch} | wc -l   # commit 数
git merge {branch} --no-ff -m "merge: {branch} → main ({N} commits, {概要})"
```

- 1 commit のみの branch は `--ff-only` でも可 (main が先行していたら branch 側を rebase してから)

### Step 2: コンフリクト解決 (発生時)

1. `git diff --name-only --diff-filter=U` で衝突ファイル列挙 → 各ファイルの衝突箇所を Read
2. 解決方針: main 側の API 変更は main 採用 / branch の新機能追加は branch 採用 / import・module 宣言等の両方追加は両取り / フォーマット差のみは main 採用
3. マーカー消去確認: `grep -rn '<<<<<<<\|>>>>>>>' --include='*.rs' --include='*.md' --include='*.toml' .`
4. `cargo check` + `cargo clippy` + `cargo test --lib` で確認

### Step 3: bevy_ash_xr 連動確認 (連動 branch の場合のみ)

vrm1 の API を変更した branch では、マージ後に **ash_xr main (旧コード) が新 vrm1 main でビルドできること**を確認する:

```bash
cd ../bevy_ash_xr && cargo check -p bevy_ash_openxr -p bevy_ash_event_client
```

- 通らない場合は vrm1 側の変更が additive でない (CLAUDE.md § 開発体制違反)。原因を特定して user に報告し、対応を仰ぐ
- ash_xr 側の対応 branch のマージは ash_xr リポジトリ側で別途実施 (そちらも `/code-review` + user 承認が前提)

### Step 4: worktree 掃除 (該当時)

```bash
git worktree list                  # 残存確認
git worktree remove {path}         # マージした branch の worktree のみ
git branch -d {branch}
```

- 他の worktree / 未マージ branch は触らない。残存を user に報告

### Step 5: SSOT 更新 + docs commit

`docs/plan/work-map.md` を更新:

1. **Active Branch** から該当 branch のサブセクションを削除
2. **完了済 Branch** に 1 行追加 (branch 名 + merge commit hash + commit 数 + 概要)
3. **残作業ブロック** の該当行を「済」に更新 or 削除、branch 作業中に見つかった新規残作業があれば行追加

```bash
git add docs/plan/work-map.md
git commit -m "docs: {branch} 完了反映 — work-map 更新"
```

## チェックリスト (完了前確認)

- [ ] gate 3 点 (main / code-review 済 / user 承認) 通過
- [ ] コンフリクトマーカー残存なし + check / clippy / test 通過
- [ ] (連動 branch) ash_xr main のビルド確認済み
- [ ] worktree + branch 削除済み (マージ対象のみ)
- [ ] work-map の Active / 完了済 / 残作業 整合
- [ ] docs commit 済み
