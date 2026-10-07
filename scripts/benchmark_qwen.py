import urllib.request
import json
import time
import statistics
import os

API_KEY = "sk-qryjgvkzekfbbdlpjlvepwbeexnzakeffvjqpgwrystkjzef"
BASE_URL = "https://api.siliconflow.cn/v1/chat/completions"
MODEL = "Qwen/Qwen2.5-7B-Instruct"

SYSTEM_PROMPT = """你是一个拼音输入法的云端联想引擎。用户正在打拼音，还没选词。你会收到一段 JSON：
letters（用户实际敲的字母）、pinyin（输入法切分）、syllables、before / after、local_sentence、local_candidates、max_items、want_sentence。
local_candidates 和 local_sentence 只是本地的猜测，可能全错。请只根据 letters 与 before / after 独立判断用户想打什么。
输出 JSON：{"words": [{"text": "…", "pinyin": "…"}], "sentence": "…" 或 null}
不解释、不加引号、不加序号。"""

TRANSLATE_SYSTEM_PROMPT = """你是一个输入法的翻译与英语学习助手。你会收到一段 JSON：
text（待翻译的原文）、target_language（目标语言代码：zh 中文、en 英语、ja 日语）。
要求：
1. 当 target_language 为 en（英语）时，译文必须是当代地道美式口语（Natural American Spoken English），符合真实日常会话习惯；
2. 优先采用整句口语搭配与习惯表达，拒绝生硬直译、书面教条或字典式死板选词；
3. 保持自然真实的对话语调，严禁生塞网络俚语；
4. 保留原文语气；
5. 绝不解释、不加引号、不加前缀。
输出 JSON：{"sentence": "译文"}"""

def call_qwen(system_prompt, user_content, timeout=4.0):
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }
    body = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": json.dumps(user_content, ensure_ascii=False)},
        ],
        "max_tokens": 150,
        "temperature": 0.3,
        "response_format": {"type": "json_object"}
    }
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(BASE_URL, data=data, headers=headers, method="POST")
    
    start = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            elapsed_ms = (time.perf_counter() - start) * 1000
            res_body = json.loads(resp.read().decode("utf-8"))
            choice = res_body["choices"][0]["message"]["content"]
            usage = res_body.get("usage", {})
            return {
                "ok": True,
                "latency_ms": elapsed_ms,
                "content": choice,
                "usage": usage,
                "error": None
            }
    except Exception as e:
        elapsed_ms = (time.perf_counter() - start) * 1000
        return {
            "ok": False,
            "latency_ms": elapsed_ms,
            "content": None,
            "usage": {},
            "error": str(e)
        }

