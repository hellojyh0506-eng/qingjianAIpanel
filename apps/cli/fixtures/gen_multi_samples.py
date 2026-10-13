#!/usr/bin/env python3
"""生成 typing_samples.tsv 里的 multi 类样本：整句里有 2~3 处互相隔开的敲错（P3 分段修复要救的场景）。

读音用 pypinyin 取（只用来造样本，构建与运行都不依赖它），固定随机种子，重跑结果不变：
    pip install pypinyin
    python3 apps/cli/fixtures/gen_multi_samples.py > /tmp/multi.tsv
每条的错误不碰最后一个字母（用户多半还在敲），错误之间至少隔 3 个字母；
五列 `letters before after expected category`，第六列备注写错误类别。
"""
import random

from pypinyin import Style, lazy_pinyin

random.seed(20261014)

SENTENCES = """
我今天想去超市买东西
明天早上八点开会
请把文件发给我
这个问题我已经解决了
周末我们一起去公园玩
你吃过晚饭了吗
最近工作特别忙
我准备下个月出国旅游
老师让我们明天交作业
他昨天晚上没有睡好
这家餐厅的菜很好吃
我的手机没电了
麻烦你帮我拿一下快递
下班以后我要去健身房
这次考试我考得不错
孩子们正在教室里上课
我们公司最近在招聘新员工
请问附近有地铁站吗
她每天坐公交车上班
今年冬天特别冷
我想学习一门新的语言
周五晚上有一场电影
你能不能再说一遍
我已经把东西收拾好了
这本书我看了三遍
他正在准备明天的演讲
外面下大雨了记得带伞
我打算周末回老家看父母
今天的会议推迟到下午
这个软件的界面很简洁
请你确认一下订单信息
我们需要尽快完成这个项目
他喜欢在周末去爬山
这条路上经常堵车
我昨天买了一台新电脑
她的普通话说得非常标准
你知道最近的医院怎么走吗
我想预约明天的体检
这个季节的水果特别甜
爸爸每天早上都去跑步
我们一起讨论一下方案
他是我大学时候的同学
请把窗户关上谢谢
我对这个结果很满意
这个周末天气会很好
老板要求我们加班完成任务
她正在厨房里做晚饭
我需要买一些生活用品
火车晚点了半个小时
这份报告明天必须交上去
我最喜欢的季节是秋天
他们已经到达机场了
你可以把地址告诉我吗
今天的作业有点多
我们学校的操场很大
请给我一杯热茶
他的建议对我帮助很大
这个周末我要整理房间
朋友邀请我参加他的婚礼
我昨晚做了一个奇怪的梦
公司楼下新开了一家咖啡店
你觉得这个主意怎么样
我想把这些照片打印出来
他正在学习如何做饭
今天下午有一场重要的比赛
我们明天早上在门口集合
这件衣服的颜色很好看
我已经预订了两张电影票
请大家保持安静
他每个月都会给家里寄钱
这个月的销售额增长了很多
我想知道这件事的真相
她最近一直在准备考试
我们的计划需要重新调整
今晚的月亮又大又圆
这个城市的夜景非常漂亮
请把这份合同仔细看一遍
我的电脑突然黑屏了
他说话的声音很好听
我们在图书馆里复习功课
明年我打算换一份工作
这个游戏的画面做得很精致
奶奶给我们包了饺子
他骑自行车去公司上班
我觉得这个方案可行
请问洗手间在哪里
今天路上遇到了老朋友
我们讨论了一整个下午
她刚刚搬到了新的住处
这家店的服务态度很好
我想订一个生日蛋糕
他们正在讨论预算问题
你记得把钥匙带上
这次旅行让我印象深刻
我们需要一个更好的解决办法
明天的天气预报说有雨
我习惯每天晚上看书
他送给我一份特别的礼物
请你把声音调小一点
公园里的花都开了
我正在等一个重要的电话
""".strip().splitlines()

NEIGHBORS = {
    "q": "wa", "w": "qes", "e": "wrd", "r": "etf", "t": "ryg", "y": "tuh", "u": "yij", "i": "uok",
    "o": "ipl", "p": "ol", "a": "qsz", "s": "awdx", "d": "sefc", "f": "drgv", "g": "fthb",
    "h": "gyjn", "j": "hukm", "k": "jil", "l": "ko", "z": "ax", "x": "zsc", "c": "xdv",
    "v": "cfb", "b": "vgn", "n": "bhm", "m": "nj",
}
KINDS = ["transpose", "adjacent", "drop", "dup"]


def apply(letters, pos, kind):
    """在 pos 处造一处错；返回新串与占用的字母区间（用来保证错误互相隔开）。"""
    ch = letters[pos]
    if kind == "transpose" and pos + 1 < len(letters) and letters[pos + 1] != ch:
        return letters[:pos] + letters[pos + 1] + ch + letters[pos + 2:], 0
    if kind == "adjacent":
        return letters[:pos] + random.choice(NEIGHBORS[ch]) + letters[pos + 1:], 0
    if kind == "drop":
        return letters[:pos] + letters[pos + 1:], -1
    if kind == "dup":
        return letters[:pos] + ch + letters[pos:], 1
    return None, 0


def make(sentence):
    letters = "".join(lazy_pinyin(sentence, style=Style.NORMAL))
    if not letters.isascii() or not letters.isalpha():
        return None
    count = random.choice([2, 2, 3])
    # 从右往左造错，这样左边的位置不会因为右边的增删而移动
    positions = sorted(random.sample(range(2, len(letters) - 2), count), reverse=True)
    if any(a - b < 3 for a, b in zip(positions, positions[1:])):
        return None
    out, kinds = letters, []
    for pos in positions:
        kind = random.choice(KINDS)
        new, _ = apply(out, pos, kind)
        if new is None or new == out:
            return None
        out = new
        kinds.append(kind)
    return out, letters, "+".join(reversed(kinds))


rows = []
for sentence in SENTENCES:
    for _ in range(20):
        made = make(sentence)
        if made:
            typed, clean, kinds = made
            rows.append(f"{typed}\t\t\t{sentence}\tmulti\t{kinds} {clean}")
            break
for row in rows:
    print(row)
