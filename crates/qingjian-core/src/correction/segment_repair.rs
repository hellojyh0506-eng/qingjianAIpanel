//! 分段修复：一串里有好几处敲错（`wojintinxiangquchashijaidongxi`）时，一处编辑和有限两处编辑都救不回来。
//! 这里按音节从左到右走：每个位置先列出「原样就是合法音节」与「改一个字母后是合法音节」的走法，
//! 各自带敲错代价，再找总代价最低的几条路，输出整串纠正后的拼音，由 Engine 用整句转换挑最通顺的。
//!
//! 只负责「在哪儿改、怎么改」，不碰词库：路径总代价超出最低代价一点点就不要，编辑总数不超过 `ceil(长度 / 6)`；
//! 改动的位置不碰刚敲下的最后一个字母（用户多半还在敲）。末尾没敲完的音节前缀原样保留。

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::parser::{self, Segmentation, Syllable};

use super::{Correction, Edit, MAX_LETTERS, MIN_LETTERS, TypoCosts, eligible, typo};

/// 每个位置最多保留多少个部分结果。
const BEAM: usize = 96;

/// 最多交出多少个整串候选（按代价从低到高）。
pub const MAX_CANDIDATES: usize = 64;

/// 比最低代价贵多少以内的路径还要：容得下「换位 vs 换字母」这种同量级的备选。
const SLACK: f64 = 3.0;

/// 纠正结果末尾还留着没敲完的音节时，原串至少要这么长才算：短串加残缺尾巴太含糊
/// （英文词、缩写 `kaifaid` 都会被「纠」成一串半截拼音），长串靠整句通顺度才有足够证据。
const MIN_LETTERS_FOR_PARTIAL_TAIL: usize = 10;

/// 每 6 个字母允许一处编辑。
const LETTERS_PER_EDIT: usize = 6;

/// 少敲时最常漏掉的字母：元音与 n / g / h / r（ng、zh / ch / sh、er）。
/// 多敲的字母既不是连击也不是邻键时的加罚
const UNEXPLAINED_EXTRA_SURCHARGE: f64 = 1.0;

/// 残缺尾巴的读法要比最便宜的完整读法省下至少这么多代价才保留（约半次编辑以上，即少一处编辑）。
const PARTIAL_TAIL_MARGIN: f64 = 1.0;

const LIKELY_MISSING: &[u8] = b"aeiouvywnghr";

#[derive(Debug, Clone, Copy)]
enum Step {
    Exact,
    Substitute {
        offset: usize,
        from: u8,
    },
    Delete {
        offset: usize,
        removed: u8,
    },
    Insert {
        offset: usize,
    },
    Transpose {
        offset: usize,
        first: u8,
        second: u8,
    },
}

impl Step {
    /// `base` 是这个音节在纠正后串里的起点；左边已经全是纠正后的字母，右边还是原样，所以下标就是 `base + offset`。
    fn to_edit(self, base: usize) -> Option<Edit> {
        Some(match self {
            Self::Exact => return None,
            Self::Substitute { offset, from } => Edit::Substitute {
                index: base + offset,
                from: from as char,
            },
            Self::Delete { offset, removed } => Edit::Delete {
                index: base + offset,
                removed: removed as char,
            },
            Self::Insert { offset } => Edit::Insert {
                index: base + offset,
            },
            Self::Transpose {
                offset,
                first,
                second,
            } => Edit::Transpose {
                index: base + offset,
                first: first as char,
                second: second as char,
            },
        })
    }
}

/// 从某个位置出发的一个走法：吃掉 `consumed` 个原字母，产出音节 `text`。
#[derive(Debug, Clone)]
struct Move {
    consumed: usize,
    text: String,

    /// `text` 是两个音节时第一个音节的长度（跨音节换位）；一个音节为 0。
    split: usize,
    step: Step,
    cost: f64,
}

#[derive(Debug, Clone)]
struct State {
    out: String,

    /// `out` 里每个音节的长度，最后直接拼成切分，省掉再切一遍。
    lens: Vec<usize>,
    cost: f64,
    edits: Vec<Edit>,
}

