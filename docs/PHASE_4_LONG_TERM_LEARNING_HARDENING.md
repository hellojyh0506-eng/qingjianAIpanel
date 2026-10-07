# Qingjian Long-Term Learning & Personalization Hardening 报告

## 一、核心设计哲学与“防学坏”防线 (Anti-Corruption Architecture)

### 1.1 根本矛盾与原则
输入法长期自适应（Personalization）最大的工业隐患是：**将用户的无意输入错误当成了个性化偏好进行恶性强化**（如用户无意间选错了同音字，系统把错误候选权重提权，导致后续越用错得越频繁）。

为了彻底根绝“越用越学坏”，Qingjian 建立了严格的**防学坏护栏（Anti-Corruption Guardrail）**：
- **核心原则**：严格区分“用户经常输入（Observed）”与“用户正确输入（Corrected / Learned）”。未纠正的单次错误绝不会直接成为高权重标准答案；
- **三态生命周期模型（3-State Lifecycle Model）**：
  $$\text{Observed (仅观察，0 加权)} \longrightarrow \text{Corrected (用户主动退格纠偏，初步信任)} \longrightarrow \text{Learned (多次稳定确认，固化偏好)}$$

---

## 二、三态生命周期状态转移模型

### 2.1 状态定义
| 状态 | 触发条件 | 中文候选加权 | 英文 Active Recall 行为 |
| :--- | :--- | :--- | :--- |
| **`Observed`** | 仅被用户正常输入上屏，未发生退格更正；或词条仅被被动曝光 | $\pm 0.0$（不予偏置，防止将偶发误选强化） | 仅计入曝光次数与常规复习队列，不设为首选替代表达 |
| **`Corrected`** | 用户发生误选后，立即执行退格（`apply_retraction`）撤销并重选正确词；或主动回忆单次成功输入认可的自然口语替代表达 | 正确词 $+3.0$ / 误选词 $-3.0$ | 记入成功计次与候选替代表达频次表，暂不升级为首选 |
| **`Learned`** | 退格纠正 $\ge 2$ 次，或纠正 1 次 + 后续相同语境无退格确认直接选择 $\ge 2$ 次；或 Active Recall 中同一认可口语替代表达成功使用 $\ge 2$ 次（或累计成功 $\ge 3$ 次） | 正确词 $+5.0$ / 误选词 $-5.0$ | 固化为该中文词条的 `preferred_alternative`，后续上屏候选直接标注展示用户偏好的地道口语表达 |

### 2.2 防学坏具体机制对比
| 场景 | 传统朴素频次统计（易学坏） | Qingjian 三态防学坏架构 |
| :--- | :--- | :--- |
| 用户在“跑”后不小心选了“的”且未退格 | “跑的”频次增加，后续优先推荐“跑的” ❌ | 属于 `Observed`，绝不建立惩罚“跑得”的更正规则，不偏置加权 ✅ |
| 用户在“跑”后误选“的”，随后退格更正为“得” | 仅撤销计数，无法学到用户的纠错意图 ❌ | 触发 `record_correction("跑", "的", "得")`，进入 `Corrected` 状态，对“得”提权 $+3.0$，“的”降权 $-3.0$ ✅ |
| 用户在 Active Recall 输入“resolve”回答“搞定了” | 乱计入或直接判错无沉淀 | 判定未匹配，`Failure` 计次 $+1$，**严格禁止进入 `alternative_counts`**，绝不污染首选词库 ✅ |
| 用户在 Active Recall 输入“We're good”回答“搞定了” | 判定为错误（因为词典只有 It's done） ❌ | 识别为 Approved Natural Spoken Alternative，计入替代计数；达到 2 次后跃迁至 `Learned`，首选替代表达生效 ✅ |

---

## 三、九大核心维度审计与加固落地清单

### 1. 个人词库 (`user-words.tsv`, `user-english.tsv`)
- **存储**：`%APPDATA%\Qingjian\user-words.tsv` 与 `user-english.tsv`；
- **机制**：用户自定义或长串组合词，按前缀匹配与纯拼音索引，私密模式下零写入。

### 2. 用户词频 (`user.tsv`)
- **存储**：`%APPDATA%\Qingjian\user.tsv`；
- **机制**：对常规用户提交记录次数，并受 `MAX_CHOICE_ENTRIES` 半衰期衰减保护，防止历史陈旧热度长期霸榜。

