use markdown_it::common::sourcemap::SourceWithLineStarts;
use markdown_it::parser::block::builtin::BlockParserRule;
use markdown_it::parser::core::{CoreRule, Root};
use markdown_it::parser::extset::RootExt;
use markdown_it::parser::inline::builtin::InlineParserRule;
use markdown_it::plugins::extra::syntect::{SyntectRule, SyntectSnippet};
use markdown_it::plugins::html::html_block::HtmlBlock;
use markdown_it::{MarkdownIt, Node};
use std::cmp::min;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Spos {
    pub start_line: u32,
    pub end_line: u32,
}

impl Spos {
    // Returns the closes to source_line Spos element that is used as attribute in HTML result
    pub fn find(source_line: u32, sposes: &Vec<Spos>) -> Option<Spos> {
        if sposes.is_empty() {
            return None;
        }

        // In case when souce_line is not in any spos range
        let mut closest_element: Option<Spos> = None;
        let mut closest_el_delta = u32::MAX;

        let mut spos_res: Option<Spos> = None;
        let mut spos_delta = u32::MAX;

        for s in sposes {
            if s.start_line <= source_line && s.end_line >= source_line {
                let delta = s.end_line - s.start_line;
                if delta < spos_delta {
                    spos_res = Some(Spos {
                        start_line: s.start_line,
                        end_line: s.end_line,
                    });
                    spos_delta = delta;
                }
            }
            if s.start_line.abs_diff(source_line) < closest_el_delta
                || s.end_line.abs_diff(source_line) < closest_el_delta
            {
                closest_element = Some(Spos {
                    start_line: s.start_line,
                    end_line: s.end_line,
                });
                closest_el_delta = min(
                    s.start_line.abs_diff(source_line),
                    s.end_line.abs_diff(source_line),
                );
            } else if (s.start_line.abs_diff(source_line) == closest_el_delta
                || s.end_line.abs_diff(source_line) == closest_el_delta)
                && closest_element.is_some()
            {
                // There are two competitors that have the same delta,
                // So we choose an element, that have lower delta between its end_line and start_line
                let closest_el = closest_element.unwrap();
                let closest_el_diff = closest_el.end_line - closest_el.start_line;
                let candidate_diff = s.end_line - s.start_line;
                if candidate_diff < closest_el_diff {
                    closest_element = Some(Spos {
                        start_line: s.start_line,
                        end_line: s.end_line,
                    });
                }
            }
        }

        if spos_res.is_some() {
            return spos_res;
        }
        if closest_element.is_some() {
            return closest_element;
        }
        return None;
    }
}

#[derive(Debug, Clone)]
pub struct SposesExt {
    pub sposes: Vec<Spos>,
}

impl RootExt for SposesExt {}

pub struct MarkdownParser {
    parserEngine: MarkdownIt,
}

fn add(md: &mut MarkdownIt) {
    md.add_rule::<SyntaxPosRule>()
        .after::<BlockParserRule>()
        .after::<InlineParserRule>()
        .after::<SyntectRule>();
}

impl MarkdownParser {
    pub fn new() -> Self {
        let mut parser = markdown_it::MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut parser);
        markdown_it::plugins::html::add(&mut parser);
        markdown_it::plugins::extra::add(&mut parser);
        add(&mut parser);
        MarkdownParser {
            parserEngine: parser,
        }
    }

    pub fn parse(&mut self, src: &str) -> Node {
        self.parserEngine.parse(src)
    }
}

