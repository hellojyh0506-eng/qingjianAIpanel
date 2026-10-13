# AI 打字识别延迟基线（P1）

2026-10-13。记录 P1（关思考、拆分模型、连接预热、`--ai-bench`）落地时的状态，后续阶段在同一份样本上对比。

## 做了什么

- `ThinkingSwitch` 新增 `EnableThinkingFlag`：SiliconFlow、阿里百炼发 `enable_thinking: false`。原来只有智谱会关，Qwen3 在这两家默认先思考，
  `max_tokens = 96` 的纠错请求被思考吃光，正文为空。
- 打字类请求强制关思考，温度 0 / 0.3；`StreamChatClient` 默认关思考。
- `typing_model` / `panel_model` 分开配置，Windows Server 里学习助手与 AI 问答改读 `panel_model`。
- 连接预热：Worker 启动与空闲 45 秒后的首个请求触发，连接池空闲超时 300 秒。

## 本地基线（不依赖云端，样本 `apps/cli/fixtures/typing_samples.tsv`）

| 类别 | 条数 | 本地 Top1 | 本地 Top3 |
|---|---|---|---|
| typo | 120 | 64.2% | 66.7% |
| clean | 300 | 97.0% | 99.3% |
| context | 60 | 40.0% | 88.3% |

clean 里本地首选本来就对的 291 条是误纠率的分母。

注意：当前 typo 样本里单处错误占多数（相邻键、颠倒、漏、多各 20–25 条），本地已经能救回大部分；多处错与长短语只有 30 条，
所以 P3 的「本地 Top1 ≥ 55%」在这份样本上已经满足，不能用来判断 P3 的收益。P3 开工前要补一批多处错的长串样本。

## 云端数字

沙箱里没有 SiliconFlow 密钥，**还没有跑过真实接口**，下面留空，等有密钥的机器跑：

```
export QINGJIAN_API_KEY=...
cargo run --release -p qingjian-cli -- --predict --ai-bench apps/cli/fixtures/typing_samples.tsv \
  --models Qwen/Qwen3-14B,Qwen/Qwen3-8B --rounds 3
```

| 模型 | P50 | P95 | 无有效建议 | typo Top1 | 误纠率 |
|---|---|---|---|---|---|
| 待测 | | | | | |

用本地 mock 服务验证过：请求带的是 `typing_model`，预热请求 `max_tokens = 1`，温度 0；`--ai-bench` 流程跑通。
