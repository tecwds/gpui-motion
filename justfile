# wb-gpui 项目管理 (just)
# 用法: just --list 查看全部任务

# Windows 上使用 PowerShell 作为 shell（默认 sh 在 Windows 不可用）
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# 默认：列出可用任务
default:
    @just --list

# === 开发循环 ===

# 运行主 demo（gallery：组件动画展示）
run:
    cargo run --example gallery

# 运行指定 example: just example codex
example name:
    cargo run --example {{name}}

# 监听变更自动重运行 demo（需先: cargo install cargo-watch）
watch:
    cargo watch -x "run --example gallery"

# === 质量门禁 ===

# 类型检查
check:
    cargo check --all-targets

# 运行测试
test:
    cargo test

# 格式化代码
fmt:
    cargo fmt

# 检查格式（不修改）
fmt-check:
    cargo fmt --check

# Clippy 检查
lint:
    cargo clippy --all-targets -- -D warnings

# 一键本地 CI（格式检查 + clippy + 测试）
ci: fmt-check lint test
    @echo "CI passed"

# === 构建 ===

# Release 构建
build:
    cargo build --release

# 清理构建产物
clean:
    cargo clean

# 生成 API 文档（含私有条目；门禁：零警告）
doc:
    cargo doc --no-deps --document-private-items
