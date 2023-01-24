#![allow(non_snake_case)]
mod markdown_body_css;
use markdown_body_css::*;

use dioxus::prelude::*;
use dioxus_desktop::Config;

use std::{env, fs};

struct AppProps {
    markdown_path: String,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    dioxus_desktop::launch_with_props(
        app,
        AppProps {
            markdown_path: args.get(1).unwrap().to_string(),
        },
        Config::default().with_custom_head(format!("<style>{}</style>", MARKDOWN_BODY_CSS)),
    );
}

#[inline_props]
pub fn Markdown<'a>(cx: Scope<'a>, content: &'a str) -> Element {
    let parser = &mut markdown_it::MarkdownIt::new();
    markdown_it::plugins::cmark::add(parser);
    markdown_it::plugins::extra::add(parser);
    markdown_it::plugins::html::add(parser);
    markdown_it::plugins::extra::add(parser);

    let html = parser.parse(content).render();
    cx.render(rsx! {
        div {
            dangerous_inner_html: "{html}"
        }
    })
}

fn app(cx: Scope<AppProps>) -> Element {
    let file_content: String = fs::read_to_string(&cx.props.markdown_path)
        .unwrap()
        .parse()
        .unwrap();

    cx.render(rsx! {
            div {
                class: "markdown-body",
                Markdown {
                    content: &use_state(&cx, || file_content)
                }
            }
        })
}
