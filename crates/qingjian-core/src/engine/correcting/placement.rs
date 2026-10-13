//! 纠正候选放第几位：本地纠错的放置规则都在这里判断。
//!
//! 整串替换的纠正（原串根本不像拼音，[`correction::unlikely_pinyin`]）没有「原串候选」可比，纠正后的候选就是第一位，不走这里。
//! 走这里的是词图里靠敲错边读出来的整句：原串本身读得通，纠正候选与原串候选同时在列表里，要决定它排第 1 还是第 2。

use super::*;

/// 纠正候选最靠前能放到哪一位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::engine) enum Slot {
    First,
    Second,
}

/// 同一个 (敲的, 要的) 音节对被接受几次以上算「高把握」：原串即使合法也放第 1 位。
const TRUSTED_ACCEPTANCES: u32 = 2;

/// 同一个串连续被拒绝纠正几次以后，不再自动把纠正放第 1 位。
const REJECTIONS_TO_DEMOTE: u32 = 2;

impl Engine {
    /// `scope` 下纠正候选的位置。`original_has_exact` 是原串有没有覆盖整串的精确候选（词库里正好有这个读音的词），
    /// `pairs` 是纠正候选里每处敲错的 (敲的音节, 要的音节)。
    ///
    /// - 连续拒绝过 [`REJECTIONS_TO_DEMOTE`] 次：最多第 2 位。
    /// - 原串没有覆盖整串的精确候选：第 1 位。
    /// - 原串有精确候选：最多第 2 位，除非每处敲错都在个人敲错表里被接受过 [`TRUSTED_ACCEPTANCES`] 次以上。
    pub(in crate::engine) fn correction_slot(
        &self,
        scope: &str,
        original_has_exact: bool,
        pairs: &[(String, String)],
    ) -> Slot {
        if self.learner.rejected_correction_count(scope) >= REJECTIONS_TO_DEMOTE {
            return Slot::Second;
        }
        if !original_has_exact {
            return Slot::First;
        }
        let trusted = !pairs.is_empty()
            && pairs.iter().all(|(typed, intended)| {
                self.learner.typo_count(typed, intended) >= TRUSTED_ACCEPTANCES
            });
        if trusted { Slot::First } else { Slot::Second }
    }

    /// 这一串是不是已经连续被拒绝过纠正：是的话 AI 的纠正也只留作提示，不再放第 1 位。
    pub(in crate::engine) fn correction_demoted(&self, scope: &str) -> bool {
        self.learner.rejected_correction_count(scope) >= REJECTIONS_TO_DEMOTE
    }

    /// 上屏时对账本地摆出来的纠正：用户选了纠正候选就清掉拒绝记录；
    /// 纠正候选在前两位而用户把整串上屏成了别的候选（选回原串），记一次拒绝。
    /// `consumed` 是这次上屏吃掉的原串字母数，只吃了一部分的不算表态。
    pub(in crate::engine) fn settle_correction_offer(
        &mut self,
        candidate: &Candidate,
        consumed: usize,
    ) {
        let Some(snapshot) = self.last_query.borrow().clone() else {
            return;
        };
        let Some(offered) = snapshot.offered else {
            return;
        };
        let scope = self.composition.scope().to_owned();
        if snapshot.scope != scope {
            return;
        }
        if candidate.corrected || candidate.text == offered {
            self.learner.clear_rejected_correction(&scope);
        } else if consumed >= scope.len()
            && matches!(
                candidate.kind,
                CandidateKind::Chinese | CandidateKind::Sentence
            )
        {
            self.learner.record_rejected_correction(&scope);
        }
    }
}