/// `input` 的多处编辑纠正，按编辑代价从低到高，最多 [`MAX_CANDIDATES`] 个，每个都是整串可切分的拼音。
/// 原串本身已经能切成合法拼音（含末尾没敲完）时返回空：它不需要修。
///
/// `prior` 给每个音节一个「冷僻度」罚分（≥ 0，常用音节为 0）：代价相同的几条路里，让由常用音节组成的先排进上限里，
/// 而不是被 `ne o`、`na` 这类冷僻拼法按字母序挤掉；它只影响哪些路径进入候选，最后挑哪个仍由整句转换定。
pub fn repair_candidates(
    input: &str,
    costs: &TypoCosts,
    prior: &dyn Fn(&str) -> f64,
) -> Vec<Correction> {
    if !eligible(input) || input.len() < MIN_LETTERS || input.len() > MAX_LETTERS {
        return Vec::new();
    }
    let bytes = input.as_bytes();
    let n = bytes.len();
    let max_edits = n.div_ceil(LETTERS_PER_EDIT);

    let moves: Vec<Vec<Move>> = (0..n).map(|p| moves_at(bytes, p, costs, prior)).collect();
    let terminal: Vec<bool> = (0..=n)
        .map(|p| p == n || parser::is_syllable_prefix(&input[p..]))
        .collect();

    // 从 p 走到结尾的最低代价（含冷僻度）；走不到为无穷。另算一份只数编辑的，用来判断原串是不是本来就合法
    let mut cheapest = vec![f64::INFINITY; n + 1];
    let mut fewest_edits = vec![f64::INFINITY; n + 1];
    for p in (0..=n).rev() {
        if terminal[p] {
            cheapest[p] = 0.0;
            fewest_edits[p] = 0.0;
            continue;
        }
        for m in &moves[p] {
            cheapest[p] = cheapest[p].min(m.cost + cheapest[p + m.consumed]);
            let edit = if matches!(m.step, Step::Exact) {
                0.0
            } else {
                1.0
            };
            fewest_edits[p] = fewest_edits[p].min(edit + fewest_edits[p + m.consumed]);
        }
    }
    if fewest_edits[0] == 0.0 || !cheapest[0].is_finite() {
        return Vec::new();
    }
    let bound = cheapest[0] + SLACK;

    let mut states: Vec<Vec<State>> = vec![Vec::new(); n + 1];
    states[0].push(State {
        out: String::new(),
        lens: Vec::new(),
        cost: 0.0,
        edits: Vec::new(),
    });
    let mut finals: HashMap<String, (State, bool)> = HashMap::new();
    for p in 0..=n {
        let mut here = std::mem::take(&mut states[p]);
        // 同样的纠正后串留代价低的；代价相同留音节少的、再留前面音节长的（与解析器的偏好一致）
        here.sort_by(|a, b| {
            a.cost
                .total_cmp(&b.cost)
                .then_with(|| a.lens.len().cmp(&b.lens.len()))
                .then_with(|| b.lens.cmp(&a.lens))
        });
        let mut seen = std::collections::HashSet::new();
        here.retain(|state| seen.insert(state.out.clone()));
        here.truncate(BEAM);
        for state in &here {
            if terminal[p] && !state.edits.is_empty() {
                let mut text = state.out.clone();
                text.push_str(&input[p..]);
                let mut finished = state.clone();
                let partial = p < n;
                if partial {
                    finished.lens.push(n - p);
                }
                finals
                    .entry(text)
                    .and_modify(|(best, best_partial)| {
                        // 同一个纠正后的串两种切法（`fan` 与 `fa` + 没敲完的 `n`）取完整的那种：它才是解析器会给的切分
                        let better = partial
                            .cmp(best_partial)
                            .then_with(|| finished.cost.total_cmp(&best.cost))
                            .then_with(|| finished.lens.len().cmp(&best.lens.len()))
                            .then_with(|| best.lens.cmp(&finished.lens))
                            .is_lt();
                        if better {
                            *best = finished.clone();
                            *best_partial = partial;
                        }
                    })
                    .or_insert((finished, partial));
            }
            if p == n {
                continue;
            }
            for step in &moves[p] {
                let next = p + step.consumed;
                let cost = state.cost + step.cost;
                if cost + cheapest[next] > bound {
                    continue;
                }
                let mut edits = state.edits.clone();
                if let Some(edit) = step.step.to_edit(state.out.len()) {
                    if edits.len() >= max_edits {
                        continue;
                    }
                    edits.push(edit);
                }
                let mut out = state.out.clone();
                out.push_str(&step.text);
                let mut lens = state.lens.clone();
                if step.split == 0 {
                    lens.push(step.text.len());
                } else {
                    lens.push(step.split);
                    lens.push(step.text.len() - step.split);
                }
                states[next].push(State {
                    out,
                    lens,
                    cost,
                    edits,
                });
            }
        }
    }

    let mut ranked: Vec<(String, State)> = finals
        .into_iter()
        .map(|(text, (state, _))| (text, state))
        .collect();
    ranked.sort_by(|a, b| a.1.cost.total_cmp(&b.1.cost).then_with(|| a.0.cmp(&b.0)));
    // 末尾留个没敲完的残缺音节不花编辑，容易抢在补全之前：同样编辑数的完整读法存在时，残缺尾巴的让路
    //（`chifn` 删掉 f 留个 n，不如补成 chifan）；只在它明显更省（少一处编辑）时才留
    // 末尾单个辅音字母（`n` 可以当「嗯」）整句转换不读它，同样算没敲完
    let has_partial_tail = |text: &str, state: &State| {
        let tail = &text.as_bytes()[text.len() - state.lens.last().copied().unwrap_or(0)..];
        !is_syllable(tail) || (tail.len() == 1 && !b"aeo".contains(&tail[0]))
    };
    let cheapest_complete = ranked
        .iter()
        .filter(|(text, state)| !has_partial_tail(text, state))
        .map(|(_, state)| state.cost)
        .min_by(f64::total_cmp);
    let mut found = Vec::new();
    for (corrected, state) in ranked {
        if corrected == input {
            continue;
        }
        if let Some(complete) = cheapest_complete
            && has_partial_tail(&corrected, &state)
            && state.cost + PARTIAL_TAIL_MARGIN >= complete
        {
            continue;
        }
        let segmentation = segmentation_of(&corrected, &state.lens);
        if segmentation.last_is_partial() && n < MIN_LETTERS_FOR_PARTIAL_TAIL {
            continue;
        }
        let edit = match state.edits.len() {
            1 => state.edits[0].clone(),
            _ => Edit::Multiple { edits: state.edits },
        };
        found.push(Correction {
            original: input.to_owned(),
            corrected,
            edit,
            segmentation,
        });
        if found.len() >= MAX_CANDIDATES {
            break;
        }
    }
    found
}

