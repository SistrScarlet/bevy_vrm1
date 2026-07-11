---
paths:
  - "docs/memory/**"
---

# memory 書式規律

harness auto-memory は無効化済 (`autoMemoryEnabled: false`)。 memory の書込・更新は CC が本規律 + CLAUDE.md § コンテキストファイル追加規律に従い手動実施する。

## file 単位

- **1 file = 1 fact**。 file 名 = `{type}_{slug}.md` (`feedback_*` / `project_*` / `user_*` / `reference_*` / `hub_*`)、 slug は snake_case
- 保存前に既存 file の重複チェック — 重複は新規作成でなく既存 file を更新。 誤りと判明した memory は削除 (archive でなく) 可
- repo が既に記録していること (コード構造 / git 履歴 / CLAUDE.md / 過去 fix) や当該 session 限りの事柄は書かない。 一般原則は書かない — プロジェクト固有の適用判断のみ

## frontmatter

```markdown
---
name: <file 名から .md を除いた slug>
description: <1 行要約 — recall 時の関連判断に使う>
metadata:
  node_type: memory
  type: user | feedback | project | reference | hub
---
```

## 本文

- `feedback_*` / `project_*` は fact に続けて **Why:** + **How to apply:** 行を付ける
- 関連 memory は `[[name]]` link (解決先 = `docs/memory/{name}.md`、 archive 落ち分は `docs/memory/archive/{name}.md`)
- 日付は相対表現でなく絶対日付 (YYYY-MM-DD)

## index (MEMORY.md)

- 書込後、 `docs/memory/MEMORY.md` に 1 行 pointer (`- [name](file.md) - hook`) を追加。 **index 追加は user 承認必須** (= 「session 開始時に存在を知る必要があるか」 基準)
- MEMORY.md に memory 本文を書かない (index 専用)。 200 行 (or 24.4KB) 以内維持 — 近付いたら `memory-prune` skill
- archive 降格・集約の手順は `memory-prune` skill が SSOT (footer 記録 + hub link 整理含む)
