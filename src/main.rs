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
static SOURCE_FOCUS_LINE: Atom<u32> = |_| 0;

struct AppProps {
    markdown_path: String,
}

fn main() {
    SimpleLogger::new().with_colors(true).init().unwrap();

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
    log::trace!("find spos result: {:?}", ss);

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
            class: "markdown-body",
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

        // TODO read /proc/sys/net/core/wmem_max to set size of this slice
        let mut msg = vec![0; 1_000_000];
        let _ = fs::remove_file("/tmp/crabix");
        let listener = UnixListener::bind("/tmp/crabix").unwrap();
        let mut content = vec![];
        let mut total_bytes = 0;
        async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => loop {
                        log::info!("Client connection accepted");
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
                                    log::info!("Connection closed");
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
    println!("Run root element!");
    use_init_atom_root(cx);
    spawn_unix_socket_listener(&cx);

    cx.render(rsx! {
            Markdown {}
    })
}
