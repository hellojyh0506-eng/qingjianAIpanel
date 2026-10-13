//! 侧栏里一个会话的摘要。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,

    pub title: String,

    pub message_count: usize,

    /// 最后更新时间（Unix 秒）。
    pub updated_at: i64,
}

impl SessionSummary {
    /// 侧栏副标题里的日期，如 `10/24`（本地时区）。
    pub fn date_label(&self) -> String {
        jiff::Timestamp::from_second(self.updated_at)
            .map(|time| {
                time.to_zoned(jiff::tz::TimeZone::system())
                    .strftime("%m/%d")
                    .to_string()
            })
            .unwrap_or_default()
    }
}
