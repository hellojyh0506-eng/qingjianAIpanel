import urllib.request, json, sys, os
sys.stdout.reconfigure(encoding="utf-8")
from benchmark_qwen import call_qwen

RERANK_PROMPT = """你是一个输入法的候选语义重排引擎。你会收到一段 JSON：
candidates（待重排的合法完整句子候选列表）、before（前文上下文，可为空）。
要求：
1. 仅在提供的 candidates 列表中按现代汉语语义自然度与上下文合理性进行重排；
2. 严禁生成不在 candidates 列表中的任何新句子；
3. 输出严格 JSON：{"ranking": [{"candidate": "…", "score": 0.9}, ...]}
"""

user_payload = {
    "candidates": ["他有一件很棒的裙子", "她有一件很棒的裙子"],
    "before": ""
}
res = call_qwen(RERANK_PROMPT, user_payload)
print("RERANK RESULT:", res["content"])
