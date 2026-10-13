//! 组句的展示状态：缓冲变化时重查候选并重建 [`Composed`]，云端词异步并入，高亮 / 翻页，按状态生成给 DLL 的帧。

mod state;

use qingjian_core::{
    Candidate, CandidateLayout, CandidateList, CloudWord,
    ErrorMajorCategory, ErrorSubcategory, FeedbackItem, Language, PredictionKind, Query,
    Sense, Translation,
};
use qingjian_platform::protocol::{Frame, PROTOCOL_VERSION, PreeditKind, PreeditSegment};

use std::time::Instant;

pub(super) use self::state::{Composed, TypedKeys};
use super::Router;

impl Router {
    /// 缓冲变化后：按 Engine 状态重建 [`Composed`]，发一次云联想请求，归零高亮与整句补全。
    pub(super) fn recompose(&mut self) {
        self.highlight = 0;
        self.navigated = false;
        self.sentence = None;
        self.passive_translation = None;
        self.passive_translation_target = None;
        self.passive_learning_card = None;
        self.recall_session = None;
        if self.engine.composition().is_empty() {
            self.surrounding_before = None;
            self.composed = None;
            self.cancel_prediction();
            self.stop_rescoring();
            return;
        }
        self.attach_loaded_model();
        let typed_scope = self.engine.composition().scope().to_owned();
        let query = self.engine.query().ok();
        let intent_suspected = !self.engine.english_mode()
            && !self.engine.raw_mode()
            && !self.engine.question_mode()
            && !self.engine.expression_mode()
            && !self.is_private()
            && should_request_intent(query.as_ref(), &typed_scope);
        let intent_candidate_hints = query
            .as_ref()
            .map(|query| query.candidates.items.clone())
            .unwrap_or_default();
        let built = query.map(|query| {
            let (preedit, cursor, typed_keys) = marked_parts(&query);
            (query.candidates.items.clone(), preedit, cursor, typed_keys)
        });
        self.composed = Some(match built {
            Some((items, preedit, cursor, typed_keys)) => {
                let mut layout =
                    CandidateLayout::new(items, self.config.page_size, self.config.cloud_slots);
                // For an obvious one-key typo, promote the best locally verified full-pinyin
                // candidate immediately; the cloud request below may refine the homophone later.
                if intent_suspected {
                    let local = layout.local().to_vec();
                    if let Some(candidate_text) = self.engine.fast_local_intent_candidate(&local) {
                        let mut order = vec![candidate_text.clone()];
                        for candidate in &local {
                            if matches!(candidate.kind, qingjian_core::CandidateKind::Chinese | qingjian_core::CandidateKind::Sentence)
                                && !order.contains(&candidate.text)
                            {
                                order.push(candidate.text.clone());
                            }
                        }
                        layout.reorder_local_by_text(&order);
                        self.engine.note_candidate_order(&order);
                        self.sentence = Some(candidate_text);
                    }
                }
                let learning_on = !self.config.learning_language_off
                    && self.engine.learning_language() != Language::Chinese
                    && !self.is_private()
                    && !self.engine.question_mode()
                    && !self.engine.expression_mode()
                    && !self.engine.raw_mode();
                let first_target = layout.candidate(0).map(|c| c.text.clone());
                if learning_on {
                    if let Some(target) = first_target {
                        let target_changed = self.passive_translation_target.as_ref() != Some(&target);
                        if target_changed {
                            self.passive_translation = None;
                            self.generation_id = self.generation_id.wrapping_add(1);
                        }
                        self.passive_translation_target = Some(target.clone());
                        if self.engine.prediction_enabled() {
                            let due = self.today_due_count();
                            let offline = self.engine
                                .translate_word(&target)
                                .and_then(|t| t.senses().first().map(|s| s.text.clone()))
                                .filter(|s| is_valid_english_text(s))
                                .or_else(|| fallback_idiom_english(&target));

                            let (initial_state, initial_primary) = if let Some(primary_en) = offline {
                                self.passive_translation = Some(primary_en.clone());
                                (
                                    qingjian_platform::protocol::LearningState::Ready,
                                    Some(qingjian_platform::protocol::LearningExpression {
                                        text: primary_en,
                                        tag: "Common".to_string(),
                                        meaning: target.clone(),
                                        tone: "标准自然".to_string(),
                                        scenario: "日常场景".to_string(),
                                        commonality: "高频".to_string(),
                                        why_natural: "本地离线词库释义".to_string(),
                                    }),
                                )
                            } else {
                                self.passive_translation = None;
                                (qingjian_platform::protocol::LearningState::Loading, None)
                            };

                            self.passive_learning_card = Some(qingjian_platform::protocol::LearningCardFrame {
                                target_text: target.clone(),
                                state: initial_state,
                                primary: initial_primary,
                                alternatives: Vec::new(),
                                expanded: false,
                                selected_index: None,
                                drill_stage: None,
                                drill_input: String::new(),
                                drill_attempt: None,
                                drill_notice: None,
                                today_due_count: due,
                                review_current_index: 0,
                                review_total_count: due,
                                upgrade: None,
                            });
                            self.translation_start_time = None;
                            if !intent_suspected {
                                // 重排在打字通道、翻译在学习通道，两路并行发出，互不等待；
                                // 重排若改了首选，结果回来时会按新首选重发翻译。
                                // A likely typo uses one Intent request instead of this pair.
                                let surrounding = self.rerank_surrounding();
                                let _ = self.engine.request_rerank(surrounding, layout.local());
                                self.translation_start_time = Some(Instant::now());
                                if let Some(sequence) = self.engine.request_translation(&target) {
                                    self.generation_id = sequence;
                                } else {
                                    self.translation_start_time = None;
                                }
                            }
                        }
                    } else if self.engine.prediction_enabled() && !intent_suspected {
                        let surrounding = self.rerank_surrounding();
                        self.engine.request_rerank(surrounding, layout.local());
                    }
                } else if self.engine.prediction_enabled() && !intent_suspected {
                    let surrounding = self.rerank_surrounding();
                    self.engine.request_rerank(surrounding, layout.local());
                }
                Composed::Candidates {
                    preedit,
                    cursor,
                    typed_keys,
                    layout,
                }
            }
            None => {
                self.cancel_prediction();
                self.passive_learning_card = None;
                self.passive_translation_target = None;
                self.passive_translation = None;
                let composition = self.engine.composition();
                let text = composition.text().to_owned();
                let cursor = text[..composition.cursor()].chars().count();
                Composed::Raw { text, cursor }
            }
        });
        if intent_suspected && self.engine.prediction_enabled() {
            let surrounding = self.rerank_surrounding();
            self.engine.request_intent_suggestion(surrounding, &intent_candidate_hints);
        }
        self.schedule_rescoring();
    }

