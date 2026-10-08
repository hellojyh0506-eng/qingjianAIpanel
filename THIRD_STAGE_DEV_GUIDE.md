# 青简智能输入法（英语学习版）第三次开发交接指南 (Phase 3 Handover)

> **基线版本**：Qingjian Custom Fork v0.1.4  
> **当前稳定 Git 分支**：`owner/v0.1.4-english-learning` (Commit: `156fbf2`)  
> **核心架构**：极薄 Windows TSF COM 客户端 DLL + 本地 Rust 后台守护 Server (Named Pipe) + 异步云端 AI 编排层

---

## 一、 系统架构图与核心运行流

```
[ 宿主应用 (WeChat / Chrome / VSCode) ]
                  │
        (Windows TSF COM 架构)
                  ▼
[ apps/windows/tsf (qingjian_tsf.dll) ]
      - 仅保留极薄按键分发 (key_sink.rs) 与组合键注册 (preserved.rs)
      - Native English Direct Passthrough (原生字母直通，不吃键)
                  │
        (Windows Named Pipe: \\.\pipe\qingjian)
                  ▼
[ apps/windows/server (qingjian-server.exe) ]
      - 状态分发与按键路由 (dispatch/message.rs)
      - 拼音组句与候选重排 (dispatch/composed/mod.rs)
      - 无焦点悬浮窗口绘制与命中测试 (ui/candidates/view.rs, ui/status/)
                  │
      ┌───────────┴───────────┐
      ▼                       ▼
[ crates/qingjian-predict ]  [ crates/qingjian-learning ]
 - 统一 AiService 编排层       - FSRS 间隔重复调度 (fsrs.rs)
 - 异步 HTTP 客户端 (reqwest)   - 个人表达本 LearningVault (vault.rs)
 - System Prompts 提示词体系   - B2 → C1 地道口语升级库 (upgrade.rs)
 - Generation ID 竞态丢弃机制
```

---

## 二、 核心模块与文件导航（精准定位）

如果要基于当前最新代码进行第三次迭代开发，请直接关注以下核心文件：

### 1. 云端 AI 编排、翻译与主动回忆链路
- **`crates/qingjian-predict/src/ai_service.rs`**：统一的服务端 AI 调度器，集中管理翻译、对比评测、口语变体生成与耗时指标记录。
- **`crates/qingjian-predict/src/prompt.rs`**：包含 `TRANSLATE_SYSTEM_PROMPT`（地道当代美式口语提示词）与 `COMPARE_SYSTEM_PROMPT`（主动回忆结构化评分提示词）。
- **`crates/qingjian-predict/src/chat_client.rs`**：底层 HTTP 交互，包含请求耗时分解（TTFT、网络用时、解析用时）与脱敏日志。
- **`crates/qingjian-predict/src/worker.rs`**：异步联想工作线程，负责 150ms 击键防抖、超时熔断与在途请求打断。

### 2. 候选窗口渲染与鼠标命中测试 (UI/UX)
- **`apps/windows/server/src/ui/candidates/view.rs`**：候选窗核心 GDI 绘制代码，包含：
  - `Daily` 模式单行低干扰提示绘制与点击展开；
  - `Learn` 模式地道口语行、`更多译法 ∨`、`Try English` 按钮及命中容差计算；
  - `Drill` 模式主动回忆卡片、英文用户输入框、对比三键（再试一次/保存/完成）绘制；
- **`crates/qingjian-render/src/renderer/learning.rs`**：跨平台 Skia 候选卡片与状态栏 Badge 胶囊（青色“学” / 橙色“练”）渲染。

### 3. 按键拦截、状态机与快捷键分发
- **`apps/windows/server/src/dispatch/message.rs`**：按键分发核心路由，包含：
  - `Ctrl+Shift+E`（切换/显示英文辅助）；
  - `Ctrl+Shift+L`（Daily / Learn / Drill 模式轮播）；
  - `Ctrl+Shift+T`（启动 Drill 主动回忆）；
  - `DrillStage::Input` 阶段英文按键拦截（隔离至 `drill_input`，绝不污染中文拼音）。
- **`apps/windows/tsf/src/com/key/preserved.rs`**：向 Windows 系统注册 preserved key 拦截保留键。
- **`apps/windows/tsf/src/com/service/key_sink.rs`**：TSF `eats_key` 原生直通判断。

### 4. 学习数据持久化与 FSRS 算法
- **`crates/qingjian-learning/src/fsrs.rs`**：纯 Rust 实现的 Free Spaced Repetition Scheduler 算法。
- **`crates/qingjian-learning/src/vault.rs`**：`user-review-queue.json` 与学习画像持久化存储。

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
