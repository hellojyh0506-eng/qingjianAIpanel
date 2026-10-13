//! 块级 Markdown 解析：逐行识别，不做嵌套与表格。

use super::block::Block;
use super::inline::parse_inline;

/// 把整段 Markdown 切成块。空行丢弃，没闭合的代码围栏当作代码块直到文末（流式输出到一半时常见）。
pub fn parse_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            match code.take() {
                Some(lines) => blocks.push(code_block(&lines)),
                None => code = Some(Vec::new()),
            }
            continue;
        }
        if let Some(lines) = code.as_mut() {
            lines.push(line);
            continue;
        }
        if trimmed.is_empty() || is_rule(trimmed) {
            continue;
        }
        blocks.push(parse_line(trimmed));
    }
    if let Some(lines) = code {
        blocks.push(code_block(&lines));
    }
    blocks
}

fn code_block(lines: &[&str]) -> Block {
    Block::Code {
        text: lines.join("\n"),
    }
}

/// 分割线（三个以上的 `-`、`*`、`_`）。
fn is_rule(line: &str) -> bool {
    (line.len() >= 3 && line.chars().all(|c| c == '-')) || line == "***" || line == "___"
}

fn parse_line(line: &str) -> Block {
    for (prefix, level) in [("### ", 3), ("## ", 2), ("# ", 1)] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Block::Heading {
                level,
                spans: parse_inline(rest.trim()),
            };
        }
    }
    if let Some(rest) = line.strip_prefix("> ") {
        return Block::Quote(parse_inline(rest.trim()));
    }
    for prefix in ["- ", "* ", "• "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Block::Bullet(parse_inline(rest.trim()));
        }
    }
    if let Some((number, rest)) = line.split_once(". ")
        && let Ok(number) = number.parse::<usize>()
    {
        return Block::Numbered {
            number,
            spans: parse_inline(rest.trim()),
        };
    }
    Block::Paragraph(parse_inline(line))
}

#[cfg(test)]
mod tests {
    use super::super::inline::Inline;
    use super::*;

    fn plain(text: &str) -> Vec<Inline> {
        vec![Inline::Plain(text.into())]
    }

    #[test]
    fn recognizes_every_block_kind() {
        let blocks = parse_blocks(
            "# 标题\n\n普通段落 **重点**\n- 第一项\n* 第二项\n1. 步骤\n> 引用\n```rust\nlet a = 1;\n  let b = 2;\n```\n结尾",
        );
        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 1,
                    spans: plain("标题")
                },
                Block::Paragraph(vec![
                    Inline::Plain("普通段落 ".into()),
                    Inline::Bold("重点".into())
                ]),
                Block::Bullet(plain("第一项")),
                Block::Bullet(plain("第二项")),
                Block::Numbered {
                    number: 1,
                    spans: plain("步骤")
                },
                Block::Quote(plain("引用")),
                Block::Code {
                    text: "let a = 1;\n  let b = 2;".into()
                },
                Block::Paragraph(plain("结尾")),
            ]
        );
    }

    #[test]
    fn unclosed_fence_is_a_code_block_to_the_end() {
        let blocks = parse_blocks("先看：\n```\nfn main() {");
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph(plain("先看：")),
                Block::Code {
                    text: "fn main() {".into()
                }
            ]
        );
    }

    #[test]
    fn heading_levels_and_rules() {
        let blocks = parse_blocks("### 小标题\n---\n## 二级");
        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 3,
                    spans: plain("小标题")
                },
                Block::Heading {
                    level: 2,
                    spans: plain("二级")
                },
            ]
        );
    }

    #[test]
    fn numbers_without_a_list_marker_stay_paragraphs() {
        assert_eq!(
            parse_blocks("3.14 is pi"),
            vec![Block::Paragraph(plain("3.14 is pi"))]
        );
    }
}
