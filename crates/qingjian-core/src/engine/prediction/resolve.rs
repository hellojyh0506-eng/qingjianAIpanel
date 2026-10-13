//! Resolve：弄清用户这一串到底想打什么。
//!
//! 一个请求同时做两件事：判断拼音有没有敲错（错键、颠倒、漏、多、边界），以及结合光标前后文选同音字。
//! 模型只给出「完整拼音 + 选哪个候选」，这里负责全部校验：拼音必须与敲的字母相差有限几处、能完整切分，
//! 纠正后的拼音要重新查一遍本地词库，最终放进候选窗口的文字一定来自本地词库。

use super::*;

/// 请求里最多带多少个本地候选。
const MAX_RESOLVE_CANDIDATES: usize = 12;

/// 纠正拼音后重查本地，`pick` 指向的文字在前几个里才采用它。
const REQUERY_WINDOW: usize = 9;

/// 模型把握够大时放哪儿。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvePlacement {
    /// 放第 1 位并标「纠」，原第 1 位降到第 2 位。
    First,

    /// 没有上下文的同音词调整：不动第 1 位，只在前三位内把它往前提一格。
    WithinThree,

    /// 把握不够（这一串连续被拒绝过纠正）：不改候选顺序，只留作提示位，Tab 采用。
    Hint,
}

/// 通过校验的 Resolve 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// 要放进候选窗口的候选，来自本地词库；放第 1 位时已标好 `corrected`。
    pub candidate: Candidate,

    /// 模型认为用户敲错了拼音（返回的拼音与敲的字母不同）。
    pub pinyin_changed: bool,

    /// 放哪儿。
    pub placement: ResolvePlacement,
}

/// 最近一次 Resolve 请求的现场：回复回来时据此校验，请求之后用户继续打字了就对不上。
#[derive(Debug, Clone)]
pub(in crate::engine) struct ResolveContext {
    sequence: u64,
    scope: String,
    candidates: Vec<Candidate>,
    has_context: bool,
}

/// 编辑距离上限随拼音长度放宽：短串只容许 2 处，长串容许更多处。
fn edit_limit(len: usize) -> usize {
    match len {
        0..=9 => 2,
        10..=15 => 3,
        16..=39 => 4,
        _ => 5,
    }
}

fn is_chinese(candidate: &Candidate) -> bool {
    matches!(
        candidate.kind,
        CandidateKind::Chinese | CandidateKind::Sentence
    )
}

impl Engine {
    /// 请求 AI 弄清当前这一串想打什么。`candidates` 是壳眼下摆出来的本地候选（含本地纠正出的那条）。
    ///
    /// 英文、双拼、注音、形码、问字、表达、私密输入不发；单字且没有上下文不发；不足 4 个字母又不足 2 个音节的短串只有带上下文才发；
    /// 没有上下文、没有纠正候选、本地首选又正好读得出整串的（本地把握很大）也不发。
    pub fn request_resolve(
        &mut self,
        surrounding: Option<SurroundingText>,
        candidates: &[Candidate],
    ) -> Option<u64> {
        self.bump_lane(PredictionLane::Typing);
        self.resolve_context = None;
        if !self.predictor.is_enabled()
            || self.private
            || self.english_mode
            || self.shuangpin.is_some()
            || self.zhuyin
            || self.code.is_some()
            || self.aux_filter().is_some()
        {
            return None;
        }
        let scope = self.composition.scope();
        if !(2..=64).contains(&scope.len()) || !scope.bytes().all(|byte| byte.is_ascii_lowercase())
        {
            return None;
        }
        if self.modes().is_expression(scope, self.zhuyin)
            || self.modes().is_question(scope, self.zhuyin)
            || is_raw(scope, self.modes(), self.shuangpin, self.zhuyin)
        {
            return None;
        }
        let letters = scope.to_owned();
        let policy = self.predictor.policy();
        let (before, after) = match surrounding {
            Some(text) => (
                take_last_chars(&text.before, policy.before),
                take_first_chars(&text.after, policy.after),
            ),
            None => (String::new(), String::new()),
        };
        let has_context = !before.trim().is_empty() || !after.trim().is_empty();
        let (pinyin, syllables, guess) = match segment_longest_prefix(&letters) {
            Ok((segmentations, tail)) => (
                query::join_marked(&segmentations, tail),
                segmentations.first().map_or(0, |seg| seg.syllables.len()),
                self.local_guess(&segmentations),
            ),
            Err(_) => (letters.clone(), 0, String::new()),
        };
        // 短串（不到 4 个字母、不足 2 个音节）只有带上下文才值得问：`ta` 在「我妹妹今年上大学了，」后面是 她
        if letters.len() < 4 && syllables < 2 && !has_context {
            return None;
        }
        if syllables <= 1 && !has_context {
            return None;
        }

        let mut local: Vec<Candidate> = Vec::new();
        for candidate in candidates.iter().filter(|candidate| is_chinese(candidate)) {
            if candidate.text.trim().is_empty()
                || local.iter().any(|seen| seen.text == candidate.text)
            {
                continue;
            }
            if local.len() >= MAX_RESOLVE_CANDIDATES {
                break;
            }
            local.push(candidate.clone());
        }
        if local.is_empty() {
            return None;
        }
        let confident = !has_context
            && !local.iter().any(|candidate| candidate.corrected)
            && local[0].syllables.concat() == letters;
        if confident {
            return None;
        }

        let resolve_candidates = local
            .iter()
            .map(|candidate| ResolveCandidate {
                text: candidate.text.clone(),
                reading: candidate.syllables.join("'"),
                corrected: candidate.corrected,
            })
            .collect();
        let request = PredictionRequest {
            sequence: self.prediction_sequence,
            kind: PredictionKind::Resolve,
            before,
            after,
            pinyin,
            letters: letters.clone(),
            syllables,
            candidates: local
                .iter()
                .map(|candidate| candidate.text.clone())
                .collect(),
            resolve_candidates,
            guess,
            current_expression: local[0].text.clone(),
            ..PredictionRequest::default()
        };
        self.resolve_context = Some(ResolveContext {
            sequence: self.prediction_sequence,
            scope: letters.clone(),
            candidates: local,
            has_context,
        });
        self.last_prediction_scope = letters;
        self.note_request(PredictionKind::Resolve);
        self.predictor.submit(request);
        Some(self.prediction_sequence)
    }

