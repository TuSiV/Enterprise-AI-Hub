# Windows 支持

本目录及以下文件是 Windows 平台的**增量**支持，不影响 macOS / Linux 现有链路：

| 文件 | 作用 |
|---|---|
| `apps/desktop/src-tauri/tauri.windows.conf.json` | Tauri 2 平台专属配置，仅在 Windows 构建时自动合并到主配置，打包目标为 NSIS 安装器 + MSI；macOS 仍使用主配置中的 `dmg`/`app` |
| `scripts/windows/smoke.ps1` | 端到端冒烟脚本的 Windows 版（等价于 `scripts/smoke.sh`），PowerShell 5.1+ 可直接运行 |
| `.github/workflows/ci-windows.yml` | 独立的 Windows CI 工作流，与 `ci.yml`（Linux）互相独立 |

## Windows 下开发 / 冒烟

```powershell
# 1. 编译（调试版即可）
cargo build

# 2. 构建 Web 控制台（可选，server 启动时自动探测 web/dist）
cd web; npm install; npm run build; cd ..

# 3. 冒烟（自动找 target\debug\*.exe，结束自动清理进程与临时目录）
powershell -ExecutionPolicy Bypass -File scripts\windows\smoke.ps1
```

## Windows 桌面安装包

在 Windows 机器上执行（Tauri 不支持交叉打包）：

```powershell
cargo install tauri-cli --locked
cargo tauri build
# 产物：target\release\bundle\nsis\*.exe 与 bundle\msi\*.msi
```