#[doc(hidden)]
pub struct SyntaxPosRule;
impl CoreRule for SyntaxPosRule {
    fn run(root: &mut Node, _: &MarkdownIt) {
        let source = root.cast::<Root>().unwrap().content.as_str();
        let mapping = SourceWithLineStarts::new(source);

        let mut sposes: Vec<Spos> = vec![];
        root.walk_mut(|node, _| {
            if let Some(map) = node.srcmap {
                if node.node_type.name == "markdown_it::plugins::extra::syntect::SyntectSnippet" {
                    // As improvement we can fork/rewrite SyntectSnippet plugin ad paste data-spos
                    // there to avoid redundant copy/replace operations
                    if let Some(ss) = node.node_value.as_any().downcast_ref::<SyntectSnippet>() {
                        let ((start_line, _startcol), (end_line, _endcol)) =
                            map.get_positions(&mapping);

                        if !ss.html.starts_with("<pre ") {
                            panic!("Unexpected Syntect Snippet result: {:?}", ss);
                        }
                        let selector: String =
                            format!("data-spos=\"{}-{}\" ", start_line, end_line);

                        let mut from_result = String::from(&ss.html[0..=4]);
                        from_result.push_str(&selector);
                        from_result.push_str(&ss.html[5..]);
                        node.replace(SyntectSnippet { html: from_result });

                        sposes.push(Spos {
                            start_line,
                            end_line,
                        });
                    } else {
                        panic!("downcast_ref for SyntectSnippet is failed");
                    }
                } else if node.node_type.name == "markdown_it::plugins::html::html_block::HtmlBlock"
                {
                    // HTML in markdown is pain in the ass
                    // If html inline rendering has artifacts, try to remove this code
                    if let Some(ss) = node.node_value.as_any().downcast_ref::<HtmlBlock>() {
                        let start_html_tag_index =
                            ss.content.chars().take_while(|c| *c != '<').count();
                        if ss.content.len() <= start_html_tag_index + 1 {
                            return;
                        }

                        if !ss
                            .content
                            .chars()
                            .nth(start_html_tag_index + 1)
                            .unwrap()
                            .is_alphabetic()
                        {
                            return;
                        }

                        let last_index_of_html_tag = ss.content[start_html_tag_index + 1..]
                            .chars()
                            .take_while(|c| c.is_alphabetic())
                            .count()
                            + start_html_tag_index;

                        let ((start_line, _startcol), (end_line, _endcol)) =
                            map.get_positions(&mapping);
                        let selector: String =
                            format!(" data-spos=\"{}-{}\"", start_line, end_line);
                        let mut html_result =
                            String::with_capacity(ss.content.len() + selector.len() + 42);
                        html_result.push_str(&ss.content[..=last_index_of_html_tag]);
                        html_result.push_str(&selector);
                        html_result.push_str(&ss.content[last_index_of_html_tag + 1..]);
                        node.replace(HtmlBlock {
                            content: html_result,
                        });

                        sposes.push(Spos {
                            start_line,
                            end_line,
                        });
                    } else {
                        panic!("downcast_ref for HtmlBlock is failed");
                    }
                } else {
                    if node.node_type.name == "markdown_it::parser::core::root::Root"
                        || node.node_type.name
                            == "markdown_it::parser::inline::builtin::skip_text::Text"
                    {
                        // root position quite clear. First line and last line :)
                        // Text is always inside higher html tag
                    } else {
                        let ((start_line, _startcol), (end_line, _endcol)) =
                            map.get_positions(&mapping);
                        let selector: String = format!("{}-{}", start_line, end_line);
                        node.attrs.push(("data-spos", selector));
                        sposes.push(Spos {
                            start_line,
                            end_line,
                        });
                    }
                }
            }
        });
        let mutrut = root.cast_mut::<Root>().unwrap();
        mutrut.ext.insert(SposesExt { sposes });
    }
}

#[cfg(test)]
mod tests {
    use crate::markdown_parser::{MarkdownParser, Spos};

    fn spos(start_line: u32, end_line: u32) -> Spos {
        Spos {
            start_line,
            end_line,
        }
    }

    #[test]
    fn spos_test() {
        let sposes = vec![spos(3, 4), spos(4, 4), spos(6, 7)];
        assert_eq!(Spos::find(5, &sposes).unwrap(), spos(4, 4));
        assert_eq!(Spos::find(6, &sposes).unwrap(), spos(6, 7));

        let sposes = vec![spos(4, 4), spos(3, 4), spos(6, 6)];
        assert_eq!(Spos::find(5, &sposes).unwrap(), spos(4, 4));
    }