### 3. 用户 N-gram (`user-ngram.tsv`)
- **存储**：`%APPDATA%\Qingjian\user-ngram.tsv`；
- **机制**：基于前向三元（Trigram）与二元（Bigram）语言模型插值，记录上下文转移概率。

### 4. 常见纠错 (`user-corrections.tsv`)
- **存储**：`%APPDATA%\Qingjian\user-corrections.tsv`；
- **机制**：专用格式 `上下文 \t 误选词 \t 更正词 \t 状态 \t 更正次数 \t 确认次数 \t 最近更新`，毫秒级内存加载与上下文末尾后缀索引。

### 5. 高频混淆词集合 (Frequently Confused Sets)
- 内置针对汉语典型混淆集合：
  - `{"他", "她", "它"}`
  - `{"在", "再"}`
  - `{"的", "地", "得"}`
  - `{"做", "作"}`
  - `{"哪", "那"}`
  - `{"像", "象"}`
  - `{"纸张", "智障"}`
  - `{"傻子", "啥子", "沙子"}`
  - `{"微信输入法", "为新输入法"}`

### 6. 高频曝光英文表达 (Frequently Exposed Expressions)
- **机制**：`RecallBook::frequently_exposed_items(min_seen >= 5)`，追踪用户日常打字中经常遇到但尚未主动复习的高频表达。

### 7. 主动回忆成败追踪 (Recall Success / Failure Tracking)
- **机制**：每次 Active Recall 严格记录 `success`、`failure`、`revealed` 次数与日期，按加权掌握度智能动态排定复习优先级。

### 8. 卡点与常看答案表达 (Frequently Revealed Bottlenecks)
- **机制**：`RecallBook::frequently_revealed_items(min_revealed >= 2)` 与 `struggling_items`，精准识别用户“卡壳”、“必须按 Tab 看答案”的认知盲区。

### 9. 高频自然口语替代表达 (Frequently Used Natural Alternatives)
- **机制**：`alternative_counts` 聚合经认定的 14 类日常地道口语簇（如 "We're good", "Got it", "All set" 等）。成功使用达到 2 次后跃迁为 `Learned`，并反哺至输入法候选释义与主动回忆通知。

---

## 四、本地数据安全、隐私模式隔离与可追溯性

1. **100% 本地存储**：所有学习数据均以纯文本 TSV 格式保存在 `%APPDATA%\Qingjian\`，用户可直接用记事本查阅、审计或手动清空；
2. **私密输入模式完全隔离**：
   - 当检测到密码框、PIN 或开启私密输入模式时，`Learner` 与 `RecallTracker` 内部均受 `!self.private` 与 `!self.muted()` 保护；
   - 保证**零学习、零记录、零盘写、零云端上传**；
3. **向后兼容与平滑回滚**：
   - `user-recall.tsv` 支持 9 列（旧版）与 11 列（新版含 preferred_alternative 与 state）自动兼容读取，旧数据平滑升级，不破坏已有记录。

---

## 五、验证与验收证据链

| 验证层级 | 验证范围 | 测试结果 | 证据标签 |
| :--- | :--- | :--- | :--- |
| **`qingjian-core`** | 308 个全量单元与集成测试（含混淆消歧、纠错调整、候选标注） | 308 passed, 0 failed | `[CODE-VERIFIED]` |
| **`qingjian-learning`** | 26 个全量测试（三态生命周期、防学坏防护、TSV 往返、14 个美式口语基准） | 26 passed, 0 failed | `[CODE-VERIFIED]` |
| **`qingjian-windows-server`** | 98 个全量集成测试（Direct Mode、Engine Loop、Active Recall 闭环） | 98 passed, 0 failed | `[CODE-VERIFIED]` |
| **`qingjian-windows-tsf`** | 23 个全量测试（含 Win32 消息投递、TSF COM 声明周期实机回归） | 23 passed, 0 failed | `[CODE-VERIFIED]` |
| **Named Pipe 管道端到端** | 真实 Win32 命名管道交互（曝光、Ctrl+Alt+R 触发、键入替代、Enter 提交） | 3 passed, 0 failed | `[REAL-EMPIRICAL]` |
| **Windows 宿主部署** | 编译 Release 产物覆盖 `E:\Qingjian` 并经 WMI 守护重启，管道连接就绪 | PID 28212 存活 | `[REAL-EMPIRICAL]` |
