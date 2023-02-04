#![allow(non_snake_case)]
mod markdown_body_css;
mod markdown_parser;

use dioxus::prelude::*;

use dioxus_desktop::*;
use fermi::*;
use markdown_body_css::*;
use markdown_it::parser::core::Root;
use markdown_parser::Spos;
use std::cmp::min;
use std::io;
use tokio::net::UnixListener;

use std::{env, fs, str};

static MARKDOWN_CONTENT: Atom<String> = |_| "".to_string();
static SOURCE_FOCUS_LINE: Atom<u32> = |_| 0;

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

fn find_spos(
    source_line: u32,
    sposes: &Vec<markdown_parser::Spos>,
) -> Option<markdown_parser::Spos> {
    if sposes.is_empty() {
        return None;
    }

    // Used in case when souce_lint is not in any spos range
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

#[inline_props]
pub fn Markdown(cx: Scope<'a>) -> Element {
    let con = use_read(cx, MARKDOWN_CONTENT);
    let source_line = use_read(cx, SOURCE_FOCUS_LINE);

    let parser = &mut markdown_parser::MarkdownParser::new();
    let ast = parser.parse(con);
    let mutroot = ast.cast::<Root>().unwrap();
    let spos_ext = mutroot.ext.get::<markdown_parser::SposesExt>().unwrap();

    let ss = find_spos(*source_line, &spos_ext.sposes);
    println!("find spos result: {:?}", ss);

    let eval = dioxus_desktop::use_eval(cx).clone();

    if let Some(s) = ss {
        cx.push_future(async move {
            let template = format!(
            "document.querySelector(`[data-spos='{}-{}']`).scrollIntoView({{behavior: 'smooth'}})",
            s.start_line, s.end_line);
            eval(template);
        });
    }

    let html = ast.render();
    cx.render(rsx! {
        div {
            dangerous_inner_html: "{html}"
        }
    })
}

fn spawn_unix_socket_listener(cx: &Scope<AppProps>) {
    cx.spawn({
        let setContent = use_set(cx, MARKDOWN_CONTENT).clone();
        let setFocusLine = use_set(cx, SOURCE_FOCUS_LINE).clone();
        // let desk = dc.clone();
        let file_content: String = fs::read_to_string(&cx.props.markdown_path)
            .unwrap()
            .parse()
            .unwrap();

        setContent(file_content.clone());

        let mut msg = vec![0; 10000];
        let _ = fs::remove_file("/tmp/crabix");
        let listener = UnixListener::bind("/tmp/crabix").unwrap();
        async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => loop {
                        let res = stream.readable().await;
                        if res.is_ok() {
                            match stream.try_read(&mut msg) {
                                Ok(0) => {
                                    println!("Connection closed");
                                    break;
                                }
                                Ok(n) => {
                                    let msgs = String::from(str::from_utf8(&msg[..n]).unwrap());
                                    // setContent(file_content.to_string());
                                    // setContent(file_content.clone());
                                    println!("MSGS: {:?}", msgs);
                                    let source_line = msgs.trim().parse::<u32>().unwrap();
                                    setFocusLine(source_line);
                                    // setContent(msgs);

                                    // markdownState.modify(|v| );
                                    // if str::from_utf8(&msg).unwrap().len() == 0 {}

                                    // static JsScroll: &str = r#"document.querySelector(`[data-sourcepos="138:1-138:14"]`).scrollIntoView({behavior: 'smooth'})"#;
                                    msg = vec![0; 10000];

                                    // msg.truncate(n);
                                    // break;
                                }
                                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    continue;
                                }
                                Err(e) => {
                                    println!("Exit from loop");
                                    return ();
                                }
                            }
                        }
                    },
                    Err(e) => {
                        println!("err");
                    }
                }
                println!("AFTER MATCH");
            }
        }
    });
}

fn app(cx: Scope<AppProps>) -> Element {
    println!("Run root element!");
    use_init_atom_root(cx);
    spawn_unix_socket_listener(&cx);

    cx.render(rsx! {
        rsx! {
            div {
            class: "markdown-body",
                Markdown {}
            }
        }
    })
}