    /// 拉一次云联想结果：云端词并进候选布局，整句补全记下；翻译评审时结果是译文。
    pub(super) fn poll_prediction(&mut self) {
        if !self.engine.prediction_enabled() {
            return;
        }

        // 超时防冻结守护：
        // 1. 被动翻译弹性守护：
        //    - 1500ms 后若本地离线词典有收录，先呈现离线释义消除等待空白，但不打断可能稍后到达的丰富云端译文；
        //    - 若本地离线词典无收录，保留充足云端等待时间（最高 8000ms），绝不因 1.5s 匆忙断定失败。
        if let Some(start) = self.translation_start_time {
            let elapsed = start.elapsed().as_millis();
            if let Some(card) = self.passive_learning_card.as_mut() {
                if card.state == qingjian_platform::protocol::LearningState::Loading {
                    if elapsed > 1500 {
                        let offline = self.passive_translation_target.as_deref().and_then(|target| {
                            self.engine.translate_word(target).and_then(|t| t.senses().first().map(|s| (s.text.clone(), target.to_string())))
                        });
                        if let Some((offline_text, target)) = offline {
                            tracing::info!(target = %target, "被动翻译暂时等待中，优先展示本地离线词典释义");
                            card.primary = Some(qingjian_platform::protocol::LearningExpression {
                                text: offline_text.clone(),
                                tag: "Offline".to_string(),
                                meaning: target,
                                tone: "Standard".to_string(),
                                scenario: "General".to_string(),
                                commonality: "Common".to_string(),
                                why_natural: "Offline glossary fallback".to_string(),
                            });
                            card.state = qingjian_platform::protocol::LearningState::Ready;
                            self.passive_translation = Some(offline_text);
                            let shown = self.self_drawn_frame();
                            self.reconcile_candidates(&shown);
                        } else if elapsed > 8000 {
                            tracing::warn!(target = ?self.passive_translation_target, "被动翻译请求超时 (>8000ms)，置为Failed");
                            card.state = qingjian_platform::protocol::LearningState::Failed;
                            self.translation_start_time = None;
                            let shown = self.self_drawn_frame();
                            self.reconcile_candidates(&shown);
                        }
                    }
                }
            }
        }
        // 2. 主动回忆对比超时保护 (>4000ms): 回退为 Input 状态并给用户明确提示
        if let Some(start) = self.compare_start_time {
            if start.elapsed().as_millis() > 4000 {
                if let Some(card) = self.passive_learning_card.as_mut() {
                    if card.drill_stage == Some(qingjian_platform::protocol::DrillStage::Comparing) {
                        tracing::warn!("主动回忆对比请求超时 (>4000ms)，回退为输入状态");
                        card.drill_stage = Some(qingjian_platform::protocol::DrillStage::Input);
                        card.drill_notice = Some("对比分析响应超时，请重试".to_string());
                        self.compare_start_time = None;
                        let shown = self.self_drawn_frame();
                        self.reconcile_candidates(&shown);
                    }
                }
            }
        }

        // 2. 选区翻译超时守护 (>8000ms)
        if self.translation.is_some() {
            if let Some(start) = self.translation_start_time {
                if start.elapsed().as_millis() > 8000 {
                    tracing::warn!("选区翻译请求超时 (>8000ms)，触发离线兜底");
                    let offline = self.last_translation_source.as_deref().and_then(|src| {
                        self.engine.translate_word(src).and_then(|t| t.senses().first().map(|s| s.text.clone()))
                    });
                    if let Some(translation) = self.translation.as_mut() {
                        translation.result = offline.or_else(|| Some("翻译响应超时，请重试".to_string()));
                    }
                    self.translation_start_time = None;
                    let shown = self.self_drawn_frame();
                    self.reconcile_candidates(&shown);
                    return;
                }
            }
        }

        let Some(prediction) = self.engine.poll_prediction() else {
            return;
        };
        if self.translation.is_some() {
            self.translation_start_time = None;
            match prediction.sentence {
                Some(text) => {
                    if let Some(translation) = self.translation.as_mut() {
                        translation.result = Some(text);
                    }
                }
                None => {
                    tracing::info!("翻译选中文字：云端没有给出译文，尝试本地离线词典兜底");
                    let offline = self.last_translation_source.as_deref().and_then(|src| {
                        self.engine.translate_word(src).and_then(|t| t.senses().first().map(|s| s.text.clone()))
                    });
                    if let Some(translation) = self.translation.as_mut() {
                        translation.result = offline.or_else(|| Some("未能获取译文，请重试".to_string()));
                    }
                }
            }
            let shown = self.self_drawn_frame();
            self.reconcile_candidates(&shown);
            return;
        }
        if prediction.kind == PredictionKind::Intent {
            // Intent is asynchronous: apply it only while the original composition is still active.
            // A local one-edit guess may already be visible; the model may confirm or refine it.
            if !self.navigated && self.highlight == 0 {
                if let Some(candidate_text) = prediction.sentence {
                    let local_candidate_exists = self.composed.as_ref().is_some_and(|composed| {
                        matches!(composed, Composed::Candidates { layout, .. }
                            if layout.local().iter().any(|candidate| candidate.text == candidate_text))
                    });
                    if local_candidate_exists {
                        let order_to_note = if let Some(Composed::Candidates { layout, .. }) = self.composed.as_mut() {
                            let local = layout.local().to_vec();
                            let mut order = vec![candidate_text.clone()];
                            for candidate in &local {
                                if matches!(candidate.kind, qingjian_core::CandidateKind::Chinese | qingjian_core::CandidateKind::Sentence)
                                    && !order.contains(&candidate.text)
                                {
                                    order.push(candidate.text.clone());
                                }
                            }
                            layout.reorder_local_by_text(&order).then_some(order)
                        } else {
                            None
                        };
                        if let Some(order) = order_to_note.as_deref() {
                            self.engine.note_candidate_order(order);
                        }
                        self.sentence = Some(candidate_text.clone());

                        // If AI refines the fast local guess, update the learning gloss without
                        // delaying or blocking the corrected candidate itself.
                        if self.passive_learning_card.is_some()
                            && self.passive_translation_target.as_deref() != Some(candidate_text.as_str())
                        {
                            let offline = self.engine
                                .translate_word(&candidate_text)
                                .and_then(|translation| translation.senses().first().map(|sense| sense.text.clone()))
                                .filter(|text| is_valid_english_text(text))
                                .or_else(|| fallback_idiom_english(&candidate_text));
                            self.passive_translation = offline.clone();
                            self.passive_translation_target = Some(candidate_text.clone());
                            if let Some(card) = self.passive_learning_card.as_mut() {
                                card.target_text = candidate_text.clone();
                                card.primary = offline.as_ref().map(|text| qingjian_platform::protocol::LearningExpression {
                                    text: text.clone(),
                                    tag: "Common".to_string(),
                                    meaning: candidate_text.clone(),
                                    tone: "标准自然".to_string(),
                                    scenario: "日常场景".to_string(),
                                    commonality: "高频".to_string(),
                                    why_natural: "本地离线词库释义".to_string(),
                                });
                                card.alternatives.clear();
                                card.state = if card.primary.is_some() {
                                    qingjian_platform::protocol::LearningState::Ready
                                } else {
                                    qingjian_platform::protocol::LearningState::Loading
                                };
                                card.upgrade = None;
                            }
                            self.translation_start_time = None;
                            if offline.is_none() && self.engine.prediction_enabled() {
                                self.translation_start_time = Some(Instant::now());
                                if let Some(sequence) = self.engine.request_translation(&candidate_text) {
                                    self.generation_id = sequence;
                                } else {
                                    self.translation_start_time = None;
                                }
                            }
                        }
                    }
                } else if self.passive_learning_card.is_some()
                    && self.translation_start_time.is_none()
                    && self.engine.prediction_enabled()
                {
                    // If Intent returns no locally valid suggestion, keep the fast local hint (if any)
                    // and restore the passive translation path instead of leaving its card Loading.
                    if let Some(target) = self.passive_translation_target.clone() {
                        self.translation_start_time = Some(Instant::now());
                        if let Some(sequence) = self.engine.request_translation(&target) {
                            self.generation_id = sequence;
                        } else {
                            self.translation_start_time = None;
                        }
                    }
                }
                let shown = self.self_drawn_frame();
                self.reconcile_candidates(&shown);
            }
            return;
        }
        if prediction.kind == PredictionKind::Rerank {
            // Rerank 结果只能重排当前状态下的原有本地候选，绝不作为云词插入。
            // 已移动高亮/翻页就不再应用，避免 AI 回包抢走用户正在操作的候选。
            if !self.navigated && self.highlight == 0 {
                let mut logged_order: Option<Vec<String>> = None;
                if let Some(Composed::Candidates { layout, .. }) = self.composed.as_mut() {
                    let order: Vec<String> = prediction.words.iter().map(|word| word.text.clone()).collect();
                    if layout.reorder_local_by_text(&order) {
                        logged_order = Some(layout.local().iter().map(|candidate| candidate.text.clone()).collect());
                    }
                }
                if let Some(order) = logged_order.as_deref() {
                    self.engine.note_candidate_order(order);
                }

                // 重排结束后才启动被动翻译，确保共享 Worker 不会让翻译请求挤掉本轮重排。
                let target = match self.composed.as_ref() {
                    Some(Composed::Candidates { layout, .. }) => layout.candidate(0).map(|candidate| candidate.text.clone()),
                    _ => None,
                };
                if let Some(target) = target.filter(|_| self.passive_learning_card.is_some()) {
                    let changed_target = self.passive_translation_target.as_ref() != Some(&target);
                    if changed_target {
                        let offline = self.engine
                            .translate_word(&target)
                            .and_then(|translation| translation.senses().first().map(|sense| sense.text.clone()))
                            .filter(|text| is_valid_english_text(text))
                            .or_else(|| fallback_idiom_english(&target));
                        self.passive_translation = offline.clone();
                        self.passive_translation_target = Some(target.clone());
                        if let Some(card) = self.passive_learning_card.as_mut() {
                            card.target_text = target.clone();
                            card.primary = offline.map(|text| qingjian_platform::protocol::LearningExpression {
                                text,
                                tag: "Common".to_string(),
                                meaning: target.clone(),
                                tone: "标准自然".to_string(),
                                scenario: "日常场景".to_string(),
                                commonality: "高频".to_string(),
                                why_natural: "本地离线词库释义".to_string(),
                            });
                            card.alternatives.clear();
                            card.state = if card.primary.is_some() {
                                qingjian_platform::protocol::LearningState::Ready
                            } else {
                                qingjian_platform::protocol::LearningState::Loading
                            };
                        }
                    }
                    // 翻译早已与重排并行发出；只有重排改了首选才需要对新首选重发。
                    if changed_target {
                        self.translation_start_time = Some(Instant::now());
                        if let Some(sequence) = self.engine.request_translation(&target) {
                            self.generation_id = sequence;
                        } else {
                            self.translation_start_time = None;
                        }
                    }
                }
            }
            let shown = self.self_drawn_frame();
            self.reconcile_candidates(&shown);
            return;
        }

        if prediction.kind == PredictionKind::Compare {
            let compare_total_ms = self
                .compare_start_time
                .take()
                .map(|t| t.elapsed().as_millis())
                .unwrap_or(0);

            let (nat_expr, status_str, advice, feedback_items) = if let Some(first) = prediction.alternatives.first() {
                let advice = prediction.sentence.clone().unwrap_or_else(|| {
                    if !first.why_natural.is_empty() {
                        first.why_natural.clone()
                    } else {
                        "表达地道，用词准确。".to_string()
                    }
                });
                let mut items = Vec::new();
                for alt in prediction.alternatives.iter().skip(1) {
                    let major = match alt.tag.as_str() {
                        "语法" | "Grammar" => ErrorMajorCategory::Grammar,
                        "用词" | "Vocabulary" => ErrorMajorCategory::Vocabulary,
                        "搭配" | "Collocation" => ErrorMajorCategory::Collocation,
                        "自然度" | "Naturalness" => ErrorMajorCategory::Naturalness,
                        "短语习语" | "PhrasalIdiomatic" => ErrorMajorCategory::PhrasalIdiomatic,
                        "语用语域" | "Pragmatics" => ErrorMajorCategory::Pragmatics,
                        _ => ErrorMajorCategory::Naturalness,
                    };
                    items.push(FeedbackItem {
                        major_category: major,
                        subcategory: ErrorSubcategory::Other(alt.meaning.clone()),
                        label: alt.tag.clone(),
                        explanation: alt.why_natural.clone(),
                        suggestion: None,
                    });
                }
                (first.text.clone(), first.tag.clone(), advice, items)
            } else {
                let advice = prediction.sentence.unwrap_or_else(|| "表达地道，用词准确。".to_string());
                ("That makes perfect sense.".to_string(), "NaturalBetter".to_string(), advice, Vec::new())
            };

            let is_good = status_str == "ExactMatch" || status_str == "ValidB2Upgrade";
            let drill_feedback: Vec<crate::ui::drill::DrillFeedbackItem> = feedback_items.iter().map(|item| {
                crate::ui::drill::DrillFeedbackItem {
                    label: item.label.clone(),
                    explanation: item.explanation.clone(),
                }
            }).collect();

            let drill_res = crate::ui::drill::DrillResultData {
                verdict: if is_good { "Perfect".to_string() } else { "Needs Work".to_string() },
                main_issue: advice.clone(),
                better_expression: nat_expr.clone(),
                feedback_items: drill_feedback,
                overall_rating: if is_good { "A (地道口语)".to_string() } else { "B (需微调)".to_string() },
                ielts_suggestion: Some(advice.clone()),
            };
            self.candidates.show_drill_result(drill_res);

            tracing::info!(
                compare_total_ms,
                status = %status_str,
                "Compare 批改结果已下发至独立练习窗口"
            );
            return;
        }
        if prediction.kind == PredictionKind::DeepLearn {
            tracing::info!(
                seq = prediction.sequence,
                alts = prediction.alternatives.len(),
                "收到 DeepLearn 深度解析数据，回填学习助手"
            );
            let rect = self.last_rect.unwrap_or(qingjian_platform::protocol::ScreenRect {
                left: 200,
                top: 200,
                right: 400,
                bottom: 230,
            });

            let target_text = self
                .passive_learning_card
                .as_ref()
                .map(|c| c.target_text.clone())
                .or_else(|| self.passive_translation_target.clone())
                .unwrap_or_else(|| "当前表达".to_string());

            let primary_text = prediction
                .sentence
                .clone()
                .filter(|s| is_valid_english_text(s))
                .or_else(|| {
                    prediction
                        .alternatives
                        .first()
                        .map(|a| a.text.clone())
                        .filter(|s| is_valid_english_text(s))
                })
                .or_else(|| {
                    self.passive_learning_card
                        .as_ref()
                        .and_then(|c| c.primary.as_ref().map(|p| p.text.clone()))
                        .filter(|s| is_valid_english_text(s))
                })
                .or_else(|| {
                    self.passive_translation
                        .clone()
                        .filter(|s| is_valid_english_text(s))
                })
                .or_else(|| {
                    self.engine
                        .translate_word(&target_text)
                        .and_then(|t| t.senses().first().map(|s| s.text.clone()))
                        .filter(|s| is_valid_english_text(s))
                })
                .or_else(|| fallback_idiom_english(&target_text))
                .unwrap_or_default();

            let mut data = crate::ui::companion::CompanionData::default();
            data.target_chinese = target_text.clone();
            data.primary_english = primary_text.clone();
            data.is_loading = false;
            data.phonetic = prediction.phonetic.clone().unwrap_or_default();
            data.tone = prediction.tone.clone().unwrap_or_default();
            data.scenario = prediction.scenario.clone().unwrap_or_default();
            data.nuance = prediction.nuance.clone().or_else(|| prediction.explanation.clone()).unwrap_or_default();
            data.explanation = prediction.explanation.clone().unwrap_or_default();
            data.collocations = prediction
                .collocations
                .iter()
                .filter(|c| is_valid_english_text(c))
                .cloned()
                .collect();
            data.ielts_friendly = prediction.ielts_friendly.clone().unwrap_or_default();

            data.alternatives = prediction
                .alternatives
                .iter()
                .filter(|alt| is_valid_english_text(&alt.text))
                .map(|alt| crate::ui::companion::ExpressionVariant {
                    text: alt.text.clone(),
                    tag: if alt.tag.is_empty() { "日常口语".to_string() } else { alt.tag.clone() },
                    meaning: alt.meaning.clone(),
                    tone: alt.tone.clone(),
                    scenario: alt.scenario.clone(),
                    why_natural: alt.why_natural.clone(),
                    example_en: String::new(),
                    example_zh: String::new(),
                })
                .collect();


            data.dialogue = prediction
                .dialogue
                .iter()
                .map(|(s, t, z)| crate::ui::companion::DialogueTurn {
                    speaker: s.clone(),
                    text: t.clone(),
                    zh: z.clone(),
                })
                .collect();

            data.examples = prediction
                .examples
                .iter()
                .map(|ex| {
                    if let Some((en, zh)) = ex.split_once('|') {
                        (en.trim().to_string(), zh.trim().to_string())
                    } else if let Some((en, zh)) = ex.split_once('—') {
                        (en.trim().to_string(), zh.trim().to_string())
                    } else if let Some((en, zh)) = ex.split_once('-') {
                        (en.trim().to_string(), zh.trim().to_string())
                    } else {
                        (ex.trim().to_string(), String::new())
                    }
                })
                .filter(|(en, _zh)| is_valid_english_text(en))
                .collect();


            if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
                let vault = qingjian_learning::LearningVault::open(&user_dir);
                for saved in vault.saved_expressions() {
                    if saved.prompt == target_text {
                        if !data.collocations.contains(&saved.expression) {
                            data.collocations.push(saved.expression.clone());
                        }
                    }
                }
            }

            self.candidates.show_companion(data, rect);
            return;
        }
        if prediction.kind == PredictionKind::Translate {
            // Generation ID 校验：丢弃过期请求返回的结果，防止 Candidate 快速变化时的覆盖竞态
            if self.generation_id != 0 && prediction.sequence != self.generation_id {
                tracing::debug!(
                    seq = prediction.sequence,
                    gen = self.generation_id,
                    "丢弃过期的被动翻译结果 (generation mismatch)"
                );
                return;
            }
            let current_highlight_text = match &self.composed {
                Some(Composed::Candidates { layout, .. }) => {
                    layout.candidate(self.highlight).map(|c| c.text.clone())
                }
                _ => None,
            };
            let target = self.passive_translation_target.clone();
            if let (Some(curr), Some(target)) = (current_highlight_text, target) {
                if curr == target {
                    if let Some(text) = &prediction.sentence {
                        // 异常防发散保护：若目标短语 <= 10 汉字，译文超过 100 字符，判定为模型幻觉或串译发散，坚决丢弃
                        if target.chars().count() <= 10 && text.chars().count() > 100 {
                            tracing::warn!(target = %target, trans_len = text.len(), "丢弃长度严重失真的被动翻译异常");
                            if let Some(card) = self.passive_learning_card.as_mut() {
                                card.state = qingjian_platform::protocol::LearningState::Failed;
                            }
                            return;
                        }
                    }

                    self.passive_translation = prediction.sentence.clone();

                    let primary_expr = prediction.alternatives.first().cloned().map(|alt| {
                        qingjian_platform::protocol::LearningExpression {
                            text: alt.text,
                            tag: alt.tag,
                            meaning: alt.meaning,
                            tone: alt.tone,
                            scenario: alt.scenario,
                            commonality: alt.commonality,
                            why_natural: alt.why_natural,
                        }
                    }).or_else(|| {
                        prediction.sentence.as_ref().map(|s| {
                            qingjian_platform::protocol::LearningExpression {
                                text: s.clone(),
                                tag: "Natural".to_string(),
                                meaning: target.clone(),
                                tone: "Casual".to_string(),
                                scenario: "Daily conversation".to_string(),
                                commonality: "Common".to_string(),
                                why_natural: "Natural spoken English".to_string(),
                            }
                        })
                    });

                    let alternatives: Vec<qingjian_platform::protocol::LearningExpression> = prediction
                        .alternatives
                        .into_iter()
                        .map(|alt| qingjian_platform::protocol::LearningExpression {
                            text: alt.text,
                            tag: alt.tag,
                            meaning: alt.meaning,
                            tone: alt.tone,
                            scenario: alt.scenario,
                            commonality: alt.commonality,
                            why_natural: alt.why_natural,
                        })
                        .collect();

                    let preserve_expanded = self.passive_learning_card.as_ref().map(|c| c.expanded).unwrap_or(false);
                    let preserve_selected = self.passive_learning_card.as_ref().and_then(|c| c.selected_index);
                    let preserve_drill = self.passive_learning_card.as_ref().and_then(|c| c.drill_stage);
                    let preserve_input = self.passive_learning_card.as_ref().map(|c| c.drill_input.clone()).unwrap_or_default();
                    let preserve_attempt = self.passive_learning_card.as_ref().and_then(|c| c.drill_attempt.clone());
                    let preserve_notice = self.passive_learning_card.as_ref().and_then(|c| c.drill_notice.clone());

                    let due = self.today_due_count();
                    let upgrade = primary_expr
                        .as_ref()
                        .and_then(|p| qingjian_learning::find_upgrade(&target, &p.text));
                    self.passive_learning_card = Some(qingjian_platform::protocol::LearningCardFrame {
                        target_text: target.clone(),
                        state: if primary_expr.is_some() || !alternatives.is_empty() {
                            qingjian_platform::protocol::LearningState::Ready
                        } else {
                            qingjian_platform::protocol::LearningState::Failed
                        },
                        primary: primary_expr,
                        alternatives,
                        expanded: preserve_expanded,
                        selected_index: preserve_selected,
                        drill_stage: preserve_drill,
                        drill_input: preserve_input,
                        drill_attempt: preserve_attempt,
                        drill_notice: preserve_notice,
                        today_due_count: due,
                        review_current_index: 0,
                        review_total_count: due,
                        upgrade,
                    });

                    let translation_total_ms = self
                        .translation_start_time
                        .take()
                        .map(|t| t.elapsed().as_millis())
                        .unwrap_or(0);
                    let t_ui = Instant::now();
                    let shown = self.self_drawn_frame();
                    self.reconcile_candidates(&shown);

                    let ui_update_ms = t_ui.elapsed().as_millis();
                    tracing::info!(
                        target = %target,
                        translation_total_ms,
                        ui_update_ms,
                        "Translation端到端耗时 (T0->T7)"
                    );
                } else {
                    tracing::debug!(
                        "丢弃过期的被动翻译结果: 目标为 {:?}, 但当前高亮为 {:?}",
                        target,
                        curr
                    );
                }
            }
            return;
        }
        if let Some(Composed::Candidates { layout, .. }) = self.composed.as_mut() {
            let words: Vec<Candidate> = prediction
                .words
                .into_iter()
                .map(CloudWord::into_candidate)
                .collect();
            layout.set_cloud(words);
            self.sentence = prediction.sentence;
        }
    }

    pub(super) fn cancel_prediction(&mut self) {
        if self.engine.prediction_enabled() {
            self.engine.cancel_prediction();
        }
    }

    /// 高亮移动 `delta`，夹在 `[0, 末尾]`，到页边自然换页。
    pub(super) fn move_highlight(&mut self, delta: isize) {
        let count = self.candidate_count();
        if count == 0 {
            self.highlight = 0;
            return;
        }
        let next = (self.highlight as isize + delta).clamp(0, count as isize - 1) as usize;
        if next != self.highlight {
            self.navigated = true;
            self.highlight = next;
            self.on_candidate_selection_changed();
        }
    }

    /// 整页翻 `step`，高亮落到目标页第一个候选。
    pub(super) fn page(&mut self, step: isize) {
        let count = self.candidate_count();
        if count == 0 {
            self.highlight = 0;
            return;
        }
        let page_size = self.config.page_size;
        let page_count = count.div_ceil(page_size);
        let current = (self.highlight / page_size) as isize;
        let target = (current + step).clamp(0, page_count as isize - 1) as usize;
        if target != current as usize {
            self.navigated = true;
            self.engine.note_page_turn();
        }
        let next = (target * page_size).min(count - 1);
        if next != self.highlight {
            self.highlight = next;
            self.on_candidate_selection_changed();
        }
    }

    /// 高亮候选词变更时：若处于 Learn/Drill 模式或 Daily 显式查看英文，立即同步绑定新候选词，取消旧请求并发起新翻译。
    pub(super) fn on_candidate_selection_changed(&mut self) {
        let learning_on = (self.learning_mode == qingjian_platform::LearningMode::Learn
            || self.learning_mode == qingjian_platform::LearningMode::Drill
            || (self.learning_mode == qingjian_platform::LearningMode::Daily && self.daily_show_english))
            && !self.config.learning_language_off
            && self.engine.learning_language() != Language::Chinese
            && !self.is_private();
        if !learning_on {
            return;
        }
        let current_target = self.layout_candidate(self.highlight).map(|c| c.text);
        let Some(target) = current_target else {
            return;
        };

        if let Some(card) = &self.passive_learning_card {
            if card.target_text == target {
                return;
            }
        }

        let preserve_expanded = self.passive_learning_card.as_ref().map(|c| c.expanded).unwrap_or(false);

        self.passive_translation_target = Some(target.clone());
        if self.engine.prediction_enabled() {
            let due = self.today_due_count();

            // 优先从本地离线词典获取即时兜底，实现 0ms 瞬间显示，消除等待空白
            let mut offline_expr = None;
            if let Some(local_trans) = self.engine.translate_word(&target) {
                if let Some(first_sense) = local_trans.senses().first() {
                    offline_expr = Some(qingjian_platform::protocol::LearningExpression {
                        text: first_sense.text.clone(),
                        tag: "Natural".to_string(),
                        meaning: target.clone(),
                        tone: "Casual".to_string(),
                        scenario: "Daily".to_string(),
                        commonality: "Common".to_string(),
                        why_natural: "Local glossary".to_string(),
                    });
                }
            }
            let initial_state = if offline_expr.is_some() {
                qingjian_platform::protocol::LearningState::Ready
            } else {
                qingjian_platform::protocol::LearningState::Loading
            };

            self.passive_learning_card = Some(qingjian_platform::protocol::LearningCardFrame {
                target_text: target.clone(),
                state: initial_state,
                primary: offline_expr,
                alternatives: Vec::new(),
                expanded: preserve_expanded,
                selected_index: None,
                drill_stage: None,
                drill_input: String::new(),
                drill_attempt: None,
                drill_notice: None,
                today_due_count: due,
                review_current_index: 0,
                review_total_count: due,
                upgrade: None,
            });
            self.translation_start_time = Some(Instant::now());
            if let Some(seq) = self.engine.request_translation(&target) {
                self.generation_id = seq;
            }
        } else {
            self.generation_id += 1;
            self.passive_learning_card = None;
        }
    }

    /// 获取今日待复习总数
    pub(crate) fn today_due_count(&self) -> usize {
        if let Some(user_dir) = qingjian_platform::dirs::user_dir() {
            let vault = qingjian_learning::LearningVault::open(&user_dir);
            let now = jiff::Zoned::now().timestamp().as_second();
            vault.today_due_count(now)
        } else {
            0
        }
    }

    pub(super) fn candidate_count(&self) -> usize {
        match &self.composed {
            Some(Composed::Candidates { layout, .. }) => layout.len(),
            _ => 0,
        }
    }

    /// 候选布局里第 `index` 个（跨页下标）。
    pub(super) fn layout_candidate(&self, index: usize) -> Option<Candidate> {
        match &self.composed {
            Some(Composed::Candidates { layout, .. }) => layout.candidate(index).cloned(),
            _ => None,
        }
    }

    pub(super) fn commit_index(&mut self, index: usize) -> Option<String> {
        let candidate = self.layout_candidate(index)?;
        Some(self.engine.commit(&candidate))
    }

    /// 按当前状态生成一帧：翻译评审优先；没在组句给空帧；否则给高亮所在的那一页。
    /// 焦点会话的 DLL 比 Server 老时按老协议降级（见 [`Self::downgrade_for_old_dll`]）。
    pub(super) fn current_frame(&self) -> Frame {
        let mut frame = self.raw_frame();
        self.show_typed_keys(&mut frame);
        self.downgrade_for_old_dll(&mut frame);
        frame
    }

    /// 给 DLL 的帧只管应用输入框：双拼「输入框显示原始按键」开着时换成敲的键，拼音行由 Server 自绘照旧全拼。
    fn show_typed_keys(&self, frame: &mut Frame) {
        if self.translation.is_some() {
            return;
        }
        let Some(Composed::Candidates {
            typed_keys: Some(keys),
            ..
        }) = &self.composed
        else {
            return;
        };
        frame.preedit = vec![PreeditSegment {
            text: keys.text.clone(),
            kind: PreeditKind::Typed,
        }];
        frame.cursor = keys.cursor;
    }

    /// 自绘候选窗用的帧：不做老 DLL 降级，码段照常画。
    pub(super) fn self_drawn_frame(&self) -> Frame {
        self.raw_frame()
    }

    /// 协议比 Server 老的 DLL 不认识 `AuxCode` 段，收到会整条消息解析失败；给它的码段降级成普通拼音段。
    fn downgrade_for_old_dll(&self, frame: &mut Frame) {
        if self.focused_dll_protocol() >= PROTOCOL_VERSION {
            return;
        }
        for segment in &mut frame.preedit {
            if segment.kind == PreeditKind::AuxCode {
                segment.kind = PreeditKind::Typed;
            }
        }
    }

    /// 焦点会话的 DLL 协议版本；没有焦点会话时按最新（不必降级）。
    fn focused_dll_protocol(&self) -> u32 {
        self.focused
            .and_then(|session| self.sessions.get(&session))
            .map_or(PROTOCOL_VERSION, |info| info.protocol)
    }

    fn raw_frame(&self) -> Frame {
        if let Some(translation) = &self.translation {
            return self.translation_frame(translation);
        }
        if self.recall_session.is_some() {
            return self.recall_frame();
        }
        match &self.composed {
            None => Frame {
                notice: self.notice.clone(),
                learning_mode: self.learning_mode,
                ..Frame::default()
            },
            Some(Composed::Raw { text, cursor }) => Frame {
                preedit: vec![PreeditSegment {
                    text: text.clone(),
                    kind: PreeditKind::Typed,
                }],
                preedit_mode: self.config.preedit,
                cursor: *cursor,
                candidates: CandidateList { items: Vec::new() },
                highlight: usize::MAX,
                page: 0,
                page_count: 1,
                layout: self.config.layout,
                theme: self.config.theme,
                aux_code_show: self.config.aux_code_show,
                sentence: self.sentence.clone(),
                notice: self.notice.clone(),
                learning_mode: self.learning_mode,
                learning_card: None,
            },
            Some(Composed::Candidates {
                preedit,
                cursor,
                layout,
                ..
            }) => {
                let page_size = self.config.page_size;
                let highlight = self.highlight.min(layout.len().saturating_sub(1));
                let page = highlight / page_size;
                let items: Vec<Candidate> = layout
                    .page(page)
                    .into_iter()
                    .filter_map(|cell| cell.candidate().cloned())
                    .collect();
                let mut candidates = CandidateList { items };
                self.engine.annotate(&mut candidates);
                if let (Some(passive), Some(target)) = (&self.passive_translation, &self.passive_translation_target) {
                    if page == 0 {
                        if let Some(first) = candidates.items.first_mut() {
                            if first.text == *target && first.translation.is_none() {
                                first.translation = Some(Translation::new(
                                    self.engine.learning_language(),
                                    vec![Sense {
                                        part_of_speech: None,
                                        text: passive.clone(),
                                        reading: None,
                                        fresh: false,
                                    }],
                                ));
                            }
                        }
                    }
                }
                let learning_card = if !self.config.learning_language_off
                    && self.engine.learning_language() != Language::Chinese
                    && !self.is_private()
                {
                    self.passive_learning_card.clone()
                } else {
                    None
                };
                Frame {
                    preedit: preedit.clone(),
                    preedit_mode: self.config.preedit,
                    cursor: *cursor,
                    candidates,
                    highlight: highlight - page * page_size,
                    page,
                    page_count: layout.pages().max(1),
                    layout: self.config.layout,
                    theme: self.config.theme,
                    aux_code_show: self.config.aux_code_show,
                    sentence: self.sentence.clone(),
                    notice: self.notice.clone(),
                    learning_mode: self.learning_mode,
                    learning_card,
                }
            }
        }
    }
}

