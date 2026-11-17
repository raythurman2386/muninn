//! The process-wide language seam.
//!
//! This is its own integration binary with **one** test because it mutates the
//! process default. A parallel test in the same binary could see it mid-change;
//! `tests/locale.rs` and the unit tests use explicit `t_in`/`phrase` calls or
//! their own thread override, so they do not read this global.

use muninn_i18n::{Lang, clear_lang, process_lang, set_lang, set_process_lang};

/// The one test here mutates the process default and the test thread's
/// override, then restores both.
#[test]
fn the_process_default_and_the_thread_override_agree() {
    let before_process = process_lang();
    let before_thread = Lang::current();
    assert_eq!(before_process, Lang::EnUs, "the process starts in English");
    assert_eq!(before_thread, before_process);

    // A thread override wins over the process default.
    set_process_lang(Lang::EnUs);
    set_lang(Lang::Pseudo);
    assert_eq!(Lang::current(), Lang::Pseudo);
    assert_eq!(process_lang(), Lang::EnUs, "the override is thread-local");

    // Clearing it follows the process default again.
    clear_lang();
    assert_eq!(Lang::current(), Lang::EnUs);
    set_process_lang(Lang::Pseudo);
    assert_eq!(
        Lang::current(),
        Lang::Pseudo,
        "a thread with no override follows the process"
    );
    set_lang(Lang::EnUs);
    assert_eq!(
        Lang::current(),
        Lang::EnUs,
        "the thread override wins again"
    );
    assert_eq!(process_lang(), Lang::Pseudo);

    // Restore the process for any later test that assumes English.
    set_process_lang(before_process);
    set_lang(before_thread);
}
