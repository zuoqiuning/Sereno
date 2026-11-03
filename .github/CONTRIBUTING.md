# 贡献指南 / Contributing

欢迎提交 Issue 和 Pull Request。

## 报告问题

- 请使用 Bug 报告模板，附上版本号、系统环境与复现步骤。
- 日志可在「实例 → 日志」里查看并导出诊断包；粘贴前请确认里面没有账号令牌等敏感信息。

## 提交代码

1. 改动前先开 Issue 说明要解决的问题，避免白做。
2. 前端：Vue 3 + TypeScript，组件用 `<script setup>`；后端：Rust，跟着 rustfmt 默认风格走。
3. 提交前至少跑一遍：

```bash
pnpm typecheck && pnpm lint && pnpm build:web
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

4. Commit Message 使用约定式提交前缀：`feat:` / `fix:` / `docs:` / `refactor:` / `chore:`。
5. PR 请描述「改了什么、为什么改、怎么验证」，附截图更好。

---

Issues and PRs are welcome. Keep commit messages in the Conventional Commits format
(`feat:` / `fix:` / `docs:` / `refactor:` / `chore:`) and run the checks above before opening a PR.
