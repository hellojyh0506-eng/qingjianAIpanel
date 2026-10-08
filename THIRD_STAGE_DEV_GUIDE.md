# 青简智能输入法（英语学习版）第三次开发交接指南 (Phase 3 Handover)

> **基线版本**：Qingjian Custom Fork v0.1.4  
> **当前稳定 Git 分支**：`owner/v0.1.4-english-learning` (Commit: `9b5ddea`)  
> **核心架构**：极薄 Windows TSF COM 客户端 DLL (Chord-first 修饰键保护) + 本地 Rust 后台守护 Server (Named Pipe) + 独立展示伴侣窗 (Companion Panel, WS_EX_NOACTIVATE) + 独立主动练习窗 (Drill Session) + 异步云端 AI 编排层 (首句 300~500ms, 1500ms 超时本地词典兜底)

---

## 一、 系统架构图与核心运行流

```
[ 宿主应用 (WeChat / Chrome / VSCode) ]
                  │
        (Windows TSF COM 架构, Chord-first 组合键保护, 防止 Shift 误切丢拼音)
                  ▼
[ apps/windows/tsf (qingjian_tsf.dll) ]
      - 仅保留极薄按键分发 (key_sink.rs) 与保留键注册 (preserved.rs: Ctrl+Alt+E, Ctrl+Alt+D)
      - Native English Direct Passthrough (原生字母直通，不吃键)
                  │
        (Windows Named Pipe: \\.\pipe\qingjian)
                  ▼
[ apps/windows/server (qingjian-server.exe) ]
      - 状态分发与按键路由 (dispatch/message.rs)
      - 纯净候选窗口渲染 (ui/candidates/view.rs, 绝不臃肿塞大表单)
      - 独立学习助手伴侣窗 (ui/companion.rs, WS_EX_NOACTIVATE, 430x280, 不抢焦点)
      - 独立主动练习窗口 (ui/drill.rs, 460x340, 独立打字输入框与 AI 评测卡片)
                  │
      ┌───────────┴───────────┐
      ▼                       ▼
[ crates/qingjian-predict ]  [ crates/qingjian-learning ]
 - 美式口语教练 Prompt 体系   - FSRS 间隔重复调度 (fsrs.rs)
 - max_tokens: 36 低延迟控制   - 个人表达本 LearningVault (vault.rs)
 - 1500ms 平滑本地词典回退    - B2 → C1 地道口语升级库 (upgrade.rs)
 - Generation ID 竞态丢弃机制
```

---

## 二、 核心模块与文件导航（精准定位）

如果要基于当前最新代码进行后续迭代开发，请直接关注以下核心文件：

### 1. 独立窗口体系与候选框瘦身 (UI/UX)
- **`apps/windows/server/src/ui/candidates/view.rs`**：主候选窗口彻底瘦身：
  - `Daily`：中文候选下方仅单行 `🇺🇸 That makes perfect sense. 🔊`（极简纯粹，首选 300~500ms 体感）；
  - `Learn`：单行口语 + `📖 学习助手 (Ctrl+Alt+E)` 轻胶囊按钮；
  - `Drill`：单行口语 + `💡 进入练习 (Ctrl+Alt+D)` 轻胶囊按钮（彻底废除旧版 Candidate 内嵌 Try English）。
- **`apps/windows/server/src/ui/companion.rs`**：独立 English Assistant 伴侣浮窗（430x280，`WS_EX_NOACTIVATE` 纯阅读展示，绝不抢宿主焦点，不破坏拼音 composition）：
  - 4 个轻量 Tab（更多译法、用法说明、高频搭配、场景例句）；
  - 快捷发音、剪贴板一键复制、收藏到表达本、直达练习。
- **`apps/windows/server/src/ui/drill.rs`**：独立 Drill Session 练习浮窗（460x340，获取焦点主动打字）：
  - 独立输入框、实时字数统计、Enter 触发对比、Tab 查看答案、AI 结构化批改反馈卡片。

### 2. TSF 修饰键物理检测与 Shift 防误切 (P0 核心稳定性)
- **`apps/windows/tsf/src/com/key/tap.rs`**：`is_other_modifier_down` 物理轮询检测 Ctrl / Alt / Win，只要伴随其他修饰键，立即 `cancel()` 纯 Shift 切换，彻底杜绝组合键切换中英文导致拼音 composition 丢失。
- **`apps/windows/tsf/src/com/key/preserved.rs`**：注册 `Ctrl+Alt+E`（打开学习助手）与 `Ctrl+Alt+D`（进入主动练习）系统保留键。

### 3. 云端 AI 编排、提示词与 1500ms 离线词典兜底
- **`crates/qingjian-predict/src/prompt.rs`**：地道当代美式口语教练提示词与主动回忆结构化评分提示词。
- **`crates/qingjian-predict/src/chat_client.rs`**：配置 `TRANSLATE_MAX_TOKENS = 36`，低延迟快速生成。
- **`apps/windows/server/src/dispatch/composed/mod.rs`**：
  - 超时从 3500ms 降至 1500ms；
  - 超时平滑回退至 `self.engine.translate_word(target)` 本地离线词典，消除“暂时没有译文”空白。

---

## 三、 第三次开发（Phase 3）核心改进建议点

1. **AI 延迟优化与模型路由**：
   - 当前已配置 SiliconFlow `Pro/Qwen/Qwen2.5-7B-Instruct`（专享推理集群，TTFT 150~250ms）或智谱 `glm-4-flash`（免费）。
   - 在 `chat_client.rs` 中确保携带 `"max_tokens": 32`，防止模型生成长篇大论拖慢网络。
2. **离线本地词典兜底 (Fallback)**：
   - 在 `apps/windows/server/src/dispatch/composed/mod.rs` 中：若云端网络超时（1500~2000ms），秒级回退至 `glossary.qj` 本地离线释义，绝不显示“暂时没有译文”空白。
3. **本地高频词 LRU 缓存**：
   - 针对高频词（如“确实有道理”、“没问题”）增加内存级 LRU 缓存，二次输入时 0ms 瞬间上屏。

---

## 四、 本地编译与部署指南

### 1. 编译工具链
推荐使用与当前环境一致的 GNU 工具链：
```bash
# 检查工具链
cargo +1.96.0-x86_64-pc-windows-gnu --version

# 全量构建 Release 产物
cargo +1.96.0-x86_64-pc-windows-gnu build --release
```

### 2. 产物输出位置
- 服务端主程序：`target/release/qingjian-server.exe`
- TSF COM DLL：`target/release/qingjian_tsf.dll`

### 3. 部署生效方法
1. 终止正在运行的服务：`Stop-Process -Name qingjian-server -Force`
2. 将产物替换至安装目录（如 `E:\Qingjian`）：
   - `qingjian-server.exe`
   - `qingjian_tsf-0.1.4.dll`（若系统进程占用，需重启应用或临时更名替换）
3. 启动新服务：运行 `E:\Qingjian\qingjian-server.exe`，检查日志确认命名管道 `\\.\pipe\qingjian` 监听正常。
