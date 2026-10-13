//! 真实对话的场景。前三个是场景胶囊，后三个收在「更多场景」里。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scene {
    /// 美式日常。
    Daily,

    /// Gen Z 口语。
    GenZ,

    /// 雅思口语：题目 + 3 句高分回答。
    Ielts,

    /// 职场沟通。
    Work,

    /// 旅行出行。
    Travel,

    /// 校园生活。
    Campus,
}

impl Scene {
    pub const ALL: [Scene; 6] = [
        Scene::Daily,
        Scene::GenZ,
        Scene::Ielts,
        Scene::Work,
        Scene::Travel,
        Scene::Campus,
    ];

    /// 场景胶囊直接显示的个数，其余收进「更多场景」。
    pub const PRIMARY_COUNT: usize = 3;

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    /// 胶囊上的名字。
    pub fn label(self) -> &'static str {
        match self {
            Scene::Daily => "美式日常",
            Scene::GenZ => "Gen Z 口语",
            Scene::Ielts => "IELTS 口语",
            Scene::Work => "职场沟通",
            Scene::Travel => "旅行出行",
            Scene::Campus => "校园生活",
        }
    }

    /// 写进提示词的场景要求（英文，给模型看）。
    pub(super) fn guidance(self) -> &'static str {
        match self {
            Scene::Daily => {
                "everyday American conversation between friends or family; relaxed, natural spoken English"
            }
            Scene::GenZ => {
                "casual chat among young Americans; light, current informal spoken English, but no offensive slang and nothing that would sound forced"
            }
            Scene::Ielts => {
                "IELTS Speaking Part 1/2: an examiner question, then a candidate's 3-sentence high-scoring answer"
            }
            Scene::Work => {
                "workplace conversation between colleagues; polite, professional but not stiff"
            }
            Scene::Travel => "travel situations such as airports, hotels, restaurants, directions",
            Scene::Campus => "campus life: classmates, study groups, professors, deadlines",
        }
    }

    /// 雅思场景的结构不同：一道题 + 3 句回答，没有 A / B 轮流。
    pub fn is_ielts(self) -> bool {
        self == Scene::Ielts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trips_and_primary_scenes_come_first() {
        for scene in Scene::ALL {
            assert_eq!(Scene::from_index(scene.index()), Some(scene));
        }
        assert_eq!(Scene::ALL[Scene::PRIMARY_COUNT - 1], Scene::Ielts);
        assert!(Scene::Ielts.is_ielts() && !Scene::Daily.is_ielts());
    }
}