    /// 按给定的完整拼音查一次本地候选，不动当前输入。壳与模型都不能绕过它：纠正后的文字必须是词库认得的。
    pub fn query_reading(&mut self, reading: &str) -> Vec<Candidate> {
        if reading.is_empty() || !reading.bytes().all(|byte| byte.is_ascii_lowercase()) {
            return Vec::new();
        }
        let saved = std::mem::take(&mut self.composition);
        for ch in reading.chars() {
            self.composition.push(ch);
        }
        let result = self.query_inner();
        self.composition = saved;
        let mut items = result
            .map(|query| query.candidates.items)
            .unwrap_or_default();
        if self.traditional
            && let Some(opencc) = &self.opencc
        {
            for candidate in items.iter_mut().filter(|candidate| is_chinese(candidate)) {
                let traditional_text = opencc.convert(&candidate.text);
                self.traditional_map
                    .borrow_mut()
                    .insert(traditional_text.clone(), candidate.text.clone());
                candidate.text = traditional_text;
            }
        }
        items
    }

    /// 用纠正后的拼音重查本地：编辑距离在上限内、能完整切分，且重查有整串同读音的中文候选才算数。
    /// `pick` 出现在前 [`REQUERY_WINDOW`] 个里就用它，否则用第 1 个。
    fn requery_for_resolve(
        &mut self,
        letters: &str,
        pinyin: &str,
        pick: Option<&str>,
    ) -> Option<Candidate> {
        if !within_edit_distance(
            letters.as_bytes(),
            pinyin.as_bytes(),
            edit_limit(letters.len()),
        ) {
            return None;
        }
        let segmentation = correction::complete_segmentation(pinyin)?;
        let rebuilt: String = segmentation
            .syllables
            .iter()
            .map(|syllable| syllable.text.as_str())
            .collect();
        if rebuilt != pinyin {
            return None;
        }
        let fresh: Vec<Candidate> = self
            .query_reading(pinyin)
            .into_iter()
            .filter(|candidate| {
                is_chinese(candidate)
                    && !candidate.syllables.is_empty()
                    && candidate.syllables.concat() == pinyin
            })
            .collect();
        pick.and_then(|text| fresh.iter().take(REQUERY_WINDOW).find(|c| c.text == text))
            .or_else(|| fresh.first())
            .cloned()
    }