def run_benchmarks():
    tasks = {
        "Task A: Normal Pinyin (日常通用拼音)": [
            {"letters": "nihao", "pinyin": "ni'hao", "syllables": 2, "before": "", "after": "", "local_sentence": "你好", "local_candidates": ["你好", "拟好", "泥壕"], "max_items": 3, "want_sentence": False},
            {"letters": "kaifa", "pinyin": "kai'fa", "syllables": 2, "before": "软件", "after": "", "local_sentence": "开发", "local_candidates": ["开发", "开放"], "max_items": 3, "want_sentence": False},
            {"letters": "gongzuo", "pinyin": "gong'zuo", "syllables": 2, "before": "认真", "after": "", "local_sentence": "工作", "local_candidates": ["工作", "动作"], "max_items": 3, "want_sentence": False},
        ],
        "Task B: Typo Correction (敲错轻微纠正)": [
            {"letters": "shurufa", "pinyin": "shu'ru'fa", "syllables": 3, "before": "微信", "after": "", "local_sentence": "输入法", "local_candidates": ["输入法"], "max_items": 3, "want_sentence": False},
            {"letters": "zhngguo", "pinyin": "zhong'guo", "syllables": 2, "before": "伟大", "after": "", "local_sentence": "中国", "local_candidates": ["中国"], "max_items": 3, "want_sentence": False},
        ],
        "Task C: High Ambiguity & Semantic Disambiguation (高歧义/语境消歧)": [
            {"letters": "zhizhang", "pinyin": "zhi'zhang", "syllables": 2, "before": "是很", "after": "", "local_sentence": "纸张", "local_candidates": ["纸张", "智障"], "max_items": 3, "want_sentence": False},
            {"letters": "sha", "pinyin": "sha", "syllables": 1, "before": "我不知道你在说", "after": "", "local_sentence": "沙", "local_candidates": ["沙", "杀", "啥"], "max_items": 3, "want_sentence": False},
            {"letters": "ta", "pinyin": "ta", "syllables": 1, "before": "今天她吃饭但是", "after": "", "local_sentence": "他", "local_candidates": ["他", "她", "它"], "max_items": 3, "want_sentence": False},
            {"letters": "shazi", "pinyin": "sha'zi", "syllables": 2, "before": "你不会是个", "after": "", "local_sentence": "沙子", "local_candidates": ["沙子", "傻子", "啥子"], "max_items": 3, "want_sentence": False},
            {"letters": "de", "pinyin": "de", "syllables": 1, "before": "跑", "after": "快", "local_sentence": "的", "local_candidates": ["的", "地", "得"], "max_items": 3, "want_sentence": False},
        ],
        "Task D: Natural Spoken English Translation (地道美式口语重写)": [
            {"text": "搞定了", "target_language": "en", "context": ""},
            {"text": "你咋还不来", "target_language": "en", "context": ""},
            {"text": "随便吧", "target_language": "en", "context": ""},
            {"text": "没事", "target_language": "en", "context": ""},
            {"text": "我服了", "target_language": "en", "context": ""},
            {"text": "真的假的", "target_language": "en", "context": ""},
            {"text": "太离谱了", "target_language": "en", "context": ""},
        ],
        "Task E: Cloud Sentence Completion (云端整句联想)": [
            {"letters": "suoyi", "pinyin": "suo'yi", "syllables": 2, "before": "今天下大雨，", "after": "", "local_sentence": "所以", "local_candidates": ["所以"], "max_items": 2, "want_sentence": True},
            {"letters": "jintian", "pinyin": "jin'tian", "syllables": 2, "before": "大家好，", "after": "", "local_sentence": "今天", "local_candidates": ["今天"], "max_items": 2, "want_sentence": True},
        ]
    }

    results = {}
    print(f"Starting Qwen2.5-7B-Instruct Empirical Routing Benchmark...")
    print(f"API Target: {BASE_URL} (Model: {MODEL})")
    print("=" * 70)

    for category, samples in tasks.items():
        print(f"\nEvaluating {category} (N={len(samples)})...")
        cat_latencies = []
        cat_prompt_tokens = []
        cat_completion_tokens = []
        successes = 0
        timeouts = 0
        fallbacks = 0
        details = []

        is_trans = "Translation" in category
        sys_prompt = TRANSLATE_SYSTEM_PROMPT if is_trans else SYSTEM_PROMPT

        for sample in samples:
            res = call_qwen(sys_prompt, sample)
            lat = res["latency_ms"]
            cat_latencies.append(lat)
            if res["ok"]:
                successes += 1
                usage = res["usage"]
                cat_prompt_tokens.append(usage.get("prompt_tokens", 0))
                cat_completion_tokens.append(usage.get("completion_tokens", 0))
                details.append({
                    "sample": sample,
                    "latency_ms": lat,
                    "output": res["content"]
                })
            else:
                fallbacks += 1
                if "timed out" in res["error"]:
                    timeouts += 1
                details.append({
                    "sample": sample,
                    "latency_ms": lat,
                    "error": res["error"]
                })

        p50 = statistics.median(cat_latencies) if cat_latencies else 0
        sorted_lats = sorted(cat_latencies)
        p95_idx = int(len(sorted_lats) * 0.95)
        p95 = sorted_lats[p95_idx] if sorted_lats else 0
        avg_prompt_tok = statistics.mean(cat_prompt_tokens) if cat_prompt_tokens else 0
        avg_comp_tok = statistics.mean(cat_completion_tokens) if cat_completion_tokens else 0

        results[category] = {
            "p50_ms": p50,
            "p95_ms": p95,
            "min_ms": min(cat_latencies) if cat_latencies else 0,
            "max_ms": max(cat_latencies) if cat_latencies else 0,
            "timeout_rate": timeouts / len(samples) if samples else 0,
            "fallback_rate": fallbacks / len(samples) if samples else 0,
            "avg_prompt_tokens": avg_prompt_tok,
            "avg_completion_tokens": avg_comp_tok,
            "details": details
        }
        print(f"  P50: {p50:.1f}ms | P95: {p95:.1f}ms | Avg Tokens: In {avg_prompt_tok:.1f}, Out {avg_comp_tok:.1f} | Success: {successes}/{len(samples)}")

    with open("scripts/benchmark_results.json", "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    print("\nBenchmark completed. Saved to scripts/benchmark_results.json")

if __name__ == "__main__":
    run_benchmarks()