/// 一次查询的拼音行分段与光标，以及输入框另显示原始按键时的那一串。
/// 光标用 Core 的映射：自动补的 `'` 会让显示串比敲的长。
pub(super) fn marked_parts(query: &Query) -> (Vec<PreeditSegment>, usize, Option<TypedKeys>) {
    let preedit = query.marked_segments().iter().map(Into::into).collect();
    let typed_keys = query.shuangpin_raw_preedit.then(|| TypedKeys {
        text: query.marked_text(),
        cursor: query.marked_cursor(),
    });
    (preedit, query.segments_cursor(), typed_keys)
}

#[cfg(test)]
mod intent_trigger_tests {
    use super::*;

    #[test]
    fn intent_trigger_requires_a_long_string_without_an_exact_local_reading() {
        assert!(!should_request_intent(None, "nihao"));
        assert!(should_request_intent(None, "kaifazheh"));

        let mut query = Query::default();
        query.candidates.items.push(Candidate {
            text: "开发者".to_owned(),
            kind: qingjian_core::CandidateKind::Chinese,
            syllables: vec!["kai".into(), "fa".into(), "zhe".into()],
            reading: None,
            translation: None,
            aux_code: None,
        });
        assert!(!should_request_intent(Some(&query), "kaifazhe"));
        query.tail = "h".into();
        assert!(should_request_intent(Some(&query), "kaifazheh"));
    }
}

