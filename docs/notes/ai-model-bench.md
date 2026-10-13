# 打字模型实测（P6）

用 `--ai-bench` 跑当前的 Resolve 请求（提示词见 `crates/qingjian-predict/src/resolve/prompt.rs`），SiliconFlow 真实接口，临时密钥，Linux 沙箱。

```
export QINGJIAN_API_KEY=...
cargo run --release -p qingjian-cli -- --predict --ai-bench apps/cli/fixtures/typing_samples.tsv \
  --models <模型名> --rounds 1 --bench-limit 30
```

## 条件与局限（先看这个）

- **每类取前 30 条（clean / context / typo / multi 各 30，共 120 条）、只跑 1 轮，不是计划里的 3 轮取中位数。** 样本小，±3 个百分点内的差别不要当真。
- **沙箱到 SiliconFlow 单次往返约 0.5 s（TCP 0.53 s + TLS 1.0 s）**，所以下面的延迟只用来互相比较，不代表用户网络；「P50 ≤ 700 ms」在这里无法验证。
- 模型之间是串行跑的（不并发），每个模型各自一次进程，没有缓存。
- 计划里的候选是 Qwen3-8B / Qwen2.5-7B / Qwen3-30B-A3B-Instruct-2507 / Qwen3-14B；用户后来给了 SiliconFlow 的新价目表，所以加测了 Qwen3.5 系列、DeepSeek-V4-Flash、GLM-5.3-Flash、Step-3.5-Flash。Qwen3-8B 只在早期用每类 10 条测过。

## 结果

本地（不调接口）作对照：context Top1 30.0% / Top3 83.3%，multi 23.3%，typo 86.7%，clean 96.7%。

| 模型 | 价格 入 / 出（¥/M） | context +AI Top1 / Top3 | typo +AI Top1 | multi +AI Top1 | 无有效建议 context / typo / multi | context P50 / P95 | clean 误纠 |
|---|---|---|---|---|---|---|---|
| **Qwen3.5-35B-A3B** | 0.4 / 3.2 | **90.0% / 90.0%** | 86.7%（持平） | 26.7% | 0% / 3.6% / 27.6% | 1098 / **1251 ms** | 0 / 29 |
| Qwen3.5-122B-A10B | 0.8 / 6.4 | 86.7% / 96.7% | 86.7%（持平） | 23.3% | 0% / 7.1% / 27.6% | 1057 / 1460 ms | 0 / 29 |
| DeepSeek-V4-Flash | 1.5 / 4.5（夜）3 / 9（日） | 73.3% / 86.7% | 86.7%（持平） | 26.7% | 13.3% / 17.9% / 34.5% | 1361 / 2631 ms | 0 / 29 |
| Qwen3-30B-A3B-Instruct-2507 | 未查 | 53.3% / 83.3% | 86.7%（持平） | 23.3% | 0% / 10.7% / 17.2% | 896 / 1299 ms | 0 / 29 |
| Qwen2.5-7B-Instruct | 未查 | 50.0% / 90.0% | **80.0%（比本地低）** | 23.3% | 0% / 7.1% / 27.6% | 765 / 1500 ms | 0 / 29 |
| Qwen3-14B | 未查 | 43.3% / 83.3% | **80.0%（比本地低）** | 23.3% | 0% / 35.7% / 34.5% | 988 / 1430 ms | 0 / 29 |
| Qwen3.5-27B（每类 10 条） | 0.6 / 4.8 | 80% / 80% | 90%（持平） | 10%（本地 10%） | 20% / 0% / 55.6% | 1208 / 2628 ms | 0 / 10 |
| Qwen3-8B（每类 10 条） | 未查 | 60% / 80% | 90%（持平） | 10%（本地 10%） | 0% / 0% / 55.6% | 1204 / 1379 ms | 0 / 10 |
| GLM-5.3-Flash | 0.8 / 2.8 | 不可用：每条都超过 2.5 s 超时（没有关掉思考） | | | | | |
| Step-3.5-Flash | 0.7 / 2.1 | 不可用：400「Json mode is not supported for this model」 | | | | | |

## 选择

规则（计划）：解析通过率 ≥ 95%，错拼 Top-1 不低于最好成绩 5 个百分点以内，在这些里选 P95 最低的。

- 「解析通过率」按「无有效建议」看（空回复 / 坏回复 / 超时 / 被本地校验拒绝）。context 与 typo 上 Qwen3.5-35B-A3B 满足（0% / 3.6%）；multi 各个模型都有 17–35% 被拒绝，是模型回的拼音本身就错，**没有模型满足 ≥ 95%**，所以这一项只在 context / typo 上判断。
- 满足的有 Qwen3.5-35B-A3B（P95 1251 ms）和 Qwen3.5-122B-A10B（P95 1460 ms），前者 P95 更低、价格是后者的一半，context Top1 还高 3.3 个百分点 → **选 `Qwen/Qwen3.5-35B-A3B`**。
- 老一代的 Qwen3-8B / 14B / 30B-A3B / Qwen2.5-7B 在 context 上只有 43–60%，其中 14B 与 2.5-7B 在 typo 上还把本地原本对的结果带偏了（80% < 本地 86.7%），不选。
- `panel_model` 保持 `Qwen/Qwen3-14B`（面板不在这条打字链路里，没有重测）。

## 没解决的

- **multi（多处错拼的整句）所有模型都没有增益**（23.3% → 最高 26.7%）：模型回的拼音常常掉字母、漏音节或保留原来的敲错。这块收益主要靠 P3 的本地分段修复，不指望云端模型。
- **typo 上 AI 不增益**：本地已经能纠，AI 只是把本地的纠正候选选回来；它的价值在 context（同音字按上下文选）。
- P50 ≤ 700 ms 没验证；真实用户网络下的延迟要在用户机器上用同一条命令再跑一次（`--bench-limit 30 --rounds 3`）。
- 小样本 1 轮，建议在用户机器上用 3 轮复核 Qwen3.5-35B-A3B 与 Qwen3.5-122B-A10B 两个。

## 默认值

`PredictConfig::default().typing_model` 与配置模板的 `typing_model` 都改成 `Qwen/Qwen3.5-35B-A3B`（常量 `DEFAULT_TYPING_MODEL`）。**已经生成过 `config.toml` 的用户不会自动换**：配置里写的是 `typing_model = ""`（回落到 `model`，即 Qwen3-14B），需要手动改成新值，或在偏好设置里改。换了 `base_url` 的用户要把它改成该接口的模型名或留空。