    /// 校验模型给的 Resolve 回复，通过才给出能放进候选窗口的那一条。
    ///
    /// `pinyin` 是模型认为的完整拼音，`pick` 是它选中的本地候选文字（没有合适的为 `None`）。
    /// `pick` 是请求里给过的、带读音的候选时，先以它的读音为准（模型常抄错拼音、却选对候选）；
    /// 它的读音重查不出整句（本地候选本身就是错键拼出来的），或者没有 `pick`，再用模型给的拼音。
    /// - 拼音与敲的字母相同：只是选字，`pick` 必须是请求里给过的候选。
    /// - 拼音不同：编辑距离在上限内、能完整切分，用纠正后的拼音重查本地；`pick` 在新结果前 9 个里就用它，
    ///   否则用新结果第 1 个。
    ///
    /// 请求之后输入已经变了（作用域不同、序号对不上）一律丢弃。
    pub(super) fn resolve_reply(
        &mut self,
        sequence: u64,
        pinyin: Option<&str>,
        pick: Option<&str>,
    ) -> Option<Resolved> {
        let context = self
            .resolve_context
            .as_ref()
            .filter(|context| context.sequence == sequence)?
            .clone();
        if self.composition.scope() != context.scope {
            return None;
        }
        let letters = context.scope.as_str();
        let pick = pick.map(str::trim).filter(|text| !text.is_empty());
        let model_pinyin = pinyin
            .map(|pinyin| pinyin.trim().to_ascii_lowercase())
            .filter(|pinyin| {
                !pinyin.is_empty() && pinyin.bytes().all(|byte| byte.is_ascii_lowercase())
            });

        // 模型选中了请求里的某个候选：以它自己的读音为准。小模型常常选对了候选、却把拼音抄错一两个字母，
        // 候选的读音来自本地词库，拿它当「用户本意的拼音」更可靠；读音为空（没有音节）的候选才退回模型给的拼音。
        let picked = pick.and_then(|text| {
            context
                .candidates
                .iter()
                .find(|candidate| candidate.text == text && !candidate.syllables.is_empty())
        });
        let picked_reading = picked.map(|picked| picked.syllables.concat());

        // 先试所选候选自己的读音；它重查不出像样的整句（读音本身就是错键拼出来的），再试模型给的拼音。
        let mut readings: Vec<String> = Vec::new();
        readings.extend(picked_reading.clone());
        if let Some(model_pinyin) = model_pinyin
            && !readings.contains(&model_pinyin)
        {
            readings.push(model_pinyin);
        }

        let mut resolved: Option<(Candidate, bool, String)> = None;
        for pinyin in readings {
            if pinyin == letters {
                if let Some(picked) = picked {
                    resolved = Some((picked.clone(), false, pinyin));
                    break;
                }
                continue;
            }
            if let Some(candidate) = self.requery_for_resolve(letters, &pinyin, pick) {
                resolved = Some((candidate, true, pinyin));
                break;
            }
        }
        let (mut candidate, pinyin_changed, pinyin) = resolved?;

        let placement = if self.correction_demoted(letters) {
            ResolvePlacement::Hint
        } else if pinyin_changed || context.has_context {
            ResolvePlacement::First
        } else {
            ResolvePlacement::WithinThree
        };
        if pinyin_changed {
            self.ai_resolved = Some((context.scope.clone(), pinyin));
        }
        if placement == ResolvePlacement::First {
            candidate.corrected = true;
            // 记成「摆出来的纠正」：用户随后选了别的候选，会按拒绝计数，连续被拒绝就不再放第 1 位
            if let Ok(mut snapshot) = self.last_query.try_borrow_mut()
                && let Some(snapshot) = snapshot.as_mut()
                && snapshot.scope == context.scope
            {
                snapshot.offered = Some(candidate.text.clone());
            }
        }
        Some(Resolved {
            candidate,
            pinyin_changed,
            placement,
        })
    }

    /// 选了 AI 纠正出来的候选：它的读音和敲的字母对不上，上屏要吃掉整段作用域，
    /// 否则按读音逐个对键会在第一处对不上的地方停下，把后半截字母留在缓冲区里。
    pub(in crate::engine) fn ai_resolved_consumption(
        &self,
        candidate: &Candidate,
    ) -> Option<usize> {
        let (scope, reading) = self.ai_resolved.as_ref()?;
        (candidate.corrected
            && scope == self.composition.scope()
            && candidate.syllables.concat() == *reading)
            .then_some(scope.len())
    }
}

#[cfg(test)]
mod tests {
    use super::edit_limit;

    #[test]
    fn edit_limit_grows_with_length() {
        assert_eq!(edit_limit(4), 2);
        assert_eq!(edit_limit(9), 2);
        assert_eq!(edit_limit(10), 3);
        assert_eq!(edit_limit(15), 3);
        assert_eq!(edit_limit(16), 4);
        assert_eq!(edit_limit(39), 4);
        assert_eq!(edit_limit(40), 5);
    }
}