/// 每个字母在键盘上的相邻字母，按字母序下标（`a` = 0）。启动后算一次。
static NEIGHBORS: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    (b'a'..=b'z')
        .map(|letter| {
            (b'a'..=b'z')
                .filter(|other| typo::adjacent(letter as char, *other as char))
                .collect()
        })
        .collect()
});

fn is_syllable(text: &[u8]) -> bool {
    std::str::from_utf8(text).is_ok_and(parser::is_syllable)
}

/// 按音节长度把纠正后的串拼成切分；最后一个音节没敲完时（不是合法音节）记为残缺。
fn segmentation_of(text: &str, lens: &[usize]) -> Segmentation {
    let mut start = 0;
    let syllables = lens
        .iter()
        .map(|&len| {
            let piece = &text[start..start + len];
            start += len;
            if parser::is_syllable(piece) {
                Syllable::complete(piece)
            } else {
                Syllable::partial(piece)
            }
        })
        .collect();
    Segmentation { syllables }
}

/// 位置 `p` 出发的全部走法：原样音节，加改一个字母后成为音节的各种写法。同一个（吃掉几个、产出什么）只留最便宜的。
/// 候选都在栈上的小缓冲里拼，只有真的是音节才分配。
fn moves_at(bytes: &[u8], p: usize, costs: &TypoCosts, prior: &dyn Fn(&str) -> f64) -> Vec<Move> {
    let n = bytes.len();
    let rest = &bytes[p..];
    let at_end = |consumed: usize| p + consumed == n;
    let transpose_cost = costs.transpose - costs.correction_transpose_discount;
    let mut found: Vec<Move> = Vec::new();
    let mut push = |consumed: usize, text: &[u8], split: usize, step: Step, cost: f64| {
        let text = String::from_utf8_lossy(text).into_owned();
        let rarity = if split == 0 {
            prior(&text)
        } else {
            prior(&text[..split]) + prior(&text[split..])
        };
        let cost = cost + rarity;
        match found
            .iter_mut()
            .find(|m| m.consumed == consumed && m.text == text)
        {
            Some(existing) => {
                // 同样的写法代价相同时留一个音节的那种（与解析器偏好一致）
                if cost < existing.cost || (cost == existing.cost && split == 0) {
                    existing.step = step;
                    existing.cost = cost;
                    existing.split = split;
                }
            }
            None => found.push(Move {
                consumed,
                text,
                split,
                step,
                cost,
            }),
        }
    };
    let mut buffer = [0u8; 2 * parser::MAX_SYLLABLE_LEN];

    for len in 1..=parser::MAX_SYLLABLE_LEN.min(rest.len()) {
        if is_syllable(&rest[..len]) {
            push(len, &rest[..len], 0, Step::Exact, 0.0);
        }
    }
    // 两个音节交界处的两个字母敲反了（`gonyguan` → `gong yuan`）：第一个音节的最后一个字母与第二个音节的第一个字母对调
    for first in 1..=parser::MAX_SYLLABLE_LEN {
        for second in 1..=parser::MAX_SYLLABLE_LEN {
            let total = first + second;
            if rest.len() < total || rest[first - 1] == rest[first] {
                continue;
            }
            buffer[..total].copy_from_slice(&rest[..total]);
            buffer.swap(first - 1, first);
            if is_syllable(&buffer[..first]) && is_syllable(&buffer[first..total]) {
                let step = Step::Transpose {
                    offset: first - 1,
                    first: rest[first - 1],
                    second: rest[first],
                };
                push(total, &buffer[..total], first, step, transpose_cost);
            }
        }
    }
    for len in 2..=parser::MAX_SYLLABLE_LEN {
        if rest.len() >= len {
            let window = &rest[..len];
            buffer[..len].copy_from_slice(window);
            for i in 0..len {
                // 换成旁边的键；刚敲下的最后一个字母不算
                if at_end(len) && i + 1 == len {
                    continue;
                }
                for &letter in &NEIGHBORS[(window[i] - b'a') as usize] {
                    buffer[i] = letter;
                    if is_syllable(&buffer[..len]) {
                        let step = Step::Substitute {
                            offset: i,
                            from: window[i],
                        };
                        push(len, &buffer[..len], 0, step, costs.substitute);
                    }
                }
                buffer[i] = window[i];
            }
            for i in 0..len - 1 {
                if window[i] == window[i + 1] {
                    continue;
                }
                buffer.swap(i, i + 1);
                if is_syllable(&buffer[..len]) {
                    let step = Step::Transpose {
                        offset: i,
                        first: window[i],
                        second: window[i + 1],
                    };
                    push(len, &buffer[..len], 0, step, transpose_cost);
                }
                buffer.swap(i, i + 1);
            }
        }
        // 多敲了一个：吃 len + 1 个，产出 len 个
        if rest.len() > len {
            let window = &rest[..len + 1];
            for i in 0..=len {
                // 删掉的是刚敲下的最后一个字母不算纠正
                if at_end(len + 1) && i == len {
                    continue;
                }
                buffer[..i].copy_from_slice(&window[..i]);
                buffer[i..len].copy_from_slice(&window[i + 1..]);
                if is_syllable(&buffer[..len]) {
                    let step = Step::Delete {
                        offset: i,
                        removed: window[i],
                    };
                    // 多敲的字母多半是连击或手滑碰到邻键；两者都不是时更可能是漏了韵母（`qchi` 是 `qu chi` 而不是 `chi`）
                    let near = |other: Option<&u8>| {
                        other.is_some_and(|&o| {
                            o == window[i] || NEIGHBORS[(window[i] - b'a') as usize].contains(&o)
                        })
                    };
                    let surcharge =
                        if near(i.checked_sub(1).map(|j| &window[j])) || near(window.get(i + 1)) {
                            0.0
                        } else {
                            UNEXPLAINED_EXTRA_SURCHARGE
                        };
                    push(len + 1, &buffer[..len], 0, step, costs.extra + surcharge);
                }
            }
        }
        // 少敲了一个：吃 len - 1 个，产出 len 个
        if len >= 2 && rest.len() >= len - 1 {
            let window = &rest[..len - 1];
            for i in 0..len {
                // 补在最后一个字母之后等于替用户把音节敲完，不算
                if at_end(len - 1) && i == len - 1 {
                    continue;
                }
                for &letter in LIKELY_MISSING {
                    // 只吃一个字母时只认两种漏法：i / u / ü 前面漏了 y / w（`ijing` → `yi jing`），
                    // 声母后面漏了韵母元音（`qchi` → `qu chi`）；别的单字母补全太随意
                    if len == 2
                        && !(i == 0 && matches!(letter, b'y' | b'w'))
                        && !(i == 1 && matches!(letter, b'a' | b'e' | b'i' | b'o' | b'u'))
                    {
                        continue;
                    }
                    buffer[..i].copy_from_slice(&window[..i]);
                    buffer[i] = letter;
                    buffer[i + 1..len].copy_from_slice(&window[i..]);
                    if is_syllable(&buffer[..len]) {
                        push(
                            len - 1,
                            &buffer[..len],
                            0,
                            Step::Insert { offset: i },
                            costs.missing,
                        );
                    }
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corrected(input: &str) -> Vec<String> {
        repair_candidates(input, &TypoCosts::DEFAULT, &|_| 0.0)
            .into_iter()
            .map(|c| c.corrected)
            .collect()
    }

    #[test]
    fn repairs_several_separated_typos_in_a_long_string() {
        let found = corrected("mingtianzaoshngbaciankaihui");
        // `baciankaihui` 本身是合法拼音（ba ci an…），归词图里的音节级敲错边管，这里只修切不动的地方
        assert!(
            found.contains(&"mingtianzaoshangbaciankaihui".to_owned()),
            "{found:?}"
        );
        let found = corrected("zhegewentiwoijingiejueele");
        assert!(
            found.iter().any(|c| c.starts_with("zhegewentiwoyijing")),
            "{found:?}"
        );
    }

    #[test]
    fn repairs_a_transposition_and_a_missing_letter_together() {
        let found = corrected("zhoumowoenyiqiqugonyguanwan");
        assert!(
            found.contains(&"zhoumowoenyiqiqugongyuanwan".to_owned()),
            "{found:?}"
        );
    }

    #[test]
    fn built_segmentations_match_the_parsers_choice() {
        for input in [
            "mingtianzaoshngbaciankaihui",
            "zhoumowoenyiqiqugonyguanwan",
            "wojintinxiangquchashijaidongxi",
        ] {
            for candidate in repair_candidates(input, &TypoCosts::DEFAULT, &|_| 0.0) {
                let parsed = crate::correction::complete_segmentation(&candidate.corrected)
                    .expect("segmentable");
                assert_eq!(
                    candidate.segmentation.syllables.len(),
                    parsed.syllables.len(),
                    "{} {}",
                    candidate.corrected,
                    candidate.segmentation
                );
                assert_eq!(candidate.segmentation.letters(), candidate.corrected.len());
            }
        }
    }

    #[test]
    fn short_strings_with_an_unfinished_tail_are_left_alone() {
        // 英文词 / 缩写不能被「纠」成半截拼音
        for candidate in repair_candidates("kaifaid", &TypoCosts::DEFAULT, &|_| 0.0) {
            assert!(
                !candidate.segmentation.last_is_partial(),
                "{}",
                candidate.corrected
            );
        }
    }

    #[test]
    fn a_string_that_already_segments_needs_no_repair() {
        assert!(corrected("mingtianzaoshangbadiankaihui").is_empty());
        // 末尾还没敲完的音节前缀也算合法
        assert!(corrected("mingtianzaoshangbadiankaih").is_empty());
    }

    #[test]
    fn edits_stay_within_the_budget_and_never_touch_the_last_typed_letter() {
        // 4~6 个字母只许一处编辑
        for candidate in repair_candidates("nihap", &TypoCosts::DEFAULT, &|_| 0.0) {
            assert_eq!(candidate.edit.components().len(), 1);
            assert!(
                !candidate
                    .edit
                    .touches_last_letter(candidate.corrected.len())
            );
        }
        for candidate in
            repair_candidates("zhoumowoenyiqiqugonyguanwan", &TypoCosts::DEFAULT, &|_| 0.0)
        {
            assert!(candidate.edit.components().len() <= 5);
            assert!(
                !candidate
                    .edit
                    .touches_last_letter(candidate.corrected.len())
            );
        }
    }

    #[test]
    fn candidates_are_sorted_by_cost_and_bounded() {
        let all = repair_candidates(
            "wojintinxiangquchashijaidongxi",
            &TypoCosts::DEFAULT,
            &|_| 0.0,
        );
        assert!(!all.is_empty());
        assert!(all.len() <= MAX_CANDIDATES);
        let costs: Vec<usize> = all.iter().map(|c| c.edit.components().len()).collect();
        assert!(costs.windows(2).all(|w| w[0] <= w[1] + 1), "{costs:?}");
    }

    #[test]
    fn composite_edit_indexes_map_back_to_the_original_length() {
        let all = repair_candidates(
            "wojintinxiangquchashijaidongxi",
            &TypoCosts::DEFAULT,
            &|_| 0.0,
        );
        let multi = all
            .iter()
            .find(|c| c.edit.components().len() >= 2)
            .expect("two edits");
        assert_eq!(
            multi.edit.to_original(multi.corrected.len()),
            multi.original.len()
        );
        let all = repair_candidates("mingtianzaoshngbaciankaihui", &TypoCosts::DEFAULT, &|_| 0.0);
        let best = all
            .iter()
            .find(|c| c.corrected == "mingtianzaoshangbaciankaihui")
            .expect("expected correction");
        // 纠正后全串（28 个字母）对应原串全部 27 个字母
        assert_eq!(best.edit.to_original(best.corrected.len()), 27);
        assert_eq!(best.edit.components().len(), 1);
    }
}
