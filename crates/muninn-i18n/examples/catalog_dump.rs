//! Dump the English and one shipped catalog side by side as TSV.
//!
//! Maintainer tooling only: run from a worktree that ships `tag`, e.g.
//! `cargo run -p muninn-i18n --example catalog_dump -- es`.

use muninn_i18n::{Key, Lang, MsgKey, phrase, t_in};

fn esc(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn main() {
    let argument = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: catalog_dump <bcp47-tag> | --list");
        std::process::exit(2);
    });
    if argument == "--list" {
        for &lang in Lang::SHIPPED {
            println!("{}\t{}", lang.tag(), lang.endonym());
        }
        return;
    }
    let lang = Lang::from_tag(&argument).unwrap_or_else(|| {
        eprintln!("{argument} is not shipped in this build; try --list");
        std::process::exit(2);
    });
    println!("kind\tid\tenglish\ttarget");
    for &key in Key::ALL {
        println!(
            "key\t{}\t{}\t{}",
            key.id(),
            esc(t_in(Lang::EnUs, key)),
            esc(t_in(lang, key))
        );
    }
    for &message in MsgKey::ALL {
        let args: Vec<String> = (0..message.arity())
            .map(|index| format!("\u{1}{index}\u{1}"))
            .collect();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        println!(
            "msg\t{}\t{}\t{}",
            message.id(),
            esc(&phrase(Lang::EnUs, message, &refs)),
            esc(&phrase(lang, message, &refs))
        );
    }
}
