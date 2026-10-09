//! 真实联网搜索与网页正文抓取（基于 Firecrawl CLI 工具）

use serde::Deserialize;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub description: String,
}

#[derive(Deserialize)]
struct FirecrawlSearchResponse {
    success: bool,
    data: Option<FirecrawlData>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct FirecrawlData {
    web: Option<Vec<FirecrawlWebItem>>,
}

#[derive(Deserialize)]
struct FirecrawlWebItem {
    title: Option<String>,
    url: Option<String>,
    description: Option<String>,
}

/// 执行真实联网搜索。若工具不存在或认证失败，返回友好的错误提示。
pub async fn execute_web_search(query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    let q = query.to_string();
    let lim = limit.to_string();

    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new("firecrawl");
        cmd.args(["search", &q, "--limit", &lim, "--json"]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let output = match cmd.output() {
            Ok(out) => out,
            Err(e) => {
                tracing::warn!(error = %e, "直接执行 firecrawl 失败，尝试 cmd /c firecrawl");
                let mut fallback = Command::new("cmd");
                fallback.args(["/c", "firecrawl", "search", &q, "--limit", &lim, "--json"]);
                fallback.stdout(Stdio::piped());
                fallback.stderr(Stdio::piped());
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                    fallback.creation_flags(CREATE_NO_WINDOW);
                }
                fallback.output().map_err(|err| format!("联网工具不可用: {}", err))?
            }
        };

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(status = ?output.status, stderr = %err_str, "firecrawl 退出异常");
        return Err(format!("联网搜索服务返回错误: {}", err_str.trim()));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let parsed: FirecrawlSearchResponse = serde_json::from_str(&stdout_str)
        .map_err(|e| format!("解析搜索响应 JSON 失败: {}", e))?;

    if !parsed.success {
        let msg = parsed.error.unwrap_or_else(|| "未知错误".to_string());
        return Err(format!("搜索未成功: {}", msg));
    }

    let mut results = Vec::new();
    if let Some(data) = parsed.data {
        if let Some(items) = data.web {
            for item in items {
                if let (Some(url), Some(title)) = (item.url, item.title) {
                    if !url.trim().is_empty() {
                        results.push(SearchResult {
                            title: title.trim().to_string(),
                            url: url.trim().to_string(),
                            description: item.description.unwrap_or_default().trim().to_string(),
                        });
                    }
                }
            }
        }
    }

        Ok(results)
    })
    .await
    .map_err(|e| format!("异步执行异常: {}", e))?
}
