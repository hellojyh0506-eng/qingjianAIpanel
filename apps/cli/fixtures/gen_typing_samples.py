#!/usr/bin/env python3
"""生成 typing_samples.tsv 里的 typo 与 clean 两类样本（context 类是手写的，不在这里）。

词与读音取自 assets/lexicon/dict.tsv，固定随机种子，重跑结果不变：
    python3 apps/cli/fixtures/gen_typing_samples.py > /tmp/generated.tsv
"""
import random

DICT = "assets/lexicon/dict.tsv"
random.seed(20261013)

# 手写的短语：错拼串里有多处错误，计划里点名要覆盖的几类
PHRASES = [
    ("今天我们去吃饭", "jintianwomenquchifan"),
    ("我今天去吃饭", "wojintianquchifan"),
    ("你好吗最近忙", "nihaomazuijinmang"),
    ("我正在开发软件", "wozhengzaikaifaruanjian"),
    ("明天天气怎么样", "mingtiantianqizenmeyang"),
    ("请问这个多少钱", "qingwenzhegeduoshaoqian"),
    ("我想订一张机票", "woxiangdingyizhangjipiao"),
    ("周末一起去爬山", "zhoumoyiqiqupashan"),
    ("这个功能还没做完", "zhegegongnenghaimeizuowan"),
    ("麻烦你帮我看一下", "mafannibangwokanyixia"),
]

NEIGHBORS = {
    "q": "wa", "w": "qes", "e": "wrd", "r": "etf", "t": "ryg", "y": "tuh", "u": "yij", "i": "uok",
    "o": "ipl", "p": "ol", "a": "qsz", "s": "awdx", "d": "sefc", "f": "drgv", "g": "fthb",
    "h": "gyjn", "j": "hukm", "k": "jil", "l": "ko", "z": "ax", "x": "zsc", "c": "xdv",
    "v": "cfb", "b": "vgn", "n": "bhm", "m": "nj",
}


def load():
    words = []
    for line in open(DICT, encoding="utf-8"):
        if line.startswith("#"):
            continue
        parts = line.rstrip("\n").split("\t")
        if len(parts) < 3:
            continue
        word, syl, freq = parts[0], parts[1].split(), int(parts[2])
        if 3 <= len(word) <= 5 and len(word) == len(syl) and all(s.isascii() for s in syl):
            words.append((word, syl, freq))
    words.sort(key=lambda w: -w[2])
    return words


def transpose(syl):
    cand = [(i, j) for i, s in enumerate(syl) for j in range(len(s) - 1) if s[j] != s[j + 1]]
    i, j = random.choice(cand)
    s = syl[i]
    out = list(syl)
    out[i] = s[:j] + s[j + 1] + s[j] + s[j + 2:]
    return "".join(out)


def adjacent(syl):
    i = random.randrange(len(syl))
    s = syl[i]
    j = random.randrange(len(s))
    out = list(syl)
    out[i] = s[:j] + random.choice(NEIGHBORS[s[j]]) + s[j + 1:]
    return "".join(out)


def missing(syl):
    cand = [i for i, s in enumerate(syl) if len(s) >= 3]
    i = random.choice(cand)
    s = syl[i]
    j = random.randrange(1, len(s))
    out = list(syl)
    out[i] = s[:j] + s[j + 1:]
    return "".join(out)


def extra(syl):
    i = random.randrange(len(syl))
    s = syl[i]
    j = random.randrange(len(s))
    out = list(syl)
    out[i] = s[:j + 1] + random.choice([s[j], random.choice(NEIGHBORS[s[j]])]) + s[j + 1:]
    return "".join(out)


def multi(syl, n=2):
    out = list(syl)
    idx = random.sample(range(len(out)), min(n, len(out)))
    ops = [transpose, adjacent, missing, extra]
    for i in idx:
        for op in random.sample(ops, len(ops)):
            try:
                out[i] = op([out[i]])
                break
            except (IndexError, ValueError):
                continue
    return "".join(out)


def main():
    words = load()
    top = words[:2500]
    print("# letters\tbefore\tafter\texpected\tcategory\tnote")
    typo = []
    plan = [(transpose, "transpose", 25), (adjacent, "adjacent", 25), (missing, "missing", 20),
            (extra, "extra", 20), (multi, "multi", 20)]
    used = set()
    for op, note, count in plan:
        made = 0
        while made < count:
            word, syl, _ = random.choice(top)
            if (word, note) in used:
                continue
            try:
                letters = op(syl)
            except (IndexError, ValueError):
                continue
            if letters == "".join(syl):
                continue
            used.add((word, note))
            typo.append((letters, word, note))
            made += 1
    for letters, word, note in typo:
        print(f"{letters}\t\t\t{word}\ttypo\t{note}")
    # 手写长短语：整串里多处错
    for text, letters in PHRASES:
        syl = [letters]
        broken = multi(list(letters[i:i + 4] for i in range(0, len(letters), 4)), 2)
        print(f"{broken}\t\t\t{text}\ttypo\tphrase")
    clean = random.sample(words[:6000], 300)
    for word, syl, _ in clean:
        print(f"{''.join(syl)}\t\t\t{word}\tclean\t")


main()