    #[test]
    fn header_test() {
        let parser = &mut MarkdownParser::new();
        let html = parser.parse("# hello").render();
        assert_eq!(html.trim(), r#"<h1 data-spos="1-1">hello</h1>"#);
    }

    #[test]
    fn pre_block_test() {
        let parser = &mut MarkdownParser::new();
        let html = parser
            .parse(
                r#"```rust
    fn app(cx: Scope) -> Element {}
```"#,
            )
            .render();
        assert!(html.starts_with("<pre data-spos=\"1-3\" "));
    }

    #[test]
    fn html_component_in_markdown() {
        let parser = &mut MarkdownParser::new();
        let html = parser
            .parse(
                r#"
<!--This is a comment. Comments are not displayed in the browser-->   
<p align = "left">
Metus sapien molestie cursus sollicitudin vivamus dignissim condimentum pretium velit.
</p>
<!--This is a comment. Comments are not displayed in the browser-->   
"#,
            )
            .render();
        assert!(html.starts_with(
            r#"<!--This is a comment. Comments are not displayed in the browser-->   
<p data-spos="3-6" align = "left">
Metus sapien molestie cursus sollicitudin vivamus dignissim condimentum pretium velit.
</p>
<!--This is a comment. Comments are not displayed in the browser-->   
"#
        ));
    }

    #[test]
    fn html_component_with_space() {
        let parser = &mut MarkdownParser::new();
        let html = parser
            .parse(
                r#"<!--This is a comment. Comments are not displayed in the browser-->   
<p align = "left">
Metus sapien molestie cursus sollicitudin vivamus dignissim condimentum pretium velit.

</p>
<!--This is a comment. Comments are not displayed in the browser-->   
"#,
            )
            .render();
        assert_eq!(
            html,
            r#"<!--This is a comment. Comments are not displayed in the browser-->   
<p data-spos="2-3" align = "left">
Metus sapien molestie cursus sollicitudin vivamus dignissim condimentum pretium velit.
</p>
<!--This is a comment. Comments are not displayed in the browser-->   
"#
        );
    }

    #[test]
    fn html_component_with_space2() {
        let parser = &mut MarkdownParser::new();
        let html = parser
            .parse(
                r#"
<p align = "left">
Metus sapien molestie cursus sollicitudin vivamus dignissim condimentum pretium velit.

</p>
<p align = "left">
Metus sapien molestie cursus sollicitudin vivamus dignissim condimentum pretium velit.
</p>

# Some text
"#,
            )
            .render();
        assert_eq!(
            html,
            r#"<p data-spos="2-3" align = "left">
Metus sapien molestie cursus sollicitudin vivamus dignissim condimentum pretium velit.
</p>
<p align = "left">
Metus sapien molestie cursus sollicitudin vivamus dignissim condimentum pretium velit.
</p>
<h1 data-spos="10-10">Some text</h1>
"#
        );
    }

    #[test]
    fn html_component_with_space3() {
        let parser = &mut MarkdownParser::new();
        let html = parser
            .parse(
                r#"
  <a href="https://github.com/jkelleyrtp/dioxus/actions">
    <img src="https://github.com/dioxuslabs/dioxus/actions/workflows/main.yml/badge.svg"
      alt="CI status" />
  </a>

  <!--Awesome -->
  <a href="https://github.com/dioxuslabs/awesome-dioxus">
    <img src="https://cdn.rawgit.com/sindresorhus/awesome/d7305f38d29fed78fa85652e3a63e154dd8e8829/media/badge.svg" alt="Awesome Page" />
  </a>
"#,
            )
            .render();
        assert_eq!(
            html,
            r#"  <a data-spos="2-5" href="https://github.com/jkelleyrtp/dioxus/actions">
    <img src="https://github.com/dioxuslabs/dioxus/actions/workflows/main.yml/badge.svg"
      alt="CI status" />
  </a>
  <!--Awesome -->
  <a data-spos="8-10" href="https://github.com/dioxuslabs/awesome-dioxus">
    <img src="https://cdn.rawgit.com/sindresorhus/awesome/d7305f38d29fed78fa85652e3a63e154dd8e8829/media/badge.svg" alt="Awesome Page" />
  </a>
"#
        );
    }
}
