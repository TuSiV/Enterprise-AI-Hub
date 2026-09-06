# Desktop 自动更新（方案 §23.6）

配置已写入 `tauri.conf.json`（`plugins.updater` + `createUpdaterArtifacts`）。
发布流水线需完成：

1. `cargo tauri signer generate -w ~/.tauri/aihub.key` 生成签名密钥对；
2. 公钥写入 `plugins.updater.pubkey`，私钥注入 CI（`TAURI_SIGNING_PRIVATE_KEY`）；
3. 更新清单（latest.json + 签名产物）发布到 `plugins.updater.endpoints` 指向的更新服务；
4. 应用内通过 `@tauri-apps/plugin-updater` 触发 Check → Download → Verify → Install（§23.6 流程），
   其中数据库迁移前的自动备份由 Core 的 `aihub-server backup` 承担（§28.1 破坏性迁移前备份）。