fn should_request_intent(query: Option<&Query>, typed: &str) -> bool {
    if !(7..=64).contains(&typed.len())
        || !typed.bytes().all(|byte| byte.is_ascii_lowercase())
    {
        return false;
    }
    let Some(query) = query else {
        return true;
    };
    if query.correction.is_some() {
        return false;
    }
    let has_exact_reading = query.candidates.items.iter().any(|candidate| {
        !candidate.syllables.is_empty() && candidate.syllables.concat() == typed
    });
    !has_exact_reading || !query.tail.is_empty()
}

/// 校验字符串是否为有效纯英文表达：必须包含 ASCII 英文字母，且绝不能包含任何汉字或 CJK 字符
pub(crate) fn is_valid_english_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let has_ascii_alphabetic = trimmed.chars().any(|c| c.is_ascii_alphabetic());
    if !has_ascii_alphabetic {
        return false;
    }
    let has_cjk = trimmed.chars().any(|c| {
        ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
            || ('\u{F900}'..='\u{FAFF}').contains(&c)
            || ('\u{3000}'..='\u{303F}').contains(&c)
            || ('\u{FF01}'..='\u{FF60}').contains(&c)
    });
    !has_cjk
}

/// 针对常见口语/习惯表达的高频地道美语兜底，杜绝生硬机翻或将中文原文回填到英文槽位
pub(crate) fn fallback_idiom_english(zh: &str) -> Option<String> {
    let trimmed = zh.trim();
    if trimmed.contains("动脑子")
        || trimmed.contains("动动脑子")
        || trimmed.contains("动脑了吗")
        || trimmed.contains("动脑")
    {
        return Some("Are you even thinking?".to_string());
    }
    if trimmed.contains("不经脑袋")
        || trimmed.contains("不经过脑袋")
        || trimmed.contains("不经大脑")
        || trimmed.contains("不过脑子")
        || trimmed.contains("说话不经")
    {
        return Some("They speak before they think.".to_string());
    }
    if trimmed.contains("有道理") || trimmed.contains("确实有道理") {
        return Some("That makes perfect sense.".to_string());
    }
    if trimmed.contains("我想吃饭") {
        return Some("I feel like having a bite.".to_string());
    }
    if trimmed.contains("比如") {
        return Some("For example".to_string());
    }
    None
}
