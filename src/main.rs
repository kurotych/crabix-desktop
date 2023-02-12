#![allow(non_snake_case)]
mod markdown_body_css;
mod markdown_parser;

use dioxus::prelude::*;
use dioxus_desktop::*;
use fermi::*;
use markdown_body_css::*;
use markdown_it::parser::core::Root;
use markdown_parser::{MarkdownParser, Spos, SposesExt};
use simple_logger::SimpleLogger;
use std::io;
use std::{env, fs, str};
use tokio::net::UnixListener;

static MARKDOWN_CONTENT: Atom<String> = |_| "".to_string();
static SOURCE_FOCUS_LINE: Atom<u32> = |_| 1;

struct AppProps {
    markdown_path: Option<String>,
}

fn main() {
    SimpleLogger::new().with_colors(true).init().unwrap();

    let args: Vec<String> = env::args().collect();

    let mut markdown_path = None;
    if args.len() >= 2 {
        markdown_path = Some(args.get(1).unwrap().to_string());
    }
    dioxus_desktop::launch_with_props(
        app,
        AppProps { markdown_path },
        Config::default()
            .with_custom_head(format!("<style>{}</style>", MARKDOWN_BODY_CSS))
            .with_window(WindowBuilder::new().with_title("Crabix Desktop")),
    );
}

#[inline_props]
pub fn Markdown(cx: Scope<'a>) -> Element {
    let con = use_read(cx, MARKDOWN_CONTENT);
    let source_line = use_read(cx, SOURCE_FOCUS_LINE);

    log::trace!("Parsing markdown");
    let parser = &mut MarkdownParser::new();
    let ast = parser.parse(con);
    let root_node = ast.cast::<Root>().unwrap();
    let spos_ext = root_node.ext.get::<SposesExt>().unwrap();
    log::trace!("Markdown parsed");

    let ss = Spos::find(*source_line, &spos_ext.sposes);
    let cs = *source_line;
    log::trace!("find spos result: {:?}", ss);

    let eval = dioxus_desktop::use_eval(cx).clone();

    if let Some(s) = ss {
        // Should be removed https://github.com/DioxusLabs/dioxus/issues/804
        cx.push_future(async move {
            let template = format!(
                r#"
            function calcOffset(height, spos_start, spos_end, current_pos) {{
                let steps = spos_end - spos_start;
                if (steps == 0) return 0;
                let step_value = height / steps;
                return (current_pos - spos_start) * step_value
            }}

            function scrollToElement(element) {{
              const rect = element.getBoundingClientRect();
              const elementTop = rect.top + window.pageYOffset;
              const elementMiddle = elementTop - (window.innerHeight / 2);
              const offset = calcOffset(rect.height, {spos_start}, {spos_end}, {current_pos})

              window.scrollTo({{
                  top: elementMiddle + offset,
                  left: 0,
                  behavior: 'smooth'
              }});
            }}
            setTimeout(function(){{
                const element = document.querySelector(`[data-spos='{spos_start}-{spos_end}']`);
                scrollToElement(element)
            }}, 100);

            "#,
                spos_start = s.start_line,
                spos_end = s.end_line,
                current_pos = cs
            );
            eval(template);
        });
    }

    let html = ast.render();
    cx.render(rsx! {
        div {
            class: "markdown-body",
            dangerous_inner_html: "{html}"
        }
    })
}

fn spawn_unix_socket_listener(cx: &Scope<AppProps>) {
    cx.spawn({
        let setContent = use_set(cx, MARKDOWN_CONTENT).clone();
        let setFocusLine = use_set(cx, SOURCE_FOCUS_LINE).clone();

        if let Some(markdown_path) = &cx.props.markdown_path {
            let file_content: String = fs::read_to_string(markdown_path).unwrap().parse().unwrap();
            setContent(file_content.clone());
        }

        // TODO Need to figure out max packet size
        let mut msg = vec![0; 1_000_000];
        let _ = fs::remove_file("/tmp/crabix");
        let listener = UnixListener::bind("/tmp/crabix").unwrap();
        let mut content = vec![];
        let mut total_bytes = 0;
        async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => loop {
                        log::trace!("Client connection accepted");
                        let res = stream.readable().await;
                        if res.is_ok() {
                            match stream.try_read(&mut msg) {
                                Ok(0) => {
                                    let msgs = String::from(
                                        str::from_utf8(&content[..total_bytes]).unwrap(),
                                    );
                                    let source_line_number_len =
                                        msgs.chars().take_while(|c| c.is_digit(10)).count();
                                    let (number, contentt) = msgs.split_at(source_line_number_len);
                                    log::trace!("Source line number: {:?}", number);

                                    setContent(contentt[1..].to_string());
                                    setFocusLine(number.parse::<u32>().unwrap());
                                    log::trace!("Connection closed");
                                    content.clear();
                                    total_bytes = 0;
                                    break;
                                }
                                Ok(n) => {
                                    log::trace!("Read {:?} bytes", n);
                                    total_bytes += n;
                                    content.extend(&msg[..n]);
                                }
                                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    continue;
                                }
                                Err(e) => {
                                    log::error!("{}", e);
                                    return ();
                                }
                            }
                        }
                    },
                    Err(e) => {
                        log::error!("{}", e);
                    }
                }
            }
        }
    });
}

fn app(cx: Scope<AppProps>) -> Element {
    log::trace!("Run root element!");
    use_init_atom_root(cx);
    spawn_unix_socket_listener(&cx);

    cx.render(rsx! {
            Markdown {}
    })
}
